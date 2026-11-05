"""Compares the Coffee analyzer with the Rust compiler on programs written in the Rust test
suite (any string literal there that contains `fn main`).

A program the Rust compiler accepts must give no error here (false positive otherwise). For a
program it rejects with a name/declaration error (E101 E103 E106 E200 E201 E600) this reports
whether the analyzer noticed too (false negatives are expected for what it does not model).

    COFFEE=/path/to/coffee python3 tools/differential.py [chunk-size]
"""
import concurrent.futures, glob, json, os, re, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
PROJECT = os.path.join(HERE, '..')
ROOT = os.path.join(PROJECT, '..', '..')
COFFEE = os.environ.get('COFFEE', 'coffee')
CHUNK = int(sys.argv[1]) if len(sys.argv) > 1 else 60
# the standard library is an implicit package: every program may import from it
STD = [(os.path.splitext(os.path.basename(f))[0], open(f, encoding='utf8').read())
       for f in sorted(glob.glob(os.path.join(ROOT, 'library', 'std', 'src', '*.cf')))]
assert len(STD) == 4, 'the driver declares exactly four std modules'
NAME_ERRORS = {'E101', 'E103', 'E106', 'E200', 'E201', 'E600'}


def unescape(s):
    out = []; i = 0
    while i < len(s):
        c = s[i]
        if c != '\\':
            out.append(c); i += 1; continue
        n = s[i + 1]
        simple = {'n': '\n', 't': '\t', 'r': '\r', '"': '"', '\\': '\\', "'": "'", '0': '\0'}
        if n in simple: out.append(simple[n]); i += 2
        elif n == '\n':
            i += 2
            while i < len(s) and s[i] in ' \t\n': i += 1
        elif n == 'x': out.append(chr(int(s[i + 2:i + 4], 16))); i += 4
        elif n == 'u':
            j = s.index('}', i); out.append(chr(int(s[i + 3:j], 16))); i = j + 1
        else: return None
    return ''.join(out)


def programs():
    found, seen = [], set()
    for f in sorted(glob.glob(os.path.join(ROOT, 'tests', '*.rs'))):
        t = open(f, encoding='utf8').read()
        i, n = 0, len(t)
        while i < n:
            if t.startswith('//', i):
                i = t.find('\n', i)
                if i < 0: break
                continue
            if t[i] == 'r' and t[i + 1] in '#"':
                h = 0
                while t[i + 1 + h] == '#': h += 1
                if t[i + 1 + h] == '"':
                    start = i + 2 + h
                    end = t.find('"' + '#' * h, start)
                    if end < 0: break
                    b = t[start:end]
                    if 'fn main' in b and b not in seen:
                        seen.add(b); found.append(b)
                    i = end + 1 + h
                    continue
            if t[i] == '"':
                j = i + 1
                while j < n and t[j] != '"':
                    j += 2 if t[j] == '\\' else 1
                b = unescape(t[i + 1:j])
                if b and 'fn main' in b and '{{' not in b and '{}' not in b and b not in seen:
                    seen.add(b); found.append(b)
                i = j + 1
                continue
            if t[i] == "'":
                i += 2 if t[i + 1] != '\\' else 4
                continue
            i += 1
    return found


def esc(s):
    return s.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n').replace('\r', '\\r').replace('\t', '\\t').replace('\0', '\\0')


def rust_verdict(src):
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, 'p.cf')
        open(p, 'w', encoding='utf8', errors='replace').write(src)
        # the compiler only reads files under its working directory
        r = subprocess.run([COFFEE, '--test-mode', '--emit-llvm', 'p.cf', '-o', 'p.ll'],
                           capture_output=True, text=True, errors='replace', timeout=60, cwd=d)
        return set(re.findall(r'\[(E\d{3})\]', re.sub(r'\x1b\[[0-9;]*m', '', r.stdout + r.stderr)))


def sema_verdicts(chunk):
    lines = ['use * in sema_probe', 'use printf, fflush in libc of c', '',
             'fn one(i: int, src: str, s0: &Ast, s1: &Ast, s2: &Ast, s3: &Ast) => int:',
             '    let a = parse(src)',
             '    let g = new_globals()',
             '    declare_module(&g, s0, "' + STD[0][0] + '")',
             '    declare_module(&g, s1, "' + STD[1][0] + '")',
             '    declare_module(&g, s2, "' + STD[2][0] + '")',
             '    declare_module(&g, s3, "' + STD[3][0] + '")',
             '    let m = declare_module(&g, &a, "t")',
             '    let r = check(&a, &g, m)',
             '    let pcode = -1',
             '    let pline = 0',
             '    if a.err_count > 0:',
             '        pcode = a.err_code[0]',
             '        pline = a.toks.line[a.err_tok[0]]',
             '    printf("%d errors=%d parse=%d,%d,%d codes=", i, r.diags.errors(), a.err_count, pcode, pline)',
             '    for k in 0..r.diags.n:', '        printf("%d,", r.diags.code[k])',
             '    printf("\\n")', '    fflush(0)', '    return 0', '', 'fn main() => int:']
    for j, (name, text) in enumerate(STD):
        lines.append('    let s%d = parse("%s")' % (j, esc(text)))
    for i, src in enumerate(chunk):
        lines.append('    one(%d, "%s", &s0, &s1, &s2, &s3)' % (i, esc(src)))
    lines.append('    return 0')
    driver = os.path.join(PROJECT, 'tests', 'z_diff.cf')
    open(driver, 'w', encoding='utf8').write('\n'.join(lines) + '\n')
    try:
        r = subprocess.run([COFFEE, '--test-mode', '-I', '../parser/src', '-I', '../lexer/src', '-I', 'src', '--jit', 'tests/z_diff.cf'],
                           capture_output=True, errors='replace', text=True, cwd=PROJECT, timeout=900)
    finally:
        os.remove(driver)
    res = {}
    if 'exited with code' not in r.stdout + r.stderr:
        print('driver failed:', re.sub(r'\x1b\[[0-9;]*m', '', r.stdout + r.stderr)[-1500:])
    for l in r.stdout.splitlines():
        m = re.match(r'(\d+) errors=(\d+) parse=(\d+),(-?\d+),(\d+) codes=([\d,]*)$', l)
        if m:
            res[int(m.group(1))] = (int(m.group(2)), [int(c) for c in m.group(6).split(',') if c], (int(m.group(3)), int(m.group(4)), int(m.group(5))))
    return res


progs = programs()
print('programs', len(progs))
with concurrent.futures.ThreadPoolExecutor(8) as ex:
    rust = list(ex.map(rust_verdict, progs))
fp, fn, agree_bad, agree_ok, missing, gaps = [], [], 0, 0, [], []
agree_bad = agree_ok = 0
for s in range(0, len(progs), CHUNK):
    chunk = progs[s:s + CHUNK]
    got = sema_verdicts(chunk)
    for i, src in enumerate(chunk):
        k = s + i
        if i not in got:
            missing.append(k)
            continue
        errs, codes, parse = got[i]
        rv = rust[k]
        if not rv and parse[0]:
            gaps.append((k, parse))
        elif not rv and errs:
            fp.append((k, codes))
        elif rv & NAME_ERRORS and not errs:
            fn.append((k, sorted(rv)))
        elif rv & NAME_ERRORS:
            agree_bad += 1
        elif not rv:
            agree_ok += 1
print('both accept %d, both reject %d, no verdict %d' % (agree_ok, agree_bad, len(missing)))
print('parser gaps %d, false positives %d, missed %d' % (len(gaps), len(fp), len(fn)))
for k, (n, code, line) in gaps:
    lines = progs[k].split('\n')
    print('PARSER GAP', k, 'errors=%d code=%d line=%d:' % (n, code, line), (lines[line - 1] if 0 < line <= len(lines) else '').strip()[:100])
for k in missing:
    print('NO VERDICT', k)
for k, codes in fp:
    print('FALSE POSITIVE', k, codes)
    print('    ' + progs[k][:300].replace('\n', '\n    '))
if os.environ.get('MISSED'):
    for k, codes in fn:
        print('MISSED', k, codes)
sys.exit(1 if fp or missing or gaps else 0)
