#!/bin/zsh
# Rebuild app/content/srd-5.2.1 from input/srd/SRD_CC_v5.2.1.pdf.  Run from this directory.
# Needs: pdftotext (poppler), python3.  Optional: .venv with jsonschema (python3 -m venv .venv && .venv/bin/pip install jsonschema).
set -e
PDF=../../../input/srd/SRD_CC_v5.2.1.pdf
[ -f srd.txt ]   || pdftotext -layout $PDF srd.txt
[ -f cols.txt ]  || ./cols.sh
[ -f pages.pkl ] || python3 -I mkpages.py
python3 -I gen_ids2014.py
python3 -I - <<'PY'
import sys; sys.path.insert(0,'.')
from common import *
open('equip.txt','w').write(text(89,103)); open('origins.txt','w').write(text(83,88)); open('creation.txt','w').write(text(19,27))
open('gear.txt','w').write(text(94,103)); open('tools.txt','w').write(text(93,94)); open('gloss.txt','w').write(text(176,191))
PY
python3 -I spells.py >/dev/null
python3 -I build_spells.py
python3 -I magic.py >/dev/null
python3 -I build_magic.py
python3 -I assemble.py
python3 -I docs.py
.venv/bin/python validate.py
python3 -I xref.py
