#!/bin/sh
# Builds libcfjit.a, libcfjit.so and copies cfjit.h: build-lib.sh <coffee2> <out dir>
# The library is the JIT's modules (embed.cf) as one object. `--pic` makes the compiler put the system call
# function (the target's `tg_syscall_stub`) in it; the rest is the C library of the host. cc only links.
set -e
C=$(realpath "${1:?path of coffee2}"); O=${2:?out dir}
D=$(cd "$(dirname "$0")/.." && pwd); R=$D/..
mkdir -p "$O"
"$C" "$D/src/embed.cf" -I "$D/src" -I "$R/ld/src" -I "$R/driver/src" -I "$R/ir/src" -I "$R/own/src" -I "$R/types/src" -I "$R/sema/src" -I "$R/parser/src" -I "$R/lexer/src" --emit-obj --pic -o "$O/cfjit.o"

rm -f "$O/libcfjit.a"
ar rcs "$O/libcfjit.a" "$O/cfjit.o"
cc -shared -o "$O/libcfjit.so" "$O/cfjit.o"
cp "$D/include/cfjit.h" "$O/cfjit.h"
