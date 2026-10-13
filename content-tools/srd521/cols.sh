#!/bin/zsh
P=../../../input/srd/SRD_CC_v5.2.1.pdf
rm -f cols.txt
for p in {1..364}; do
  echo "=====PAGE $p" >> cols.txt
  pdftotext -layout -f $p -l $p -x 0 -y 0 -W 297 -H 783 $P - >> cols.txt
  echo "=====RIGHT" >> cols.txt
  pdftotext -layout -f $p -l $p -x 297 -y 0 -W 297 -H 783 $P - >> cols.txt
done
