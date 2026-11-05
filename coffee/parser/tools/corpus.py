"""Parses every Coffee source in the repository with the self-hosted parser and reports
the files that produce errors. It is the parser's real-world check: the standard library,
the example programs and the lexer and parser themselves must all parse cleanly.

    COFFEE=/path/to/coffee python3 tools/corpus.py
"""
import glob, os, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
PROJECT = os.path.join(HERE, '..')
ROOT = os.path.join(PROJECT, '..', '..')
GLOBS = ['library/std/src/*.cf', 'tests/coffee_programs/*.cf', 'coffee/lexer/src/*.cf',
         'coffee/lexer/tests/*.cf', 'coffee/parser/src/*.cf', 'coffee/parser/tests/test_*.cf']
files = [f for g in GLOBS for f in sorted(glob.glob(os.path.join(ROOT, g)))]


def esc(s):
    return s.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n').replace('\r', '\\r').replace('\t', '\\t')


head = ('use * in tree_dump\n\n'
        'fn one(name: int, src: str) => int:\n'
        '    let a = parse(src)\n'
        '    if a.err_count > 0:\n'
        '        printf("%d errors=%d first=%d line=%d\\n", name, a.err_count, a.err_code[0], a.toks.line[a.err_tok[0]])\n'
        '        fflush(0)\n'
        '    return a.err_count\n\n'
        'fn main() => int:\n    let bad = 0\n')
body = ''.join('    bad = bad + one(%d, "%s")\n' % (i, esc(open(f, encoding='utf8').read())) for i, f in enumerate(files))
driver = os.path.join(PROJECT, 'tests', 'z_corpus.cf')
open(driver, 'w').write(head + body + '    return bad\n')
try:
    r = subprocess.run([os.environ.get('COFFEE', 'coffee'), '--test-mode', '-I', '../lexer/src', '--jit', 'tests/z_corpus.cf'],
                       capture_output=True, errors='replace', text=True, cwd=PROJECT)
finally:
    os.remove(driver)
bad = [l for l in r.stdout.splitlines() if ' errors=' in l]
for l in bad:
    i, rest = l.split(' ', 1)
    print(os.path.relpath(files[int(i)], ROOT), rest)
print('files %d, with errors %d' % (len(files), len(bad)))
sys.exit(1 if bad or 'exited with code: 0' not in r.stdout + r.stderr else 0)
