"""Identifier-only index (id -> name,type) of the legacy fixtures, used ONLY to fill maps_to/renamed_from links.
Output ids2014.json is git-ignored; fixtures are never copied."""
import os,re,glob,json
idx={}
for f in glob.glob(os.environ.get('BOREALIS_ID_FIXTURES','../../../fixtures/legacy-xml')+'/*.xml'):
    s=open(f,encoding='utf-8',errors='ignore').read()
    for m in re.finditer(r'<element\s+([^>]*)>',s):
        a=dict(re.findall(r'(\w+)="([^"]*)"',m.group(1)))
        if 'id' in a: idx[a['id']]=(a.get('name'),a.get('type'))
json.dump(idx,open('ids2014.json','w'))
print(len(idx),'legacy ids')
