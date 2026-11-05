#!/bin/bash
# Bootstrap check: generation 1 (the Rust compiler) builds the driver, which compiles its own
# sources; that result compiles them again; the two LLVM texts must be identical.
# Usage: tools/bootstrap.sh <rust coffee binary> <work dir>
set -e
R=$(realpath "${1:?rust coffee binary}"); D=$(realpath -m "${2:?work dir}"); H=$(realpath "$(dirname "$0")/..")
mkdir -p "$D"; cp "$H/src/main.cf" "$D/main.cf"; cd "$D"
INC="-I $H/src -I $H/../ir/src -I $H/../types/src -I $H/../sema/src -I $H/../parser/src -I $H/../lexer/src"
$R --test-mode $INC main.cf --bin -o gen2 >/dev/null
./gen2 main.cf $INC --emit-llvm -o gen2.ll
clang -w -O0 -o gen3 gen2.ll -lm
./gen3 main.cf $INC --emit-llvm -o gen3.ll
cmp gen2.ll gen3.ll && echo "fixed point: gen2 and gen3 produce identical LLVM text ($(wc -c < gen3.ll) bytes)"
