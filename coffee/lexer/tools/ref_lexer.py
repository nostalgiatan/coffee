KW="fn c class type packed enum of if elif else while for in match let mv clone copy rm clean raise return break continue use as true false int float bool string str void".split()
OP2={'//':'o','<<':'o','>>':'o','&&':'o','||':'o','==':'o','!=':'o','<=':'o','>=':'o','=>':'o','::':'o','..':'o'}
OP1={'+':'o','-':'o','*':'o','/':'o','%':'o','&':'o','|':'o','^':'o','<':'o','>':'o','!':'o','~':'o','=':'o','.':'o',',':',',':':':',';':';','(':'(',')':')','[':'[',']':']','{':'{','}':'}'}
BIN2={'//','<<','>>','&&','||','==','!=','<=','>='}
BIN1=set('+-*/%&|^<>')
def isd(c): return c.isascii() and c.isdigit()
def isa(c): return c.isascii() and (c.isalpha() or c=='_')
def isi(c): return isa(c) or isd(c)
def ishex(c): return c in '0123456789abcdefABCDEF'
def lex(src):
    n=len(src); out=[]  # list of (shape, text, tokentype)
    def at(i): return src[i] if 0<=i<n else '\0'
    stack=[0]; depth=0; i=0; bol=True
    def last(): return out[-1] if out else None
    def cont():
        l=last()
        if not l: return False
        if l[2] in('binop','assign'):
            # size suffix int(N)+
            if len(out)>=5 and l[2]=='binop' and l[1] in('+','-') and out[-2][1]==')' and out[-3][2]=='int' and out[-4][1]=='(' and out[-5][1] in('int','float') and out[-5][2]=='kw':
                return False
            if l[1] in('>','>>'):
                for q in reversed(out[:-1]):
                    if q[2] in('nl','indent','dedent'): break
                    if q[1]=='<': return False
            return True
        return False
    def numscan(i):
        c0,c1=at(i),at(i+1)
        if c0=='0' and c1 in 'xXbBoO' and c1!='\0':
            rad={'x':16,'X':16,'b':2,'B':2,'o':8,'O':8}[c1]
            j=i+2
            ok={16:lambda c:c=='_' or ishex(c),2:lambda c:c in '_01',8:lambda c:c=='_' or c in '01234567'}[rad]
            while j<n and ok(src[j]): j+=1
            if j==i+2 or isi(at(j)):
                while isi(at(j)): j+=1
                return j,False,True
            return j,False,False
        j=i
        while j<n and (isd(src[j]) or src[j]=='_'): j+=1
        fl=False
        if at(j)=='.' and isd(at(j+1)):
            fl=True; j+=1
            while j<n and (isd(src[j]) or src[j]=='_'): j+=1
        if at(j) in 'eE' and at(j)!='\0':
            k=j+1
            if at(k) in '+-' and at(k)!='\0': k+=1
            if isd(at(k)):
                fl=True; j=k
                while j<n and (isd(src[j]) or src[j]=='_'): j+=1
        if isi(at(j)):
            while isi(at(j)): j+=1
            return j,False,True
        return j,fl,False
    def esclen(i):
        c=at(i+1)
        if c in 'ntr\\"\'0' and c!='\0': return 2
        if c=='x' and ishex(at(i+2)) and ishex(at(i+3)): return 4
        return 0
    while i<n:
        if bol:
            ind=0;j=i
            while at(j) in ' \t' and j<n:
                if at(j)=='\t': out.append(('X','\t','err')); ind+=4
                else: ind+=1
                j+=1
            c=at(j)
            if j>=n: i=j
            elif c in '\n\r':
                if c=='\r': j+=1
                i=j
                if at(i)=='\n': i+=1
            elif src.startswith('/#/',j):
                while j<n and src[j]!='\n': j+=1
                i=j
            else:
                i=j; bol=False
                top=stack[-1]
                if ind>top:
                    if len(stack)>=128: out.append(('X','','err'))
                    else: stack.append(ind); out.append(('I','','indent'))
                elif ind<top:
                    while len(stack)>1 and ind<stack[-1]:
                        stack.pop(); out.append(('D','','dedent'))
                    if ind!=stack[-1]: out.append(('X','','err'))
        else:
            c=at(i)
            if c in ' \t\r': i+=1
            elif c=='\n':
                l=last()
                ends = l is not None and l[2] not in('nl','indent','dedent')
                ct=cont()
                if depth==0 and ends and not ct: out.append(('N','\n','nl'))
                i+=1
                bol = depth==0 and not ct
            elif src.startswith('/#/',i):
                while i<n and src[i]!='\n': i+=1
            elif src.startswith('/#*',i):
                j=src.find('*#/',i+3)
                if j<0: out.append(('X','','err')); i=n
                else: i=j+3
            elif isd(c):
                e,fl,err=numscan(i)
                if err: out.append(('X','','err'))
                elif fl: out.append(('d',src[i:e],'float'))
                else: out.append(('i',src[i:e],'int'))
                i=e
            elif c=='f' and at(i+1)=='"':
                e,err=strscan(src,i+1,esclen)
                out.append(('X' if err else 'f','','err' if err else 'str')); i=e
            elif isa(c):
                e=i+1
                while isi(at(e)): e+=1
                w=src[i:e]
                if w in KW: out.append(('k',w,'kw'))
                else: out.append(('w',w,'id'))
                i=e
            elif c=='"':
                e,err=strscan(src,i,esclen)
                out.append(('X' if err else 's','','err' if err else 'str')); i=e
            elif c=="'":
                c1=at(i+1)
                e=None
                if c1=='\\':
                    l=esclen(i+1)
                    if l and at(i+1+l)=="'": e=i+l+2
                elif c1 not in '\0\'\n' and at(i+2)=="'": e=i+3
                if e: out.append(('c','','char')); i=e
                else:
                    j=i+1
                    while j<n and src[j] not in "'\n": j+=1
                    if at(j)=="'": j+=1
                    out.append(('X','','err')); i=j
            elif c=='#':
                e=i+1
                while isi(at(e)): e+=1
                out.append(('X' if e==i+1 else '@','','err' if e==i+1 else 'lis')); i=e
            else:
                two=src[i:i+2]
                if two in OP2:
                    out.append(('o',two,'binop' if two in BIN2 else 'op')); i+=2
                elif c in OP1:
                    sh=OP1[c]
                    ty='binop' if c in BIN1 else ('assign' if c=='=' else 'op')
                    if c in '([{': depth+=1
                    elif c in ')]}':
                        if depth==0: sh='X'; ty='err'
                        else: depth-=1
                    out.append((sh,c,ty)); i+=1
                else:
                    out.append(('X','','err')); i+=1
    if depth>0: out.append(('X','','err'))
    l=last()
    if l is not None and l[2] not in('nl','indent','dedent'): out.append(('N','','nl'))
    while len(stack)>1: stack.pop(); out.append(('D','','dedent'))
    out.append(('E','','eof'))
    return ''.join(o[0] for o in out)
def strscan(src,i,esclen):
    n=len(src); j=i+1; bad=False
    while j<n:
        c=src[j]
        if c=='"': return j+1,bad
        if c=='\n': return j,True
        if c=='\\':
            l=esclen(j)
            if l==0: bad=True; j+=1
            else: j+=l
        else: j+=1
    return n,True
