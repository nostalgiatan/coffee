#!/bin/bash
# The vectorizer's strided and offset loads (matrix product with a run-time size, `a[o + k]`, `b[k * 3 + 1]`, a column sum): the exit
# status of vecstride.cf must be the same natively and on the riscv64 targets (scalar, rva22, rva23 with vector loops).
# Usage: vecstride.sh <coffee2>
C=$(realpath "${1:?coffee2}"); H=$(dirname "$(realpath "$0")"); D=${TMPDIR:-$HOME/.cache/coffee-work}/rvbench; mkdir -p $D
$C $H/vecstride.cf -o $D/vs.x86 || exit 1; $D/vs.x86; want=$?
bad=0
for T in riscv64-linux riscv64-linux-rva22 riscv64-linux-rva23; do
  CPU=""; case $T in riscv64-linux-rva22) CPU="-cpu rva22u64";; riscv64-linux-rva23) CPU="-cpu rva23u64";; esac
  $C $H/vecstride.cf --target=$T --static -o $D/vs.$T || { bad=1; continue; }
  qemu-riscv64 $CPU $D/vs.$T; got=$?
  [ $got = $want ] || { echo "$T: $got, native $want"; bad=1; }
done
[ $bad = 0 ] && echo "vecstride: ok ($want)"
exit $bad
