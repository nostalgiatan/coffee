#!/usr/bin/env python3
"""Differential test of the aarch64 code generator: each program in a64progs/ is run natively (x86-64)
and, compiled with --target=aarch64-linux, in the interpreter of a64sim.py; the exit status and the
standard output must be equal.

Usage: a64diff.py <coffee2> [name-filter]
"""
import glob
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))


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


def main():
    coffee2 = sys.argv[1]
    flt = sys.argv[2] if len(sys.argv) > 2 else ''
    bad = 0
    gen = tempfile.mkdtemp()
    bigframe(os.path.join(gen, 'gen_bigframe.cf'))
    progs = sorted(glob.glob(os.path.join(HERE, 'a64progs', '*.cf'))) + [os.path.join(gen, 'gen_bigframe.cf')]
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
            r = subprocess.run([coffee2, p, '--target=aarch64-linux', '--emit-obj', '-o', d + '/a.o'], capture_output=True, text=True)
            if r.returncode != 0:
                print(name, 'FAIL aarch64 compile', r.stdout[:300], r.stderr[:300])
                bad += 1
                continue
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
