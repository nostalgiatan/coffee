#!/usr/bin/env python3
"""Tests the Win64 calling convention of the x86-64 code generator without Windows: `--target=x86_64-linux-ms` makes an ELF
object that follows the Win64 convention (rcx, rdx, r8, r9, shadow space, rsi and rdi kept by the `c fn`s), and `--static`
links it with a start-up code and a `cf_syscall` that follow it too, so the programs run on Linux.

  winabi.py <coffee2> cases [name-filter] [-j N]   the `exits` cases of coffee/compat/tests: `main` must return n
  winabi.py <coffee2> link [name-filter]           the same cases for --target=x86_64-windows: the COFF object must link with MinGW-w64
                                                   (nothing is run: there is no Windows here)
  winabi.py <coffee2> progs                        ir/tools/a64progs/*.cf: the output must equal that of the System V build
"""
import concurrent.futures
import glob
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from a64cases import cases  # noqa: E402


def build(coffee2, src, out, target):
    r = subprocess.run([coffee2, src, '--target=' + target, '--static', '-o', out], capture_output=True, text=True)
    return r.returncode == 0 and os.path.exists(out), (r.stdout + r.stderr)[:300]


def run_case(args):
    coffee2, name, src, want = args
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, 'c.cf')
        open(p, 'w').write(src)
        out = os.path.join(d, 'c')
        ok, msg = build(coffee2, p, out, 'x86_64-linux-ms')
        if not ok:
            return name, 'compile', msg
        r = subprocess.run([out], capture_output=True, text=True, timeout=120)
        if r.returncode == want:
            return name, 'ok', ''
        return name, 'wrong', 'got %d want %d' % (r.returncode, want)


def link_case(args):
    coffee2, name, src, want = args
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, 'c.cf')
        open(p, 'w').write(src)
        out = os.path.join(d, 'c.exe')
        r = subprocess.run([coffee2, p, '--target=x86_64-windows', '-o', out], capture_output=True, text=True)
        if r.returncode == 0 and os.path.exists(out):
            return name, 'ok', ''
        return name, 'link', (r.stdout + r.stderr)[:300]


def run_prog(args):
    coffee2, path = args
    name = os.path.basename(path)
    with tempfile.TemporaryDirectory() as d:
        res = []
        for t in ('x86_64-linux', 'x86_64-linux-ms'):
            out = os.path.join(d, t)
            ok, msg = build(coffee2, path, out, t)
            if not ok:
                return name, 'compile', t + ' ' + msg
            r = subprocess.run([out], capture_output=True, timeout=120)
            res.append((r.returncode, r.stdout))
        if res[0] == res[1]:
            return name, 'ok', ''
        return name, 'wrong', 'exit %d/%d, output %s' % (res[0][0], res[1][0], 'same' if res[0][1] == res[1][1] else 'differs')


def main():
    coffee2 = sys.argv[1]
    mode = sys.argv[2]
    flt = sys.argv[3] if len(sys.argv) > 3 and not sys.argv[3].startswith('-') else ''
    jobs = int(sys.argv[sys.argv.index('-j') + 1]) if '-j' in sys.argv else 8
    if mode == 'link':
        todo = [(coffee2, n, s, w) for n, s, w in cases() if flt in n]
        fn = link_case
    elif mode == 'cases':
        todo = [(coffee2, n, s, w) for n, s, w in cases() if flt in n]
        fn = run_case
    else:
        todo = [(coffee2, p) for p in sorted(glob.glob(os.path.join(HERE, 'a64progs', '*.cf'))) if flt in p]
        fn = run_prog
    stats = {}
    with concurrent.futures.ThreadPoolExecutor(jobs) as ex:
        for name, st, msg in ex.map(fn, todo):
            stats[st] = stats.get(st, 0) + 1
            if st != 'ok':
                print(name, st, msg)
    print(stats, 'of', len(todo))
    sys.exit(0 if stats.get('ok', 0) == len(todo) else 1)


if __name__ == '__main__':
    main()
