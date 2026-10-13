#!/usr/bin/env python3
"""Step 1: column-aware text extraction of the SRD 5.1 monster pages -> raw_cols.txt (scratch).

Pages are two-column; plain/-layout pdftotext interleaves them, so each page is cropped
into a left and a right column and read left-then-right.
"""
import re, subprocess, sys
from pathlib import Path
HERE = Path(__file__).resolve().parent
PDF = HERE.parents[2] / "input" / "srd" / "SRD_CC_v5.1.pdf"   # <repo parent>/input/srd/ (maintainer-local, see content-tools/README.md)
FIRST, LAST = 260, 403
def crop(page, x):
    out = subprocess.run(["pdftotext", "-f", str(page), "-l", str(page), "-layout", "-x", str(x), "-y", "0",
                          "-W", "306", "-H", "792", str(PDF), "-"], capture_output=True, check=True).stdout.decode("utf8")
    out = out.replace("\xa0", " ").replace("\t", " ").replace("\r", " ").replace("\x0c", "")
    out = re.sub(r" {2,}", " ", out)
    return [l.strip() for l in out.split("\n")]
lines = []
for p in range(FIRST, LAST + 1):
    for x in (0, 306):
        lines.append(f"@@COL {p} {x}")
        lines.extend(crop(p, x))
(HERE / "raw_cols.txt").write_text("\n".join(lines))
print("pages", FIRST, LAST, "lines", len(lines))
