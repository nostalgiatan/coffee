#!/bin/bash
# usage: run.sh <coffee2>   the Win64 calling convention against gcc's ms_abi: functions of Coffee called from C and the other way round,
# registers kept, a frame of several pages. Needs gcc; runs on Linux (`--target=x86_64-linux-ms` is the convention in an ELF object).
C=$1; D=$(dirname "$(realpath "$0")"); T=${TMPDIR:-$HOME/.cache/coffee-work}/winabi; mkdir -p $T
$C $D/prog.cf -I $D --target=x86_64-linux-ms --emit-obj -o $T/prog.o || exit 1
# the C library is System V: the two functions the code calls itself get Win64 wrappers
objcopy --redefine-sym malloc=ms_malloc --redefine-sym free=ms_free $T/prog.o || exit 1
gcc -O1 -w -c $D/harness.c -o $T/harness.o || exit 1
gcc -no-pie -o $T/t $T/harness.o $T/prog.o || exit 1
$T/t
