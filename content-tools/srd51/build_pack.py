"""Write pack.json (manifest) and ATTRIBUTION.md for the srd-5.1 pack from the generated content files."""
import json, os, sys, glob
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import OUT
from build_misc import ATTRIBUTION, MODIFICATION_NOTE

ORDER = ['source.json', 'races.json', 'backgrounds.json', 'feats.json', 'rules/conditions.json']

def files():
    out = list(ORDER)
    for grp in ('classes', 'spells', 'items'):
        found = sorted(glob.glob(os.path.join(OUT, grp, '**', '*.json'), recursive=True),
                       key=lambda p: (os.path.relpath(p, OUT).count('/'), os.path.relpath(p, OUT)))
        def keyf(p):
            r = os.path.relpath(p, OUT)
            return (r.count('/'), r)
        out += [os.path.relpath(p, OUT).replace(os.sep, '/') for p in sorted(found, key=keyf)]
    return [f for f in out if os.path.exists(os.path.join(OUT, f))]

def main():
    fl = files()
    manifest = {
        'format': 'borealis-pack/1',
        'id': 'srd-5.1',
        'name': 'System Reference Document 5.1',
        'version': '1.0.0',
        'origin': 'core',
        'rulesets': ['dnd2014'],
        'license': 'CC-BY-4.0',
        'attribution': ATTRIBUTION + ' ' + MODIFICATION_NOTE,
        'description': 'Bundled 2014-rules content (fifth edition) from the SRD 5.1: races, classes with one subclass each, the Acolyte '
                       'background, the Grappler feat, spells, equipment and magic items.',
        'files': [{'path': f, 'kind': 'content'} for f in fl],
    }
    with open(os.path.join(OUT, 'pack.json'), 'w', encoding='utf-8') as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2); f.write('\n')
    att = f'''# Attribution

{ATTRIBUTION}

## Licence

The System Reference Document 5.1 is licensed by Wizards of the Coast LLC under the Creative Commons Attribution 4.0
International License (CC-BY-4.0), https://creativecommons.org/licenses/by/4.0/legalcode. Section 5 of CC-BY-4.0
(disclaimer of warranties and limitation of liability) applies.

## Changes made (CC-BY-4.0 section 3(a)(1)(B))

{MODIFICATION_NOTE} Mechanical rules (grant, select and stat rules), identifiers and support tags were added by the
Borealis authors so a character builder can compute statistics from the content. No material outside the SRD 5.1 is included.

This content is compatible with fifth edition.
'''
    with open(os.path.join(OUT, 'ATTRIBUTION.md'), 'w', encoding='utf-8') as f:
        f.write(att)
    print(len(fl), 'files listed')

if __name__ == '__main__':
    main()
