#!/bin/bash
# Bootstrap check: generation 1 (the Rust compiler) builds the driver, which compiles its own
# sources to an object; that object, linked, compiles them again; the two objects must be identical.
# Usage: tools/bootstrap.sh <rust coffee binary> <work dir>
set -e
R=$(realpath "${1:?rust coffee binary}"); D=$(realpath -m "${2:?work dir}"); H=$(realpath "$(dirname "$0")/..")
mkdir -p "$D"; cp "$H/src/main.cf" "$D/main.cf"; cd "$D"
INC="-I $H/src -I $H/../ir/src -I $H/../own/src -I $H/../types/src -I $H/../sema/src -I $H/../parser/src -I $H/../lexer/src"
$R --test-mode $INC main.cf --bin -o gen2 >/dev/null
./gen2 main.cf $INC --emit-obj -o gen2.o
cc -o gen3 gen2.o -lm
./gen3 main.cf $INC --emit-obj -o gen3.o
cmp gen2.o gen3.o && echo "fixed point: gen2 and gen3 produce identical objects ($(wc -c < gen3.o) bytes)"
