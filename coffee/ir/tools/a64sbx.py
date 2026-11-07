#!/usr/bin/env python3
"""The sandbox code generator for aarch64 (--target=aarch64-linux --sandbox) on the 191 `exits(...)` cases of
compat/tests: each program is compiled to an object, and the verifier (ir/src/sbxcheck_a64.cf, run by the
driver) must accept it. A program that needs an argument on the stack or a function value is refused by the
generator ("could not be translated"); anything else refused is a failure.

Usage: a64sbx.py <coffee2>
"""
import collections
import glob
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import a64cases as c


def main():
    coffee2 = sys.argv[1]
    srcs = []
    for f in sorted(glob.glob(os.path.join(c.TESTS, '*.cf'))):
        for m in c.LIT.finditer(open(f).read()):
            srcs.append(c.unescape(m.group(2)))
    cnt = collections.Counter()
    bad = []
    with tempfile.TemporaryDirectory() as d:
        for i, s in enumerate(srcs):
            open(d + '/p.cf', 'w').write(s)
            r = subprocess.run([coffee2, d + '/p.cf', '--target=aarch64-linux', '--sandbox', '--emit-obj', '-o', d + '/p.o'],
                               capture_output=True, text=True)
            out = r.stdout + r.stderr
            if r.returncode == 0 and 'not safe' not in out and 'could not' not in out:
                cnt['accepted'] += 1
            elif 'could not be translated' in out and 'not safe' not in out:
                cnt['refused by the generator'] += 1
            else:
                cnt['FAILED'] += 1
                bad.append((i, out[:120]))
    print(dict(cnt), 'of', len(srcs))
    for b in bad[:10]:
        print(b)
    sys.exit(1 if bad else 0)


if __name__ == '__main__':
    main()
