"""Runs the semantic analyzer over every Coffee source in the repository, as one program:
the std library and the lexer, parser and sema sources are the modules (each is declared,
then checked against the others); the tests and example programs are checked on top. Any
error is a false positive of the analyzer, or a bug in the code.

    COFFEE=/path/to/coffee python3 tools/corpus.py
"""
import glob, os, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
PROJECT = os.path.join(HERE, '..')
ROOT = os.path.join(PROJECT, '..', '..')
MODULE_GLOBS = ['library/std/src/*.cf', 'coffee/lexer/src/*.cf', 'coffee/parser/src/*.cf', 'coffee/sema/src/*.cf', 'coffee/types/src/*.cf']
OTHER_GLOBS = ['coffee/lexer/tests/*.cf', 'coffee/parser/tests/test_*.cf', 'coffee/sema/tests/test_*.cf', 'coffee/types/tests/test_*.cf', 'tests/coffee_programs/*.cf']
modules = [f for g in MODULE_GLOBS for f in sorted(glob.glob(os.path.join(ROOT, g)))]
others = [f for g in OTHER_GLOBS for f in sorted(glob.glob(os.path.join(ROOT, g)))]
files = modules + others


def esc(s):
    return s.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n').replace('\r', '\\r').replace('\t', '\\t')


lines = ['use * in sema_probe', '', 'fn report(a: &Ast, g: &Syms, m: int, file: int) => int:',
         '    let r = check(a, g, m)',
         '    for i in 0..r.diags.n:',
         '        let w = 0',
         '        if sc_is_warning(r.diags.code[i]):',
         '            w = 1',
         '        printf("%d code=%d line=%d warn=%d name=", file, r.diags.code[i], a.toks.line[r.diags.tok[i]], w)',
         '        for k in a.toks.start[r.diags.tok[i]]..a.toks.end[r.diags.tok[i]]:',
         '            putchar(a.byte_at(k))',
         '        putchar(10)',
         '    fflush(0)',
         '    return r.diags.errors()', '', 'fn main() => int:', '    let g = new_globals()', '    let bad = 0']
for i, f in enumerate(files):
    lines.append('    let a%d = parse("%s")' % (i, esc(open(f, encoding='utf8').read())))
for i, f in enumerate(modules):
    lines.append('    let m%d = declare_module(&g, &a%d, "%s")' % (i, i, os.path.splitext(os.path.basename(f))[0]))
for i, f in enumerate(modules):
    lines.append('    bad = bad + report(&a%d, &g, m%d, %d)' % (i, i, i))
for i in range(len(modules), len(files)):
    lines.append('    let m%d = declare_module(&g, &a%d, "t%d")' % (i, i, i))
    lines.append('    bad = bad + report(&a%d, &g, m%d, %d)' % (i, i, i))
lines.append('    return bad')
driver = os.path.join(PROJECT, 'tests', 'z_corpus.cf')
open(driver, 'w').write('use * in sema_probe\nuse printf, fflush, putchar in libc of c\n' + '\n'.join(lines[1:]) + '\n')
try:
    r = subprocess.run([os.environ.get('COFFEE', 'coffee'), '--test-mode', '-I', '../parser/src', '-I', '../lexer/src', '-I', 'src', '--jit', 'tests/z_corpus.cf'],
                       capture_output=True, errors='replace', text=True, cwd=PROJECT)
finally:
    os.remove(driver)
out = r.stdout + r.stderr
names = {i: os.path.relpath(f, ROOT) for i, f in enumerate(files)}
errs = 0
for l in out.splitlines():
    parts = l.split()
    if len(parts) == 5 and parts[1].startswith('code=') and parts[0].isdigit():
        if parts[3] == 'warn=0':
            errs += 1
            print(names[int(parts[0])], parts[1], parts[2], parts[4])
        elif os.environ.get('WARNINGS'):
            print('warning', names[int(parts[0])], parts[2], parts[4])
if 'exited with code' not in out:
    print(out[-2000:])
print('files %d, errors %d' % (len(files), errs))
sys.exit(1 if errs or 'exited with code' not in out else 0)
