"""Runs the type checker over every Coffee source in the repository, as one program: the std
library and the lexer, parser, sema and types sources are the modules (declared in two rounds
so imports see each other's types), the tests and example programs are checked on top. Every
finding is a false positive of the checker, or a bug in the code.

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
if len(sys.argv) > 1:
    others = [f for f in others if sys.argv[1] in f]
files = modules + others


def esc(s):
    return s.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n').replace('\r', '\\r').replace('\t', '\\t')


lines = ['use * in types_probe', 'use printf, fflush, putchar in libc of c', '',
         'fn report(t: &Tys, a: &Ast, g: &Syms, m: int, file: int) => int:',
         '    let r = check_types(t, g, a, m)',
         '    for i in 0..r.diags.n:',
         '        printf("%d code=%d line=%d name=", file, te_number(r.diags.code[i]), a.toks.line[r.diags.tok[i]])',
         '        for k in a.toks.start[r.diags.tok[i]]..a.toks.end[r.diags.tok[i]]:',
         '            putchar(a.byte_at(k))',
         '        let o = new_out()',
         '        o.put(" want=")',
         '        show(&o, t, g, r.diags.want[i])',
         '        o.put(" found=")',
         '        show(&o, t, g, r.diags.found[i])',
         '        for k in 0..o.n:',
         '            putchar(o.b[k])',
         '        putchar(10)',
         '    printf("%d stats visited=%d untyped=%d\\n", file, r.visited(a), r.untyped(a))',
         '    fflush(0)',
         '    return r.diags.n', '', 'fn main() => int:', '    let g = new_globals()', '    let t = new_tys()', '    let bad = 0']
for i, f in enumerate(files):
    lines.append('    let a%d = parse("%s")' % (i, esc(open(f, encoding='utf8').read())))
for i, f in enumerate(modules):
    lines.append('    let m%d = declare_module(&g, &a%d, "%s")' % (i, i, os.path.splitext(os.path.basename(f))[0]))
for rnd in range(2):
    for i in range(len(modules)):
        lines.append('    declare_types(&t, &g, &a%d, m%d)' % (i, i))
for i in range(len(modules)):
    lines.append('    bad = bad + report(&t, &a%d, &g, m%d, %d)' % (i, i, i))
for i in range(len(modules), len(files)):
    lines.append('    let m%d = declare_module(&g, &a%d, "t%d")' % (i, i, i))
    lines.append('    declare_types(&t, &g, &a%d, m%d)' % (i, i))
    lines.append('    bad = bad + report(&t, &a%d, &g, m%d, %d)' % (i, i, i))
lines.append('    return bad')
driver = os.path.join(PROJECT, 'tests', 'z_corpus.cf')
open(driver, 'w').write('\n'.join(lines) + '\n')
try:
    r = subprocess.run([os.environ.get('COFFEE', 'coffee'), '--test-mode', '-I', '../sema/src', '-I', '../parser/src', '-I', '../lexer/src', '-I', 'src', '--jit', 'tests/z_corpus.cf'],
                       capture_output=True, errors='replace', text=True, cwd=PROJECT, timeout=1800)
finally:
    os.remove(driver)
out = r.stdout + r.stderr
names = {i: os.path.relpath(f, ROOT) for i, f in enumerate(files)}
errs = 0
vis = unt = 0
for l in out.splitlines():
    parts = l.split(None, 3)
    if len(parts) == 4 and parts[1] == 'stats':
        vis += int(parts[2].split('=')[1]); unt += int(parts[3].split('=')[1])
        continue
    if len(parts) >= 4 and parts[1].startswith('code=') and parts[0].isdigit():
        errs += 1
        print(names[int(parts[0])], parts[1], parts[2], parts[3])
if 'exited with code' not in out:
    print(out[-2500:])
print('files %d, findings %d; typed %d of %d nodes' % (len(files), errs, vis - unt, vis))
sys.exit(1 if errs or 'exited with code' not in out else 0)
