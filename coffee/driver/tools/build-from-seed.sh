#!/bin/bash
# Builds the self-hosted compiler with no C compiler, no Rust and no LLVM: the committed seed is a
# static executable of the compiler (x86-64 or aarch64, by the machine); it compiles the sources, and the
# result compiles them again.
# Usage: tools/build-from-seed.sh <work dir>      (leaves <work dir>/coffee2, <work dir>/tea, <work dir>/cfjit and <work dir>/cfprof)
# Refresh the seeds after a compiler change: tools/build-from-seed.sh <work dir> --refresh
# On x86-64 the aarch64 seed is the compiler cross compiled for aarch64 (--target=aarch64-linux --static); it is
# compared with the committed one and refreshed with it. On aarch64 only the aarch64 seed is built and refreshed.
set -e
D=$(realpath -m "${1:?work dir}"); H=$(realpath "$(dirname "$0")/.."); cd "$H" ; mkdir -p "$D"; cd "$D"
INC="-I $H/src -I $H/../ir/src -I $H/../ld/src -I $H/../own/src -I $H/../types/src -I $H/../sema/src -I $H/../parser/src -I $H/../lexer/src"
case "$(uname -m)" in
    aarch64|arm64) ARCH=aarch64;;
    *) ARCH=x86_64;;
esac
gunzip -c "$H/seed/coffee2-$ARCH.gz" > seed
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
STATE=current
if ! cmp -s seed $FIN; then
    STATE=old
    if [ "$2" = "--refresh" ]; then
        gzip -9 -n -c $FIN > "$H/seed/coffee2-$ARCH.gz"
        STATE=refreshed
    fi
fi
# the aarch64 seed, from an x86-64 host
A64=current
if [ "$ARCH" = x86_64 ]; then
    ./coffee2 "$H/src/main.cf" $INC --target=aarch64-linux --static -o seed-aarch64
    if [ -f "$H/seed/coffee2-aarch64.gz" ] && gunzip -c "$H/seed/coffee2-aarch64.gz" | cmp -s - seed-aarch64; then
        :
    else
        A64=old
        if [ "$2" = "--refresh" ]; then
            gzip -9 -n -c seed-aarch64 > "$H/seed/coffee2-aarch64.gz"
            A64=refreshed
        fi
    fi
fi
if [ "$STATE" = current ] && [ "$A64" = current ]; then
    echo "seed is current; coffee2 built in $D"
elif [ "$STATE" = old ] || [ "$A64" = old ]; then
    echo "coffee2 built in $D; the seed is older than the sources (run with --refresh to update it)"
else
    echo "seed refreshed ($(wc -c < $FIN) bytes); coffee2 built in $D"
fi
