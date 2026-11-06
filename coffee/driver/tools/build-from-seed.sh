#!/bin/bash
# Builds the self-hosted compiler without Rust or LLVM: the committed seed object is linked with
# the system C compiler, and the result compiles the sources again.
# Usage: tools/build-from-seed.sh <work dir>      (leaves <work dir>/coffee2 and <work dir>/tea)
# Refresh the seed after a compiler change: tools/build-from-seed.sh <work dir> --refresh
set -e
D=$(realpath -m "${1:?work dir}"); H=$(realpath "$(dirname "$0")/.."); CC=${CC:-cc}
mkdir -p "$D"; cd "$D"
INC="-I $H/src -I $H/../ir/src -I $H/../ld/src -I $H/../own/src -I $H/../types/src -I $H/../sema/src -I $H/../parser/src -I $H/../lexer/src"
gunzip -c "$H/seed/coffee2-x86_64.o.gz" > seed.o
$CC -o seed seed.o -lm
./seed "$H/src/main.cf" $INC --emit-obj -o gen1.o
$CC -o coffee2 gen1.o -lm
./coffee2 "$H/src/main.cf" $INC --emit-obj -o gen2.o
FIN=gen1.o
if ! cmp -s gen1.o gen2.o; then
    # the object format changed since the seed was made: the seed's output is in the old format, so
    # one more round is needed before two stages agree
    $CC -o coffee2 gen2.o -lm
    ./coffee2 "$H/src/main.cf" $INC --emit-obj -o gen3.o
    cmp gen2.o gen3.o || { echo "no fixed point: gen2 and gen3 objects differ"; exit 1; }
    FIN=gen2.o
fi
./coffee2 "$H/../tea/src/main.cf" -I "$H/../tea/src" -I "$H/src" -I "$H/../ir/src" -I "$H/../ld/src" -o tea
if cmp -s seed.o $FIN; then
    echo "seed is current; coffee2 built in $D"
elif [ "$2" = "--refresh" ]; then
    gzip -9 -n -c $FIN > "$H/seed/coffee2-x86_64.o.gz"
    echo "seed refreshed ($(wc -c < $FIN) bytes); coffee2 built in $D"
else
    echo "coffee2 built in $D; the seed is older than the sources (run with --refresh to update it)"
fi
