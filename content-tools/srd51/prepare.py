"""Prepare the scratch inputs for the srd-5.1 build (all outputs are gitignored).

  srd.xml          pdftohtml -xml of input/srd/SRD_CC_v5.1.pdf (positions + fonts)
  events.pkl       ordered heading / paragraph / bullet / table events (srdxml.events)
  fixture_ids.json (type, id, name) rows read from the legacy fixtures; used ONLY to align element ids
                   so imported characters resolve to the same ids. Text and rules are never taken from them.

Run:  python3 -I prepare.py
"""
import glob, json, os, pickle, re, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
ROOT = os.path.normpath(os.path.join(HERE, '..', '..', '..'))
PDF = os.path.join(ROOT, 'input', 'srd', 'SRD_CC_v5.1.pdf')
# Optional, maintainer-local id-alignment fixtures (not included in this repository).
FIXTURES = os.environ.get('BOREALIS_ID_FIXTURES', os.path.join(ROOT, 'fixtures', 'legacy-xml'))

def main():
    with open(os.path.join(HERE, 'srd.xml'), 'wb') as f:
        subprocess.run(['pdftohtml', '-xml', '-i', '-stdout', PDF], stdout=f, stderr=subprocess.DEVNULL, check=True)
    import srdxml
    ev = srdxml.events()
    with open(os.path.join(HERE, 'events.pkl'), 'wb') as f:
        pickle.dump(ev, f)
    print('events:', len(ev))
    rows = []
    for path in sorted(glob.glob(os.path.join(FIXTURES, '*.xml'))):
        s = open(path, encoding='utf-8-sig').read()
        short = path.split('Data.')[-1]
        for m in re.finditer(r'<element\s+([^>]*)>', s):
            a = dict(re.findall(r'(\w+)="([^"]*)"', m.group(1)))
            rows.append((short, a['type'], a['id'], a['name']))
    with open(os.path.join(HERE, 'fixture_ids.json'), 'w') as f:
        json.dump(rows, f)
    print('fixture id rows:', len(rows))

if __name__ == '__main__':
    main()
