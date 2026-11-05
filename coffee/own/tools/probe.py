"""Ownership-checks small programs and prints each finding with its token.

    COFFEE=... python3 tools/probe.py file.cf ...        programs from files
    COFFEE=... python3 tools/probe.py --diff 3,7,88      programs of differential.py by index
"""
import os, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
PROJECT = os.path.join(HERE, '..')
COFFEE = os.environ.get('COFFEE', 'coffee')


def esc(s):
    return s.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n').replace('\r', '\\r').replace('\t', '\\t')


def load():
    if sys.argv[1] == '--diff':
        src = open(os.path.join(HERE, 'differential.py'), encoding='utf8').read().split('progs = programs()')[0]
        ns = {'__file__': os.path.join(HERE, 'differential.py'), '__name__': 'diffmod', 'sys': sys}
        saved = sys.argv
        sys.argv = ['x']
        exec(compile(src, 'differential', 'exec'), ns)
        sys.argv = saved
        progs = ns['programs']()
        return [(int(i), progs[int(i)]) for i in sys.argv[2].split(',')], ns['STD']
    import glob
    std_dir = os.path.join(HERE, '..', '..', '..', 'library', 'std', 'src')
    std = [(os.path.splitext(os.path.basename(f))[0], open(f, encoding='utf8').read()) for f in sorted(glob.glob(os.path.join(std_dir, '*.cf')))]
    return [(i, open(f, encoding='utf8').read()) for i, f in enumerate(sys.argv[1:])], std


items, std = load()
lines = ['use * in own_probe', 'use printf, fflush, putchar in libc of c', '',
         'fn one(i: int, src: str, t: &Tys, g: &Syms) => int:',
         '    let a = parse(src)',
         '    let m = declare_module(g, &a, "t")',
         '    declare_types(t, g, &a, m)',
         '    let r = check_types(t, g, &a, m)',
         '    let f = check_own(t, g, &a, &r)',
         '    for k in 0..f.diags.n:',
         '        printf("%d code=%d line=%d name=", i, oe_number(f.diags.code[k]), a.toks.line[f.diags.tok[k]])',
         '        for b in a.toks.start[f.diags.tok[k]]..a.toks.end[f.diags.tok[k]]:',
         '            putchar(a.byte_at(b))',
         '        putchar(10)',
         '    fflush(0)',
         '    return 0', '', 'fn main() => int:', '    let t = new_tys()']
for j, (name, text) in enumerate(std):
    lines.append('    let s%d = parse("%s")' % (j, esc(text)))
for n, (i, src) in enumerate(items):
    lines.append('    let g%d = new_globals()' % n)
    for j, (name, text) in enumerate(std):
        lines.append('    let sm%d_%d = declare_module(&g%d, &s%d, "%s")' % (n, j, n, j, name))
    for j in range(len(std)):
        lines.append('    declare_types(&t, &g%d, &s%d, sm%d_%d)' % (n, j, n, j))
    lines.append('    one(%d, "%s", &t, &g%d)' % (i, esc(src), n))
lines.append('    return 0')
driver = os.path.join(PROJECT, 'tests', 'z_probe.cf')
open(driver, 'w', encoding='utf8').write('\n'.join(lines) + '\n')
try:
    r = subprocess.run([COFFEE, '--test-mode', '-I', '../types/src', '-I', '../sema/src', '-I', '../parser/src', '-I', '../lexer/src', '-I', 'src', '--jit', 'tests/z_probe.cf'],
                       capture_output=True, errors='replace', text=True, cwd=PROJECT, timeout=900)
finally:
    os.remove(driver)
out = r.stdout + r.stderr
for l in out.splitlines():
    l = l.replace('\x1b', '')
    if ' code=' in l and l.split()[0].isdigit():
        print(l)
if 'exited with code' not in out:
    print(out[-2000:])
