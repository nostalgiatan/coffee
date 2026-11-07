#!/bin/sh
# Runs each test file of a stage directory natively and through cfjit and compares the exit code
# and the output. usage: compare.sh <coffee2> <cfjit> <stage dir> [files...]
# For another machine: JITRUN=qemu-aarch64 CIRARGS=--target=aarch64 JITARGS=--target=aarch64 (cfjit built for that machine)
# With SANDBOX=1 cfjit runs the code in the sandbox (stdin, stdout and stderr only).
c2=$1; jit=$2; dir=$3; shift 3
inc=""
for d in $(sed -n 's/^include = \[\(.*\)\]/\1/p' "$dir/coffee.toml" | tr -d '", ' | tr ']' '\n' | head -1 | tr '[' ' '); do :; done
incs=$(grep '^include' "$dir/coffee.toml" | tail -1 | sed 's/.*\[\(.*\)\].*/\1/' | tr -d '",' )
for d in $incs; do inc="$inc -I $dir/$d"; done
tmp=$(mktemp -d)
sbx=""
[ "$SANDBOX" = 1 ] && sbx="--sandbox"
[ "$SANDBOX" = base ] && sbx="--sandbox=base"
files="$@"
[ -z "$files" ] && files=$(ls $dir/tests/test_*.cf)
pass=0; fail=0
for f in $files; do
  n=$(basename $f .cf)
  $c2 $f $inc --static -o $tmp/$n.nat >/dev/null 2>$tmp/$n.cerr || { echo "SKIP $n (no native build)"; continue; }
  $c2 $f $inc --emit-cir $CIRARGS -o $tmp/$n.cir >/dev/null 2>$tmp/$n.cerr || { echo "NOCIR $n"; fail=$((fail+1)); continue; }
  (cd $dir && timeout 60 $tmp/$n.nat >$tmp/$n.o1 2>/dev/null; echo $? >$tmp/$n.r1)
  (cd $dir && timeout 60 $JITRUN $jit $tmp/$n.cir $sbx $JITARGS >$tmp/$n.o2 2>/dev/null; echo $? >$tmp/$n.r2)
  if cmp -s $tmp/$n.r1 $tmp/$n.r2 && cmp -s $tmp/$n.o1 $tmp/$n.o2; then pass=$((pass+1)); else echo "DIFF $n native=$(cat $tmp/$n.r1) jit=$(cat $tmp/$n.r2)"; fail=$((fail+1)); fi
done
echo "pass $pass fail $fail"
rm -rf $tmp
