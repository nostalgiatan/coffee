#!/usr/bin/env python3
"""Differential test of the aarch64 code generator: each program in a64progs/ is run natively (x86-64)
and, compiled with --target=aarch64-linux, in the interpreter of a64sim.py; the exit status and the
standard output must be equal.

With qemu-aarch64 installed the aarch64 side is linked with --static and run under qemu; `--sim` (or no
qemu) uses the interpreter on the object instead.

Usage: a64diff.py <coffee2> [name-filter] [--sim] [--target=aarch64-linux|riscv64-linux]
(riscv64 runs only under qemu-riscv64)
"""
import glob
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
TARGET = 'aarch64-linux'
for _a in sys.argv:
    if _a.startswith('--target='):
        TARGET = _a[9:]
QEMU = {'aarch64-linux': ['qemu-aarch64'], 'riscv64-linux': ['qemu-riscv64'], 'riscv64-linux-rva22': ['qemu-riscv64', '-cpu', 'rva22u64'], 'riscv64-linux-rva23': ['qemu-riscv64', '-cpu', 'rva23u64']}[TARGET]


def bigframe(path):
    """A function with 6000 values live at once: its frame is far past the reach of a scaled offset,
    and a call in the middle and a sum of everything at the end read every place."""
    n = 6000
    lines = ['fn id(x: int) => int:', '    return x', '', 'fn big(n: int) => int:']
    for i in range(n):
        lines.append('    let v%d = n * %d + %d' % (i, i % 7 + 1, i))
        if i == n // 2:
            lines.append('    let mid = id(v3) + id(v%d)' % (n // 2))
    lines.append('    return mid + ' + ' + '.join('v%d' % i for i in range(0, n, 1)))
    lines += ['', 'fn main() => int:', '    return big(3) % 251']
    open(path, 'w').write('\n'.join(lines) + '\n')


def bigfunc(path):
    """A function of about 1.6 MB of aarch64 code with a loop around all of it: the exit test of the
    loop and the branches over some statements reach past the +-1 MB of a conditional branch."""
    n = 40000
    lines = ['fn big(n: int) => int:', '    let s = 1', '    let i = 0', '    while i < n:']
    for k in range(n):
        lines.append('        s = (s * 3 + (s >> %d) + i * %d) %% 1000003' % (k % 7 + 1, k * 31 + 5))
        if k % 200 == 0:
            lines.append('        if s %% 5 == %d:' % (k % 5))
            lines.append('            s = s + %d' % k)
    lines.append('        i = i + 1')
    lines.append('    return s')
    lines += ['', 'fn main() => int:', '    return big(3) % 251']
    open(path, 'w').write('\n'.join(lines) + '\n')


def main():
    coffee2 = sys.argv[1]
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    flt = args[1] if len(args) > 1 else ''
    qemu = shutil.which(QEMU[0]) and '--sim' not in sys.argv
    bad = 0
    gen = tempfile.mkdtemp()
    bigframe(os.path.join(gen, 'gen_bigframe.cf'))
    bigfunc(os.path.join(gen, 'gen_bigfunc.cf'))
    progs = sorted(glob.glob(os.path.join(HERE, 'a64progs', '*.cf'))) + [os.path.join(gen, 'gen_bigframe.cf'), os.path.join(gen, 'gen_bigfunc.cf')]
    progs = [p for p in progs if flt in os.path.basename(p)]
    for p in progs:
        name = os.path.basename(p)
        with tempfile.TemporaryDirectory() as d:
            r = subprocess.run([coffee2, p, '-o', d + '/x'], capture_output=True, text=True)
            if r.returncode != 0:
                print(name, 'FAIL native compile', r.stdout[:300], r.stderr[:300])
                bad += 1
                continue
            nat = subprocess.run([d + '/x'], capture_output=True)
            if qemu:
                r = subprocess.run([coffee2, p, '--target=' + TARGET, '--static', '-o', d + '/a'], capture_output=True, text=True)
            else:
                r = subprocess.run([coffee2, p, '--target=' + TARGET, '--emit-obj', '-o', d + '/a.o'], capture_output=True, text=True)
            if r.returncode != 0:
                print(name, 'FAIL aarch64 compile', r.stdout[:300], r.stderr[:300])
                bad += 1
                continue
            if qemu:
                sim = subprocess.run(QEMU + [d + '/a'], capture_output=True)
            else:
                sim = subprocess.run([sys.executable, os.path.join(HERE, 'a64sim.py'), d + '/a.o'], capture_output=True)
            if sim.returncode != nat.returncode or sim.stdout != nat.stdout:
                print(name, 'FAIL native exit %d, simulated %d' % (nat.returncode, sim.returncode))
                print('  native:', nat.stdout[:300])
                print('  sim:   ', sim.stdout[:300], sim.stderr[-300:])
                bad += 1
            else:
                print(name, 'ok')
    sys.exit(1 if bad else 0)


if __name__ == '__main__':
    main()
