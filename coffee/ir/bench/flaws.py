#!/usr/bin/env python3
"""Counts avoidable instruction patterns in the code of the benchmark programs.
Usage: bench/flaws.py <coffee2>    (compiles every benchmark with --static, reads it with objdump)"""
import subprocess, sys, re, os, tempfile
C = os.path.realpath(sys.argv[1]); B = os.path.dirname(os.path.realpath(__file__)); L = B + "/../../lexer/src"
names = "fib acc sieve collatz mandel sort hash objects strings matmul bits".split()
pats = ["movchain", "movback", "leamov", "testflags", "jmpnext", "movzero", "selfmov"]
print("%-9s %6s " % ("program", "insns") + " ".join("%8s" % p for p in pats))
tot = {p: 0 for p in pats}; tn = 0
for n in names:
    exe = tempfile.mktemp()
    subprocess.run([C, f"{B}/{n}.cf", "-I", L, "--static", "-g", "-o", exe], check=True, capture_output=True)
    out = subprocess.run(["objdump", "-d", "--no-show-raw-insn", exe], capture_output=True, text=True).stdout
    os.unlink(exe)
    ins = []; cur = ""
    for l in out.splitlines():
        m = re.match(r"^[0-9a-f]+ <(.*)>:", l)
        if m: cur = m.group(1); continue
        m = re.match(r"^\s+([0-9a-f]+):\s+(\S+)\s*(.*)$", l)
        if m and not cur.startswith("__cf") and not cur.startswith("cf_"):
            ins.append((int(m.group(1), 16), m.group(2), m.group(3).split("#")[0].strip(), cur))
    c = {p: 0 for p in pats}
    reg = r"%[a-z0-9]+"
    for i in range(len(ins) - 1):
        a, b = ins[i], ins[i + 1]
        if a[3] != b[3]: continue
        am = re.match(rf"^({reg}),({reg})$", a[2]) if a[1].startswith("mov") and a[1] != "movss" else None
        bm = re.match(rf"^({reg}),({reg})$", b[2]) if b[1].startswith("mov") and b[1] != "movss" else None
        if am and am.group(1) == am.group(2): c["selfmov"] += 1
        if am and bm and am.group(2) == bm.group(1): c["movchain"] += 1
        if am and bm and am.group(1) == bm.group(2) and am.group(2) == bm.group(1): c["movback"] += 1
        if a[1] == "lea" and bm and a[2].endswith("," + bm.group(1)): c["leamov"] += 1
        if b[1] == "test" and re.match(rf"^({reg}),\1$", b[2]) and a[1] in ("add", "sub", "and", "or", "xor", "shl", "sar", "shr", "imul") and a[2].endswith("," + b[2].split(",")[0]): c["testflags"] += 1
        if a[1] == "jmp" and re.match(r"^([0-9a-f]+)", a[2]) and int(a[2].split()[0], 16) == b[0]: c["jmpnext"] += 1
        if a[1].startswith("mov") and re.match(r"^\$0x0,%[a-z0-9]+$", a[2]): c["movzero"] += 1
    print("%-9s %6d " % (n, len(ins)) + " ".join("%8d" % c[p] for p in pats))
    for p in pats: tot[p] += c[p]
    tn += len(ins)
print("%-9s %6d " % ("total", tn) + " ".join("%8d" % tot[p] for p in pats))
