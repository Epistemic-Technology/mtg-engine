#!/bin/bash
# word-level three-way merge of a conflicted file (long table lines)
f=$1; T=$(mktemp -d)
for i in 1 2 3; do git show :$i:$f | python3 -c "
import sys
s=sys.stdin.read()
sys.stdout.write(s.replace('\n','\n\x01\n').replace(' ',' \n'))" > $T/$i; done
git merge-file -p $T/2 $T/1 $T/3 > $T/m; echo "wordmerge rc=$? conflicts=$(grep -c '^<<<<<<<' $T/m)"
python3 -c "
s=open('$T/m').read()
open('$f','w').write(s.replace(' \n',' ').replace('\n\x01\n','\n'))"
