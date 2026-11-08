#!/bin/bash
# Instructions executed (qemu-riscv64 -d exec, one instruction per translation block) by the Coffee riscv64 code against clang -O2 and
# -O1 of the same kernels in C; the exit status of all three must be equal.
# Usage: run.sh <coffee2> [name...]
C=$(realpath "${1:?coffee2}"); shift; H=$(dirname "$(realpath "$0")"); D=${TMPDIR:-$HOME/.cache/coffee-work}/rvbench; mkdir -p $D
names=${@:-"fib sieve matmul mandel collatz hash"}
count() { local rc n; qemu-riscv64 "$1" >/dev/null 2>&1; rc=$?; n=$(qemu-riscv64 -one-insn-per-tb -d nochain,exec -D /dev/fd/3 "$1" 3>&1 >/dev/null 2>&1 | grep -c '^Trace'); echo "$rc $n"; }
printf "%-8s %12s %12s %12s  %s\n" kernel coffee clang-O2 clang-O1 "coffee/O2   exits"
for n in $names; do
  $C $H/$n.cf --target=riscv64-linux --static -o $D/$n.cf.out || continue
  for o in 2 1; do
    clang --target=riscv64 -march=rv64gc -O$o -ffreestanding -fno-builtin -c $H/$n.c -o $D/$n.$o.o &&
    clang --target=riscv64 -march=rv64gc -O$o -ffreestanding -c $H/start.c -o $D/start.o &&
    clang --target=riscv64 -nostdlib -static -fuse-ld=lld $D/$n.$o.o $D/start.o -o $D/$n.c$o.out 2>/dev/null
  done
  read r0 i0 <<<"$(count $D/$n.cf.out)"; read r2 i2 <<<"$(count $D/$n.c2.out)"; read r1 i1 <<<"$(count $D/$n.c1.out)"
  printf "%-8s %12d %12d %12d  %5.2fx  %s\n" $n $i0 $i2 $i1 $(echo "$i0 / $i2" | bc -l) "$r0/$r2/$r1"
done
