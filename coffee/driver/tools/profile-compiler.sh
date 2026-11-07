#!/bin/sh
# Profiles the compiler compiling itself: builds a copy with debug information, runs it under cfprof.
# usage: profile-compiler.sh [coffee2 dir (default: the dir of the coffee2 in PATH)] [cfprof options...]
# Needs coffee2 and cfprof (coffee/driver/tools/build-from-seed.sh makes both).
set -e
H=$(cd "$(dirname "$0")/.." && pwd)
INC=$(sed -n 's/^include = \[\(.*\)\]/\1/p' "$H/coffee.toml" | head -1 | tr -d '",' | tr ' ' '\n' | sed "s#^#-I $H/#" | tr '\n' ' ')
T=$(mktemp -d)
coffee2 "$H/src/main.cf" $INC --static -g -O1 --lines "$T/c.lines" -o "$T/coffee2g"
cfprof --lines "$T/c.lines" "$@" -- "$T/coffee2g" "$H/src/main.cf" $INC --static -o "$T/out"
rm -rf "$T"
