import sys, os, json; sys.path.insert(0,'.')
import gen_misc as M
from lib import OUT
DEST='../../content/srd-5.2.1'
ATTR=('This work includes material from the System Reference Document 5.2.1 ("SRD 5.2.1") by Wizards of the Coast LLC, available at https://www.dndbeyond.com/srd. '
      'The SRD 5.2.1 is licensed under the Creative Commons Attribution 4.0 International License, available at https://creativecommons.org/licenses/by/4.0/legalcode.')
MODNOTE=('Modified: the SRD text was restructured into Borealis JSON content files (element records, rules, setters and tables); descriptions were reflowed and converted to HTML fragments, and game rules were encoded as rules. No other attribution to Wizards of the Coast or its parent or affiliates is made.')
ORDER=['base','species','backgrounds','feats','equipment','classes','spells','magic-items','rules']
def sortkey(p):
    return (ORDER.index(p.split('/')[0]),p)
import shutil
if os.path.isdir(DEST):
    for f in os.listdir(DEST):
        fp=os.path.join(DEST,f)
        if os.path.isdir(fp): shutil.rmtree(fp)
        elif f.endswith('.json'): os.remove(fp)
def fix_keys(o):
    if isinstance(o,dict):
        return {('from' if k=='from_' else k):fix_keys(v) for k,v in o.items()}
    if isinstance(o,list): return [fix_keys(x) for x in o]
    return o
files=[]
for path in sorted(OUT,key=sortkey):
    els=fix_keys(OUT[path])
    doc={'format':'borealis-content/1','info':{'name':'SRD 5.2.1: '+path.rsplit('.',1)[0],'description':'Bundled 2024-rules content, SRD 5.2.1 (CC-BY-4.0).','update':{'version':'1.0.0'}},'ruleset':'dnd2024','elements':els}
    full=os.path.join(DEST,path); os.makedirs(os.path.dirname(full),exist_ok=True)
    with open(full,'w',encoding='utf-8') as f: json.dump(doc,f,ensure_ascii=False,indent=1); f.write('\n')
    files.append({'path':path,'kind':'content'})
manifest={'format':'borealis-pack/1','id':'srd-5.2.1','name':'SRD 5.2.1 (2024 rules)','version':'1.0.0','origin':'core','rulesets':['dnd2024'],
          'license':'CC-BY-4.0','attribution':ATTR+' '+MODNOTE,
          'description':'Borealis bundled content pack for the 2024 ("5.5") rules: species, backgrounds, feats, 12 classes with SRD subclasses, 339 spells, equipment and magic items from SRD 5.2.1. Compatible with fifth edition.',
          'files':files}
with open(os.path.join(DEST,'pack.json'),'w',encoding='utf-8') as f: json.dump(manifest,f,ensure_ascii=False,indent=1); f.write('\n')
print('wrote',len(files),'files',sum(len(v) for v in OUT.values()),'elements')
