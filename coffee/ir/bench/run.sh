#!/bin/bash
# Compile-time and run-time measurements; best of 3 runs each.
# Usage: bench/run.sh <rust coffee binary> <scratch dir inside the current directory tree>
# First build and run bench/emit.cf (it writes /tmp/claude_bench_<program>_<opt>.ll), see RESULTS.md.
R=$(realpath "${1:?coffee binary}"); D=${2:?scratch dir}; B=$(dirname "$(realpath "$0")")
mkdir -p "$D"; cp "$B"/fib.cf "$B"/acc.cf "$B"/sieve.cf "$D"; cd "$D" || exit 1
best() { local b=999; for i in 1 2 3; do local s=$(date +%s.%N); "$@" >/dev/null 2>&1; local d=$(echo "$(date +%s.%N) - $s" | bc); b=$(echo "if ($d < $b) $d else $b" | bc); done; printf "%.3f s" "$b"; }
for p in fib acc sieve; do
  case $p in fib) k=0;; acc) k=1;; sieve) k=2;; esac
  echo "== $p"
  for o in 0 2 3; do
    printf "rust coffee -O$o:          compile "; best "$R" --test-mode $p.cf --bin -O$o -o r$p$o
    printf "  run "; best ./r$p$o; echo
  done
  for opt in 0 1; do
    for o in 0 2; do
      printf "ir opt=$opt + clang -O$o:   compile "; best clang -w -O$o -o s$p$opt$o /tmp/claude_bench_${k}_$opt.ll
      printf "  run "; best ./s$p$opt$o; echo
    done
  done
done
