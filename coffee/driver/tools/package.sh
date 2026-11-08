#!/bin/sh
# Builds the release archive of one target from a work dir that has the native `coffee2` (see build-from-seed.sh).
# usage: package.sh <work dir> <version> <target> <out dir>
#   target: x86_64-linux (the native tools), riscv64-linux, riscv64-linux-rva23, aarch64-linux (cross compiled, static)
# The archive holds bin/{coffee2,tea,cfjit,cfprof,cfld}, library/, README.md and SYNTAX.md.
set -eu
W=$(cd "$1" && pwd)
V=$2
T=$3
O=$4
cd "$(dirname "$0")/../.."
R=$(cd .. && pwd)
INC="-I driver/src -I ir/src -I ld/src -I own/src -I types/src -I sema/src -I parser/src -I lexer/src"
P="coffee-$V-${T%%-rva*}"
case "$T" in *rva23) P="coffee-$V-linux-riscv64-rva23";; riscv64-linux) P="coffee-$V-linux-riscv64";; aarch64-linux) P="coffee-$V-linux-aarch64";; x86_64-linux) P="coffee-$V-linux-x86_64";; esac
D="$O/$P"
rm -rf "$D"
mkdir -p "$D/bin"
if [ "$T" = x86_64-linux ]; then
  for t in coffee2 tea cfjit cfprof; do cp "$W/$t" "$D/bin/"; done
  "$W/coffee2" ld/src/main.cf -I ld/src $INC -I rt/src --static -o "$D/bin/cfld"
else
  "$W/coffee2" driver/src/main.cf $INC -I rt/src --target=$T --static -o "$D/bin/coffee2"
  "$W/coffee2" tea/src/main.cf -I tea/src $INC -I rt/src --target=$T --static -o "$D/bin/tea"
  "$W/coffee2" jit/src/main.cf -I jit/src $INC --target=$T --static -o "$D/bin/cfjit"
  "$W/coffee2" prof/src/main.cf -I prof/src $INC --target=$T --static -o "$D/bin/cfprof"
  "$W/coffee2" ld/src/main.cf -I ld/src $INC -I rt/src --target=$T --static -o "$D/bin/cfld"
fi
cp -r "$R/library" "$D/library"
cp "$R/README.md" "$R/SYNTAX.md" "$D/"
tar -C "$O" -czf "$O/$P.tar.gz" "$P"
(cd "$O" && sha256sum "$P.tar.gz" > "$P.tar.gz.sha256")
echo "$O/$P.tar.gz"
