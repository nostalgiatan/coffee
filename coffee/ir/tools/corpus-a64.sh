#!/bin/bash
# usage: corpus-a64.sh <coffee2> <outdir>   aarch64 objects/static programs of a corpus, one sha256 line each
# Compare two compilers by running this once per compiler on the same, unchanged tree and diffing the two .sha files.
# The output is about 850 MB: give a directory on disk (not /tmp, which is memory), and delete it after comparing the .sha files.
C=$1; O=$2; rm -rf $O; mkdir -p $O; cd "$(dirname "$(realpath "$0")")/../.."
INC="-I driver/src -I ir/src -I ld/src -I own/src -I types/src -I sema/src -I parser/src -I lexer/src"
T="--target=aarch64-linux"
$C driver/src/main.cf $INC $T --static -o $O/compiler.static 2>/dev/null
$C driver/src/main.cf $INC $T -O0 --emit-obj -o $O/compiler.O0 2>/dev/null
for p in ir/bench/*.cf; do n=bench.$(basename $p .cf); $C $p -I lexer/src $T --static -o $O/$n.static 2>/dev/null; $C $p -I lexer/src $T --emit-obj -o $O/$n.o 2>/dev/null; $C $p -I lexer/src $T -O0 --emit-obj -o $O/$n.O0 2>/dev/null; done
for d in lexer parser sema types own; do
  inc=$(grep '^include' $d/coffee.toml | tail -1 | sed 's/.*\[\(.*\)\].*/\1/' | tr -d '",'); I=""; for x in $inc; do I="$I -I $d/$x"; done
  for f in $d/tests/test_*.cf; do n=$d.$(basename $f .cf); $C $f $I $T --static -o $O/$n.static 2>/dev/null; $C $f $I $T -g --emit-obj -o $O/$n.g 2>/dev/null; done
done
for f in ir/tools/a64progs/*.cf; do n=prog.$(basename $f .cf); $C $f $T --static -o $O/$n.static 2>/dev/null; done
(cd $O && sha256sum * > ../$(basename $O).sha)
