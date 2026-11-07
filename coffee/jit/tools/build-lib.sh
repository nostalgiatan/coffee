#!/bin/sh
# Builds libcfjit.a, libcfjit.so and copies cfjit.h: build-lib.sh <coffee2> <out dir>
# The library is the JIT's modules (embed.cf) as one object, plus a few assembler lines that give it the
# system call function (src/shim_<arch>.S, assembled by cc); it uses the host's C library for the rest.
set -e
C=$(realpath "${1:?path of coffee2}"); O=${2:?out dir}
D=$(cd "$(dirname "$0")/.." && pwd); R=$D/..
mkdir -p "$O"
"$C" "$D/src/embed.cf" -I "$D/src" -I "$R/ld/src" -I "$R/driver/src" -I "$R/ir/src" -I "$R/own/src" -I "$R/types/src" -I "$R/sema/src" -I "$R/parser/src" -I "$R/lexer/src" --emit-obj --pic -o "$O/cfjit.o"
cc -c -fPIC -o "$O/shim.o" "$D/src/shim_$(uname -m).S"
rm -f "$O/libcfjit.a"
ar rcs "$O/libcfjit.a" "$O/cfjit.o" "$O/shim.o"
cc -shared -o "$O/libcfjit.so" "$O/cfjit.o" "$O/shim.o"
cp "$D/include/cfjit.h" "$O/cfjit.h"
