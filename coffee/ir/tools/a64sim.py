#!/usr/bin/env python3
"""Runs the code of an aarch64 ELF object (made by `coffee2 --target=aarch64-linux --emit-obj`) in a
small interpreter, for machines with no aarch64 hardware and no qemu. It decodes with llvm-objdump
(so the encodings are checked by LLVM's decoder, not by this file) and executes the subset of
instructions the code generator emits. Calls to the C library are done by Python versions of
malloc, free, memcpy, strlen, printf, ...

Usage: a64sim.py file.o [--entry main] [--max-steps N]
Prints what the program wrote to stdout; the exit status is `main`'s result, as for a program.
"""
import re
import struct
import subprocess
import sys

M64 = (1 << 64) - 1


def objdump(args, path):
    return subprocess.run(['llvm-objdump'] + args + [path], capture_output=True, text=True, check=True).stdout


class Sim:
    def __init__(self, path, max_steps=200_000_000):
        self.max_steps = max_steps
        self.code = {}      # address -> (mnemonic, [operands], relocation or None)
        self.syms = {}      # function / data symbol -> address
        self.sec_base = {}
        self.mem = {}       # byte memory
        self.out = bytearray()
        self.load(path)
        self.x = [0] * 31
        self.sp = 0x7ff00000
        self.v = [0] * 32
        self.n = self.z = self.c = self.vf = 0
        self.heap = 0x10000000

    # ---- loading ----------------------------------------------------------
    def load(self, path):
        base = 0x400000
        text = objdump(['-dr', '--no-show-raw-insn'], path)
        sec = None
        cur = None
        pending = {}
        for line in text.splitlines():
            m = re.match(r'Disassembly of section (\S+):', line)
            if m:
                sec = m.group(1)
                self.sec_base[sec] = base
                base += 0x100000
                continue
            m = re.match(r'([0-9a-f]+) <(.+)>:', line)
            if m and sec:
                self.syms[m.group(2)] = self.sec_base[sec] + int(m.group(1), 16)
                continue
            m = re.match(r'\s+([0-9a-f]+):\s+(\S+)\s*(.*)', line)
            if m and sec and not line.strip().startswith('0000'):
                addr = self.sec_base[sec] + int(m.group(1), 16)
                mn = m.group(2)
                ops = split_ops(re.sub(r'\s*//.*$', '', m.group(3)))
                self.code[addr] = [mn, ops, None, self.sec_base[sec]]
                cur = addr
                continue
            m = re.match(r'\s+([0-9a-f]+):\s+(R_AARCH64_\w+)\s+(\S+)', line)
            if m and sec:
                addr = self.sec_base[sec] + int(m.group(1), 16)
                pending[addr] = (m.group(2), m.group(3))
        for a, r in pending.items():
            if a in self.code:
                self.code[a][2] = r
        # read-only data and globals
        self.rodata = 0x20000000
        ro = objdump(['-s', '-j', '.rodata'], path)
        for line in ro.splitlines():
            m = re.match(r' ([0-9a-f]+) ((?:[0-9a-f]+ ?){1,4})', line)
            if m:
                off = int(m.group(1), 16)
                hexs = m.group(2).replace(' ', '')
                for i in range(0, len(hexs), 2):
                    self.mem[self.rodata + off + i // 2] = int(hexs[i:i + 2], 16)
        self.syms['.rodata'] = self.rodata
        gl = 0x30000000
        for line in objdump(['-t'], path).splitlines():
            m = re.match(r'([0-9a-f]+) (\w+)\s+(\w+)\s+(\S+)\s+([0-9a-f]+)\s+(\S+)$', line)
            if m and m.group(4) == '.data' and m.group(3) == 'O':
                self.syms[m.group(6)] = gl
                gl += max(8, (int(m.group(5), 16) + 15) // 16 * 16)

    # ---- state helpers ----------------------------------------------------
    def rd(self, r, w=64):
        if r in ('xzr', 'wzr'):
            return 0
        if r == 'sp':
            v = self.sp
        elif r[0] in 'xw':
            v = self.x[int(r[1:])]
        else:
            raise ValueError(r)
        return v & ((1 << w) - 1) if r[0] == 'x' and w == 64 else v & 0xffffffff if r[0] == 'w' else v & M64

    def wr(self, r, v):
        if r in ('xzr', 'wzr'):
            return
        if r == 'sp':
            self.sp = v & M64
        elif r[0] == 'x':
            self.x[int(r[1:])] = v & M64
        elif r[0] == 'w':
            self.x[int(r[1:])] = v & 0xffffffff
        else:
            raise ValueError(r)

    def load_n(self, a, n):
        return sum(self.mem.get(a + i, 0) << (8 * i) for i in range(n))

    def store_n(self, a, n, v):
        for i in range(n):
            self.mem[a + i] = (v >> (8 * i)) & 255

    def imm(self, s):
        return int(s.lstrip('#'), 0)

    def operand_value(self, s, w=64):
        s = s.strip()
        if s.startswith('#'):
            return self.imm(s) & M64
        return self.rd(s)

    def addr(self, s):
        # [xn], [xn, #imm], [xn, xm], [xn, xm, lsl #n]
        inner = s.strip()[1:-1]
        parts = [p.strip() for p in inner.split(',')]
        b = self.rd(parts[0])
        if len(parts) == 1:
            return b
        if parts[1].startswith('#'):
            return (b + self.imm(parts[1])) & M64
        off = self.rd(parts[1])
        if len(parts) > 2:
            m = re.match(r'lsl #(\d+)', parts[2])
            if m:
                off <<= int(m.group(1))
        return (b + off) & M64

    def sym_addr(self, rel):
        kind, name = rel
        m = re.match(r'(.+?)([+-]0x[0-9a-f]+)?$', name)
        sym, add = m.group(1), int(m.group(2), 16) if m.group(2) else 0
        return sym, add

    # ---- flags ------------------------------------------------------------
    def set_sub(self, a, b):
        r = (a - b) & M64
        self.n = r >> 63
        self.z = int(r == 0)
        self.c = int(a >= b)
        sa, sb, sr = a >> 63, b >> 63, r >> 63
        self.vf = int(sa != sb and sr != sa)
        return r

    def set_add(self, a, b):
        t = a + b
        r = t & M64
        self.n = r >> 63
        self.z = int(r == 0)
        self.c = int(t > M64)
        sa, sb, sr = a >> 63, b >> 63, r >> 63
        self.vf = int(sa == sb and sr != sa)
        return r

    def cond(self, cc):
        n, z, c, v = self.n, self.z, self.c, self.vf
        return {
            'eq': z == 1, 'ne': z == 0, 'hs': c == 1, 'lo': c == 0, 'cs': c == 1, 'cc': c == 0,
            'mi': n == 1, 'pl': n == 0, 'vs': v == 1, 'vc': v == 0,
            'hi': c == 1 and z == 0, 'ls': not (c == 1 and z == 0),
            'ge': n == v, 'lt': n != v, 'gt': z == 0 and n == v, 'le': not (z == 0 and n == v),
            'al': True,
        }[cc]

    # ---- floats -----------------------------------------------------------
    def fget(self, r):
        i = int(r[1:])
        if r[0] == 'd':
            return struct.unpack('<d', struct.pack('<Q', self.v[i] & M64))[0]
        return struct.unpack('<f', struct.pack('<I', self.v[i] & 0xffffffff))[0]

    def fset(self, r, f):
        i = int(r[1:])
        if r[0] == 'd':
            self.v[i] = struct.unpack('<Q', struct.pack('<d', f))[0]
        else:
            self.v[i] = struct.unpack('<I', struct.pack('<f', f))[0]

    # ---- C library --------------------------------------------------------
    def cstr(self, a):
        s = bytearray()
        while self.mem.get(a, 0):
            s.append(self.mem[a])
            a += 1
        return bytes(s)

    def alloc(self, n):
        a = self.heap
        self.heap += (n + 15) // 16 * 16 + 16
        return a

    def libc(self, name):
        x = self.x
        if name == 'malloc':
            x[0] = self.alloc(x[0])
        elif name == 'calloc':
            x[0] = self.alloc(x[0] * x[1])
        elif name == 'free':
            pass
        elif name == 'realloc':
            n = self.alloc(x[1])
            for i in range(x[1]):
                self.mem[n + i] = self.mem.get(x[0] + i, 0)
            x[0] = n
        elif name == 'memcpy' or name == 'memmove':
            d, s, n = x[0], x[1], x[2]
            data = [self.mem.get(s + i, 0) for i in range(n)]
            for i in range(n):
                self.mem[d + i] = data[i]
        elif name == 'memset':
            for i in range(x[2]):
                self.mem[x[0] + i] = x[1] & 255
        elif name == 'strlen':
            x[0] = len(self.cstr(x[0]))
        elif name == 'strcmp':
            a, b = self.cstr(x[0]), self.cstr(x[1])
            x[0] = ((a > b) - (a < b)) & M64
        elif name == 'strdup':
            s = self.cstr(x[0])
            n = self.alloc(len(s) + 1)
            for i, ch in enumerate(s + b'\0'):
                self.mem[n + i] = ch
            x[0] = n
        elif name == 'putchar':
            self.out.append(x[0] & 255)
        elif name == 'puts':
            self.out += self.cstr(x[0]) + b'\n'
            x[0] = 0
        elif name == 'write':
            if x[0] == 1:
                self.out += bytes(self.mem.get(x[1] + i, 0) for i in range(x[2]))
        elif name == 'printf':
            self.out += self.printf()
            x[0] = 0
        elif name == 'fmod':
            import math
            self.fset('d0', math.fmod(self.fget('d0'), self.fget('d1')))
        elif name == 'exit':
            raise SystemExit(x[0] & 255)
        else:
            raise RuntimeError('unknown C function ' + name)

    def printf(self):
        fmt = self.cstr(self.x[0]).decode('latin1')
        ints = list(self.x[1:8])
        flts = [self.fget('d%d' % i) for i in range(8)]
        out = []
        i = 0
        while i < len(fmt):
            ch = fmt[i]
            if ch != '%':
                out.append(ch)
                i += 1
                continue
            m = re.match(r'%([-0 +#]*)(\d*)(?:\.(\d+))?(l|ll|h|hh|z)?([diuxXcsfgeEp%])', fmt[i:])
            if not m:
                out.append(ch)
                i += 1
                continue
            fl, wd, pr, ln, cv = m.groups()
            i += len(m.group(0))
            if cv == '%':
                out.append('%')
                continue
            spec = '%' + fl + wd + ('.' + pr if pr else '')
            if cv in 'fgeE':
                out.append((spec + cv) % flts.pop(0))
            elif cv == 's':
                out.append((spec + 's') % self.cstr(ints.pop(0)).decode('latin1'))
            elif cv == 'c':
                out.append(chr(ints.pop(0) & 255))
            else:
                v = ints.pop(0)
                if cv in 'di':
                    if ln in ('l', 'll', 'z'):
                        v = v - (1 << 64) if v >> 63 else v
                    else:
                        v &= 0xffffffff
                        v = v - (1 << 32) if v >> 31 else v
                    out.append((spec + 'd') % v)
                else:
                    v &= M64 if ln in ('l', 'll', 'z') else 0xffffffff
                    out.append((spec + ('d' if cv == 'u' else cv)) % v if cv != 'p' else hex(v))
        return ''.join(out).encode('latin1')

    # ---- execution --------------------------------------------------------
    def call(self, name, args=()):
        for i, a in enumerate(args):
            self.x[i] = a & M64
        self.x[30] = 0xdead0000
        pc = self.syms[name]
        steps = 0
        while pc != 0xdead0000:
            steps += 1
            if steps > self.max_steps:
                raise RuntimeError('step limit')
            if pc not in self.code:
                raise RuntimeError('pc outside the code: %x' % pc)
            pc = self.step(pc)
        return self.x[0]

    def step(self, pc):
        mn, ops, rel, base = self.code[pc]
        nxt = pc + 4
        x = self

        def target(s):
            m = re.match(r'(?:#)?(0x[0-9a-f]+|\d+)', s)
            return base + int(m.group(1), 0)

        if mn == 'bl':
            sym, add = self.sym_addr(rel)
            if sym in self.syms:
                self.x[30] = nxt
                return self.syms[sym] + add
            self.libc(sym)
            return nxt
        if mn == 'blr':
            self.x[30] = nxt
            return self.rd(ops[0])
        if mn == 'b':
            return target(ops[0])
        if mn.startswith('b.'):
            return target(ops[0]) if self.cond(mn[2:]) else nxt
        if mn == 'ret':
            return self.x[30]
        if mn == 'brk':
            raise RuntimeError('brk (unreachable) at %x' % pc)
        if mn == 'adrp':
            sym, add = self.sym_addr(rel)
            if sym not in self.syms:
                raise RuntimeError('unknown symbol ' + sym)
            self.wr(ops[0], self.syms[sym] + add)
            return nxt
        if mn == 'add' and rel is not None:
            return nxt  # the low 12 bits: adrp already made the whole address
        self.exec(mn, ops)
        return nxt

    def reg_w(self, r):
        return 32 if r[0] in 'ws' and r != 'sp' else 64

    def exec(self, mn, ops):
        rd = self.rd
        wr = self.wr
        if mn in ('add', 'sub', 'adds', 'subs', 'cmp', 'cmn'):
            if mn in ('cmp', 'cmn'):
                ops = ['xzr'] + ops
            d, a = ops[0], ops[1]
            b = ops[2]
            av = rd(a)
            if b.startswith('#'):
                bv = self.imm(b) & M64
                if len(ops) > 3:
                    bv <<= int(re.search(r'#(\d+)', ops[3]).group(1))
            else:
                bv = rd(b)
                if len(ops) > 3:
                    m = re.match(r'lsl #(\d+)', ops[3])
                    if m:
                        bv = (bv << int(m.group(1))) & M64
            sub = mn in ('sub', 'subs', 'cmp')
            if mn in ('adds', 'subs', 'cmp', 'cmn'):
                r = self.set_sub(av, bv) if sub else self.set_add(av, bv)
            else:
                r = (av - bv) & M64 if sub else (av + bv) & M64
            if mn not in ('cmp', 'cmn'):
                wr(d, r)
            return
        if mn == 'mov':
            d, s = ops
            if s.startswith('#'):
                wr(d, self.imm(s) & (M64 if d[0] == 'x' else 0xffffffff))
            else:
                wr(d, rd(s))
            return
        if mn == 'movk':
            d, s, sh = ops[0], ops[1], ops[2] if len(ops) > 2 else 'lsl #0'
            n = int(re.search(r'#(\d+)', sh).group(1))
            v = rd(d)
            wr(d, (v & ~(0xffff << n)) | (self.imm(s) << n))
            return
        if mn in ('and', 'orr', 'eor'):
            a, b = rd(ops[1]), rd(ops[2])
            wr(ops[0], {'and': a & b, 'orr': a | b, 'eor': a ^ b}[mn])
            return
        if mn == 'mvn':
            wr(ops[0], ~rd(ops[1]))
            return
        if mn == 'neg':
            wr(ops[0], -rd(ops[1]))
            return
        if mn == 'mul':
            wr(ops[0], rd(ops[1]) * rd(ops[2]))
            return
        if mn == 'madd':
            wr(ops[0], rd(ops[3]) + rd(ops[1]) * rd(ops[2]))
            return
        if mn == 'msub':
            wr(ops[0], rd(ops[3]) - rd(ops[1]) * rd(ops[2]))
            return
        if mn in ('sdiv', 'udiv'):
            a, b = rd(ops[1]), rd(ops[2])
            if mn == 'sdiv':
                a = a - (1 << 64) if a >> 63 else a
                b = b - (1 << 64) if b >> 63 else b
                q = 0 if b == 0 else abs(a) // abs(b) * (1 if (a < 0) == (b < 0) else -1)
            else:
                q = 0 if b == 0 else a // b
            wr(ops[0], q)
            return
        if mn == 'smulh':
            a, b = rd(ops[1]), rd(ops[2])
            a = a - (1 << 64) if a >> 63 else a
            b = b - (1 << 64) if b >> 63 else b
            wr(ops[0], (a * b) >> 64)
            return
        if mn in ('lsl', 'lsr', 'asr'):
            a = rd(ops[1])
            s = self.imm(ops[2]) if ops[2].startswith('#') else rd(ops[2]) & 63
            if mn == 'lsl':
                r = a << s
            elif mn == 'lsr':
                r = a >> s
            else:
                r = (a - (1 << 64) if a >> 63 else a) >> s
            wr(ops[0], r)
            return
        if mn in ('sxtb', 'sxth', 'sxtw', 'uxtb', 'uxth', 'uxtw'):
            n = {'b': 8, 'h': 16, 'w': 32}[mn[3]]
            v = rd(ops[1]) & ((1 << n) - 1)
            if mn[0] == 's' and v >> (n - 1):
                v -= 1 << n
            wr(ops[0], v)
            return
        if mn == 'cset':
            wr(ops[0], int(self.cond(ops[1])))
            return
        if mn in ('stp', 'ldp'):
            a = self.addr(ops[2])
            if mn == 'stp':
                self.store_n(a, 8, rd(ops[0]))
                self.store_n(a + 8, 8, rd(ops[1]))
            else:
                wr(ops[0], self.load_n(a, 8))
                wr(ops[1], self.load_n(a + 8, 8))
            return
        m = re.match(r'(ldr|str|ldur|stur)(b|h|sb|sh|sw)?$', mn)
        if m:
            n = {None: 8 if ops[0][0] == 'x' else 4, 'b': 1, 'h': 2, 'sb': 1, 'sh': 2, 'sw': 4}[m.group(2)]
            a = self.addr(ops[1])
            if m.group(1).startswith('ldr') or m.group(1) == 'ldur':
                v = self.load_n(a, n)
                if m.group(2) and m.group(2)[0] == 's' and v >> (8 * n - 1):
                    v -= 1 << (8 * n)
                wr(ops[0], v)
            else:
                self.store_n(a, n, rd(ops[0]))
            return
        # floats
        if mn == 'fmov':
            d, s = ops
            if d[0] in 'ds' and s[0] in 'xw':
                self.v[int(d[1:])] = rd(s) & (M64 if d[0] == 'd' else 0xffffffff)
            elif d[0] in 'xw':
                wr(d, self.v[int(s[1:])] & (M64 if s[0] == 'd' else 0xffffffff))
            else:
                self.v[int(d[1:])] = self.v[int(s[1:])]
            return
        if mn in ('fadd', 'fsub', 'fmul', 'fdiv'):
            a, b = self.fget(ops[1]), self.fget(ops[2])
            if mn == 'fdiv' and b == 0:
                r = float('nan') if a == 0 or a != a else float('inf') * (1 if a > 0 else -1)
            else:
                r = {'fadd': a + b, 'fsub': a - b, 'fmul': a * b, 'fdiv': (a / b) if b else 0}[mn]
            self.fset(ops[0], r)
            return
        if mn == 'fneg':
            self.fset(ops[0], -self.fget(ops[1]))
            return
        if mn == 'fcmp':
            a, b = self.fget(ops[0]), self.fget(ops[1])
            if a != a or b != b:
                self.n, self.z, self.c, self.vf = 0, 0, 1, 1
            elif a == b:
                self.n, self.z, self.c, self.vf = 0, 1, 1, 0
            elif a < b:
                self.n, self.z, self.c, self.vf = 1, 0, 0, 0
            else:
                self.n, self.z, self.c, self.vf = 0, 0, 1, 0
            return
        if mn in ('scvtf', 'ucvtf'):
            v = rd(ops[1])
            if mn == 'scvtf' and v >> 63:
                v -= 1 << 64
            self.fset(ops[0], float(v))
            return
        if mn in ('fcvtzs', 'fcvtzu'):
            f = self.fget(ops[1])
            if f != f:
                r = 0
            else:
                r = int(f) if abs(f) != float('inf') else (1 << 63) - 1
            wr(ops[0], r)
            return
        if mn == 'fcvt':
            self.fset(ops[0], self.fget(ops[1]))
            return
        raise RuntimeError('instruction not simulated: %s %s' % (mn, ops))


def split_ops(s):
    out = []
    depth = 0
    cur = ''
    for ch in s:
        if ch == '[':
            depth += 1
        elif ch == ']':
            depth -= 1
        if ch == ',' and depth == 0:
            out.append(cur.strip())
            cur = ''
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


def main():
    path = sys.argv[1]
    entry = 'main'
    steps = 200_000_000
    if '--entry' in sys.argv:
        entry = sys.argv[sys.argv.index('--entry') + 1]
    if '--max-steps' in sys.argv:
        steps = int(sys.argv[sys.argv.index('--max-steps') + 1])
    sim = Sim(path, steps)
    try:
        r = sim.call(entry)
    except SystemExit as e:
        r = e.code
    sys.stdout.buffer.write(bytes(sim.out))
    sys.stdout.flush()
    sys.exit(r & 255)


if __name__ == '__main__':
    main()
