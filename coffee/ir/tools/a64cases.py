#!/usr/bin/env python3
"""Runs the `exits(name, src, n)` cases of coffee/compat/tests through the aarch64 code generator and
the interpreter in a64sim.py: each program is compiled with --target=aarch64-linux --emit-obj and its
`main` must return n.

With qemu-aarch64 each program is linked with --static and run under qemu; otherwise (or with --sim)
the object is run in the interpreter.

Usage: a64cases.py <coffee2> [name-filter] [-j N] [--sim]
"""
import concurrent.futures
import glob
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
TESTS = os.path.join(HERE, '..', '..', 'compat', 'tests')
LIT = re.compile(r'exits\("([^"]+)", "((?:[^"\\]|\\.)*)", (-?\d+)\)')


def unescape(s):
    out = []
    i = 0
    while i < len(s):
        if s[i] == '\\' and i + 1 < len(s):
            out.append({'n': '\n', 't': '\t', '"': '"', '\\': '\\'}.get(s[i + 1], '\\' + s[i + 1]))
            i += 2
        else:
            out.append(s[i])
            i += 1
    return ''.join(out)


def cases():
    for f in sorted(glob.glob(os.path.join(TESTS, '*.cf'))):
        for m in LIT.finditer(open(f).read()):
            yield m.group(1), unescape(m.group(2)), int(m.group(3)) & 255


def run(args):
    coffee2, name, src, want = args
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, 'c.cf')
        open(p, 'w').write(src)
        use_qemu = shutil.which('qemu-aarch64') and '--sim' not in sys.argv
        out = os.path.join(d, 'c' if use_qemu else 'c.o')
        flag = '--static' if use_qemu else '--emit-obj'
        r = subprocess.run([coffee2, p, '--target=aarch64-linux', flag, '-o', out], capture_output=True, text=True)
        if r.returncode != 0 or not os.path.exists(out):
            return name, 'compile', (r.stdout + r.stderr)[:200]
        if use_qemu:
            r = subprocess.run(['qemu-aarch64', out], capture_output=True, text=True, timeout=120)
        else:
            r = subprocess.run([sys.executable, os.path.join(HERE, 'a64sim.py'), out, '--max-steps', '5000000'],
                               capture_output=True, text=True, timeout=120)
        if r.returncode == want:
            return name, 'ok', ''
        return name, 'wrong', 'got %d want %d %s' % (r.returncode, want, r.stderr.strip().splitlines()[-1:])


def main():
    coffee2 = sys.argv[1]
    flt = sys.argv[2] if len(sys.argv) > 2 and not sys.argv[2].startswith('-') else ''
    jobs = int(sys.argv[sys.argv.index('-j') + 1]) if '-j' in sys.argv else 8
    todo = [(coffee2, n, s, w) for n, s, w in cases() if flt in n]
    bad = 0
    stats = {}
    with concurrent.futures.ThreadPoolExecutor(jobs) as ex:
        for name, st, msg in ex.map(run, todo):
            stats[st] = stats.get(st, 0) + 1
            if st != 'ok':
                print(name, st, msg)
    print(stats, 'of', len(todo))


if __name__ == '__main__':
    main()
