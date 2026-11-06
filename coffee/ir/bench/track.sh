#!/bin/bash
# Compile-time and size tracker: the instructions (callgrind Ir) the compiler needs to compile
# its own driver, and the size of the compiler that results. One line is appended to TRACK.tsv.
# Usage: bench/track.sh <coffee2> [label]       (needs valgrind; run it after each commit)
C=$(realpath "${1:?path of coffee2}")
B=$(dirname "$(realpath "$0")"); R=$B/../..
D=$(mktemp -d /tmp/coffee_track.XXXXXX); trap 'rm -rf "$D"' EXIT
cd "$R/driver" || exit 1
INC=""; for d in ../ir/src ../ld/src ../own/src ../types/src ../sema/src ../parser/src ../lexer/src src; do INC="$INC -I $d"; done
ir=$(valgrind --tool=callgrind --callgrind-out-file=/dev/null "$C" src/main.cf $INC -o "$D/y" 2>&1 | grep Collected | awk '{print $4}')
"$C" src/main.cf $INC --static -o "$D/x" >/dev/null 2>&1
printf "%s\t%s\t%s\t%s\n" "$(git rev-parse --short HEAD)" "${2:-}" "$ir" "$(stat -c %s "$D/x")" | tee -a "$B/TRACK.tsv"
