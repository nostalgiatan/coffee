#!/bin/sh
# Source place of a code address: cfline.sh <exe built with -g> <file made by --lines> <hex address>
# Prints function+offset and file:line. For JIT code use the .lines of cfjit (lines of `--emit-flat`)
# with the function address from /tmp/perf-PID.map.
exe=${1:?exe}; lines=${2:?lines file}; addr=${3:?hex address}
a=$(printf '%d' "$addr")
nm -n "$exe" | awk -v a="$a" '
  function hex(s,  i,c,v,d){ v=0; for(i=1;i<=length(s);i++){ c=index("0123456789abcdef",substr(s,i,1))-1; v=v*16+c } return v }
  { t=$2; if (t=="T"||t=="t") { v=hex($1); if (v<=a) { fn=$3; at=v } } }
  END { if (fn=="") exit 1; print fn, a-at }' | {
  read fn off || exit 1
  awk -v fn="$fn" -v off="$off" '
    $1=="F" { cur=($2==fn); next }
    cur && $1=="L" { if ($2+0 <= off) best=$3 }
    END { printf "%s+%d %s\n", fn, off, (best==""?"?":best) }' "$lines"
}
