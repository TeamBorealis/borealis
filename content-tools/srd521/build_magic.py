import sys;sys.path.insert(0,'.')
from common import *
import pickle
lines,heads=pickle.load(open('magic_heads.pkl','rb'))
n14={}
for k,(n,t) in IDS2014.items():
    if t=='Magic Item': n14.setdefault(norm_name(n),k)
RAR=['Very Rare','Uncommon','Common','Rare','Legendary','Artifact']
def typeblock(i):
    t=lines[i].strip(); k=i+1
    while True:
        bal=t.count('(')-t.count(')')
        last=t.split()[-1]
        if bal>0 or last in('Very','or','by','a','an','the','of') or t.endswith(','):
            t+=' '+lines[k].strip(); k+=1
        else: break
    return t,k
out=[]
for idx,(nj,ti,name) in enumerate(heads):
    t,k=typeblock(ti)
    end=heads[idx+1][0] if idx+1<len(heads) else len(lines)
    body=lines[k:end]
    while body and not body[-1].strip(): body.pop()
    m=re.match(r'^(Armor|Potion|Ring|Rod|Scroll|Staff|Wand|Weapon|Wondrous Item)(?: \((.*?)\))?,\s*(.*)$',t)
    assert m,(name,t)
    cat,sub,rest=m.group(1),m.group(2),m.group(3)
    attune=None
    am=re.search(r'\(Requires Attunement( by (.*?))?\)\s*$',rest)
    if am:
        attune=am.group(2) or ''
        rest=rest[:am.start()].strip()
    rarity=rest
    out.append(dict(name=name,cat=cat,sub=sub,rarity=rarity,attune=attune,raw=t,body=body))
pickle.dump(out,open('magic_raw.pkl','wb'))
from collections import Counter
print(Counter(o['rarity'] for o in out).most_common(40))
print(Counter(o['attune'] for o in out).most_common(20))
print(Counter(o['cat'] for o in out))
