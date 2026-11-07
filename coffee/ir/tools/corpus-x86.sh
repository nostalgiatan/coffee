#!/bin/bash
# usage: corpus-x86.sh <coffee2> <outdir>   compiles a corpus and writes one sha256 line per output
# Compare two compilers by running this once per compiler on the same, unchanged tree and diffing the two .sha files.
# The output is about 850 MB: give a directory on disk (not /tmp, which is memory), and delete it after comparing the .sha files.
C=$1; O=$2; rm -rf $O; mkdir -p $O; cd "$(dirname "$(realpath "$0")")/../.."
INC="-I driver/src -I ir/src -I ld/src -I own/src -I types/src -I sema/src -I parser/src -I lexer/src"
$C driver/src/main.cf $INC --static -o $O/compiler.static 2>/dev/null
$C driver/src/main.cf $INC -O0 --emit-obj -o $O/compiler.O0 2>/dev/null
$C driver/src/main.cf $INC --emit-cir -o $O/compiler.cir 2>/dev/null
for d in lexer parser sema types own ir ld jit driver tea; do
  inc=$(grep '^include' $d/coffee.toml | tail -1 | sed 's/.*\[\(.*\)\].*/\1/' | tr -d '",')
  I=""; for x in $inc; do I="$I -I $d/$x"; done
  for f in $d/tests/test_*.cf; do
    n=$d.$(basename $f .cf)
    $C $f $I --static -o $O/$n.static 2>/dev/null
    $C $f $I --emit-cir -o $O/$n.cir 2>/dev/null
    $C $f $I -O0 --emit-obj -o $O/$n.O0 2>/dev/null
    $C $f $I -g --emit-obj -o $O/$n.g 2>/dev/null
  done
done
inc=$(grep '^include' compat/coffee.toml | tail -1 | sed 's/.*\[\(.*\)\].*/\1/' | tr -d '",'); I=""; for x in $inc; do I="$I -I compat/$x"; done
for f in compat/tests/*.cf; do n=compat.$(basename $f .cf); $C $f $I --static -o $O/$n.static 2>/dev/null; $C $f $I --emit-cir -o $O/$n.cir 2>/dev/null; done
# sandbox images, both forms, and the vector programs
for f in /tmp/pk/red.cf /tmp/pk/ms.cf /tmp/pk/fibs.cf /tmp/pk/vt.cf; do
  n=$(basename $f .cf); $C $f --static -o $O/x.$n.static 2>/dev/null; $C $f --emit-cir -o $O/x.$n.cir 2>/dev/null
  for m in "" --sandbox --sandbox=base; do [ -x /tmp/w/cfjit ] && /tmp/w/cfjit $O/x.$n.cir $m --no-run --cache >/dev/null 2>&1; done
  cp $O/x.$n.cir.* $O/ 2>/dev/null
done
(cd $O && sha256sum * | sort -k2 > ../$(basename $O).sha)
echo "$(wc -l < $O/../$(basename $O).sha) outputs"
