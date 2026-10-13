import json,sys,os,glob
from jsonschema import Draft202012Validator
S=json.load(open('../../crates/borealis-content/schema/content-format.schema.json'))
V=Draft202012Validator(S)
root='../../content/srd-5.2.1'
errs=0;n=0;ids={}
for f in sorted(glob.glob(root+'/**/*.json',recursive=True)):
    if f.endswith('pack.json'): continue
    d=json.load(open(f))
    for e in V.iter_errors(d):
        errs+=1
        if errs<15: print(f,list(e.absolute_path)[:4],e.message[:200])
    for el in d['elements']:
        n+=1
        if el['id'] in ids: print('DUP',el['id'],f,ids[el['id']]); errs+=1
        ids[el['id']]=f
print('elements',n,'schema errors',errs)
