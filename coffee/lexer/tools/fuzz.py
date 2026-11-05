"""Differential fuzzer: random token soup is lexed by lexer.cf and by ref_lexer.py
(an independent reimplementation of the same spec); the shape strings must agree and
the structural invariants of check.cf must hold.

    COFFEE=/path/to/coffee python3 tools/fuzz.py [seed] [cases]
"""
import random,subprocess,sys,os
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)
from ref_lexer import lex
random.seed(int(sys.argv[1]) if len(sys.argv)>1 else 1)
FRAG=['fn','let','c','int','float','x','abc','_a','1','23','0x1F','0b101','0o7','1_0','1.5','2e3','1e','0x','12ab','.','..',',',':',';','(',')','[',']','{','}','+','-','*','/','//','%','&','&&','|','||','^','<','<<','<=','>','>>','>=','=','==','!=','=>','::','!','~','"s"','"a\\n"','"bad\\q"','"open','f"x"','f','\'a\'','\'\\n\'','\'ab\'','\'','#tag','#','@','$','?','`','\\',' ','  ','    ','\n','\n','\n    ','\n        ','\n  ','\t','\r\n','/#/ c','/#* b *#/','/#*','*#/','é','单']
def gen():
    return ''.join(random.choice(FRAG) for _ in range(random.randint(1,25)))
def esc(s): return s.replace('\\','\\\\').replace('"','\\"').replace('\n','\\n').replace('\r','\\r').replace('\t','\\t')
N=int(sys.argv[2]) if len(sys.argv)>2 else 300
cases=[gen() for _ in range(N)]
body=''.join('    show("%s")\n'%esc(c) for c in cases)
open(os.path.join(HERE,'..','tests','z_fuzz.cf'),'w').write('use * in dump\nuse fflush, putchar in libc of c\n\nfn show(src: &str) => void:\n    let t = lex(src)\n    if invariants(src, &t) > 0:\n        putchar(33)\n    dump_shape(&t)\n\nfn main() => int:\n'+body+'    fflush(0)\n    return 0\n')
r=subprocess.run([os.environ.get('COFFEE','coffee'),'--test-mode','-I','src','--jit','tests/z_fuzz.cf'],capture_output=True,errors='replace',text=True,cwd=os.path.join(HERE,'..'))
os.remove(os.path.join(HERE,'..','tests','z_fuzz.cf'))
lines=[l for l in r.stdout.split('\n')]
# program prints "Program exited" at some point; filter shape lines by order
shapes=[l for l in lines if l and not l.startswith('Program exited') and 'DEBUG' not in l]
if len(shapes)<N: print('short output',len(shapes),r.stderr[:300]); sys.exit(1)
bad=0
for c,got in zip(cases,shapes):
    try: want=lex(c)
    except Exception as e: want='EXC'+str(e)
    if want!=got:
        bad+=1
        if bad<=8: print('MISMATCH',repr(c),'\n  want',want,'\n  got ',got)
print('cases',N,'mismatches',bad)
