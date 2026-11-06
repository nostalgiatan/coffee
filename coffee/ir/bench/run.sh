#!/bin/bash
# Run time and code size of the benchmark programs, with a check of their output.
# Usage: bench/run.sh <coffee2> [runs]      (user time in seconds, best of `runs`, default 5)
# The output of every program is compared with bench/expected.txt, so a miscompiled
# program shows up as WRONG instead of as a fast result.
C=$(realpath "${1:?path of coffee2}"); N=${2:-5}
B=$(dirname "$(realpath "$0")"); L=$B/../../lexer/src
D=$(mktemp -d /tmp/coffee_bench.XXXXXX); trap 'rm -rf "$D"' EXIT
printf "%-9s %9s %10s %9s  %s\n" program "run (s)" "code (B)" "build (s)" check
for p in fib acc sieve collatz mandel sort hash objects strings matmul bits; do
  s=$(date +%s.%N)
  "$C" "$B/$p.cf" -I "$L" --static -o "$D/$p" >/dev/null 2>&1 || { printf "%-9s  compile failed\n" $p; continue; }
  e=$(date +%s.%N); bt=$(echo "$e - $s" | bc)
  "$C" "$B/$p.cf" -I "$L" --emit-obj -o "$D/$p.o" >/dev/null 2>&1
  sz=$(stat -c %s "$D/$p.o")
  best=999
  for i in $(seq "$N"); do
    out=$( { /usr/bin/time -f "%U" "$D/$p" > "$D/out" 2>"$D/err"; echo "rc=$?" >> "$D/out"; } 2>&1 )
    t=$(tail -1 "$D/err")
    best=$(echo "if ($t < $best) $t else $best" | bc)
  done
  got=$(tr '\n' ' ' < "$D/out" | sed 's/ *$//')
  want=$(grep "^$p " "$B/expected.txt" | cut -d' ' -f2-)
  ok=ok; [ "$got" == "$want" ] || ok="WRONG (got: $got)"
  printf "%-9s %9.2f %10d %9.2f  %s\n" $p "$best" "$sz" "$bt" "$ok"
done
