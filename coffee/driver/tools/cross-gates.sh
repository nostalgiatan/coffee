#!/bin/sh
# The gates of the other targets, under qemu-user (qemu-riscv64, qemu-aarch64): the ported cases run on
# riscv64, rva22, rva23 and aarch64, inside the sandbox of a `cfjit` built for the target, and the
# compiler built for riscv64 and aarch64 compiles itself to the same bytes.
# usage: cross-gates.sh <dir with coffee2> [work dir]
set -eu
W=$(cd "$1" && pwd)
T=${2:-$W/cross}
mkdir -p "$T"
cd "$(dirname "$0")/../.."
I="-I driver/src -I ir/src -I ld/src -I own/src -I types/src -I sema/src -I parser/src -I lexer/src -I rt/src"
J="-I jit/src -I driver/src -I ir/src -I ld/src -I own/src -I types/src -I sema/src -I parser/src -I lexer/src"
bad=0
for t in riscv64-linux riscv64-linux-rva22 riscv64-linux-rva23 aarch64-linux; do
  echo "== cases $t"
  python3 ir/tools/a64cases.py "$W/coffee2" --target=$t -j 4 | tail -3 | tee "$T/cases.$t"
  grep -q "'ok': 191} of 191" "$T/cases.$t" || bad=1
done
for t in riscv64 aarch64; do
  echo "== sandbox $t"
  "$W/coffee2" jit/src/main.cf $J --target=$t-linux --static -o "$T/cfjit.$t"
  python3 ir/tools/a64cases.py "$W/coffee2" --target=$t-linux --sandbox="$T/cfjit.$t" -j 4 | tail -3 | tee "$T/sbx.$t"
  grep -q "'ok': 191} of 191" "$T/sbx.$t" || bad=1
done
for t in riscv64:qemu-riscv64 aarch64:qemu-aarch64; do
  a=${t%%:*}; q=${t#*:}
  echo "== self-compile $a"
  "$W/coffee2" driver/src/main.cf $I --target=$a-linux --static -o "$T/c2.$a"
  "$q" "$T/c2.$a" driver/src/main.cf $I --target=$a-linux --static -o "$T/c2b.$a"
  cmp "$T/c2.$a" "$T/c2b.$a" || bad=1
done
exit $bad
