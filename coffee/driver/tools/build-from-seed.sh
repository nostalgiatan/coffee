#!/bin/bash
# Builds the self-hosted compiler with no C compiler, no Rust and no LLVM: the committed seed is a
# static x86-64 executable of the compiler; it compiles the sources, and the result compiles them again.
# Usage: tools/build-from-seed.sh <work dir>      (leaves <work dir>/coffee2, <work dir>/tea, <work dir>/cfjit and <work dir>/cfprof)
# Refresh the seed after a compiler change: tools/build-from-seed.sh <work dir> --refresh
set -e
D=$(realpath -m "${1:?work dir}"); H=$(realpath "$(dirname "$0")/.."); cd "$H" ; mkdir -p "$D"; cd "$D"
INC="-I $H/src -I $H/../ir/src -I $H/../ld/src -I $H/../own/src -I $H/../types/src -I $H/../sema/src -I $H/../parser/src -I $H/../lexer/src"
gunzip -c "$H/seed/coffee2-x86_64.gz" > seed
chmod +x seed
./seed "$H/src/main.cf" $INC -o gen1
./gen1 "$H/src/main.cf" $INC -o gen2
FIN=gen1
if ! cmp -s gen1 gen2; then
    # the seed is older than a change in what the compiler writes: one more round
    ./gen2 "$H/src/main.cf" $INC -o gen3
    cmp gen2 gen3 || { echo "no fixed point: gen2 and gen3 differ"; exit 1; }
    FIN=gen2
fi
cp $FIN coffee2
./coffee2 "$H/../tea/src/main.cf" -I "$H/../tea/src" -I "$H/src" -I "$H/../ir/src" -I "$H/../ld/src" -I "$H/../lexer/src" -o tea
./coffee2 "$H/../jit/src/main.cf" -I "$H/../jit/src" $INC --static -o cfjit
./coffee2 "$H/../prof/src/main.cf" -I "$H/../prof/src" $INC --static -o cfprof
if cmp -s seed $FIN; then
    echo "seed is current; coffee2 built in $D"
elif [ "$2" = "--refresh" ]; then
    gzip -9 -n -c $FIN > "$H/seed/coffee2-x86_64.gz"
    echo "seed refreshed ($(wc -c < $FIN) bytes); coffee2 built in $D"
else
    echo "coffee2 built in $D; the seed is older than the sources (run with --refresh to update it)"
fi
