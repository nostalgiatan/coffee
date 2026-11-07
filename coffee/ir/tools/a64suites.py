#!/usr/bin/env python3
"""Runs the test programs of the stage directories (lexer parser sema types own ir ld rt driver compat ...)
as aarch64 programs under qemu-aarch64: each tests/*.cf is built with --target=aarch64-linux --static, using
the include path of the directory's coffee.toml, and must exit 0 (what `tea test` expects of it).

Usage: a64suites.py <coffee2> <stage dir>... [-j N]
"""
import concurrent.futures
import os
import re
import subprocess
import sys
import tempfile


def includes(d):
    t = open(os.path.join(d, 'coffee.toml')).read()
    inc = []
    for sec in ('test', 'build'):
        m = re.search(r'\[' + sec + r'\][^\[]*?include\s*=\s*\[(.*?)\]', t, re.S)
        if m:
            inc += re.findall(r'"([^"]+)"', m.group(1))
    inc += ['src', 'tests', '../ir/src', '../ld/src', '../driver/src', 'kit', '../lexer/src']
    return [os.path.normpath(os.path.join(d, i)) for i in inc]


def run(a):
    coffee2, d, f = a
    with tempfile.TemporaryDirectory() as t:
        cmd = [coffee2, f, '--target=aarch64-linux', '--static', '-o', t + '/x']
        for i in includes(d):
            cmd += ['-I', i]
        r = subprocess.run(cmd, capture_output=True, text=True, cwd=d)
        if r.returncode != 0 or not os.path.exists(t + '/x'):
            msg = (r.stdout + r.stderr).strip().splitlines()
            return d, f, 'compile', msg[-1][:150] if msg else ''
        try:
            r = subprocess.run(['qemu-aarch64', t + '/x'], capture_output=True, text=True, timeout=300, cwd=d)
        except subprocess.TimeoutExpired:
            return d, f, 'timeout', ''
        if r.returncode == 0:
            return d, f, 'ok', ''
        out = (r.stdout + r.stderr).strip().splitlines()
        return d, f, 'exit %d' % r.returncode, out[-1][:150] if out else ''


def main():
    coffee2 = os.path.abspath(sys.argv[1])
    jobs = int(sys.argv[sys.argv.index('-j') + 1]) if '-j' in sys.argv else 8
    dirs = [os.path.abspath(a) for a in sys.argv[2:] if os.path.isdir(a)]
    todo = []
    for d in dirs:
        td = os.path.join(d, 'tests')
        for f in sorted(os.listdir(td)):
            if f.endswith('.cf') and f.startswith('test'):
                todo.append((coffee2, d, os.path.join(td, f)))
    stats = {}
    with concurrent.futures.ThreadPoolExecutor(jobs) as ex:
        for d, f, st, msg in ex.map(run, todo):
            key = (os.path.basename(d), st if st == 'ok' else 'fail')
            stats[key] = stats.get(key, 0) + 1
            if st != 'ok':
                print(os.path.basename(d), os.path.basename(f), st, msg)
    for k in sorted(stats):
        print(k, stats[k])


if __name__ == '__main__':
    main()
