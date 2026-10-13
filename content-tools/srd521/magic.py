import sys;sys.path.insert(0,'.')
from common import *
import pickle
T=stream(209,253)
lines=[s for _,s in T]
# skip the first lines until 'Adamantine Armor'
start=next(i for i,l in enumerate(lines) if l.strip()=='Adamantine Armor')
lines=lines[start:]
CAT=r'(Armor|Potion|Ring|Rod|Scroll|Staff|Wand|Weapon|Wondrous Item)'
RAR=r'(Rarity Varies|Common|Uncommon|Rare|Very Rare|Legendary|Artifact)'
tl=re.compile(r'^'+CAT+r'\b.*')
def isend(l): return (not l.strip()) or l.rstrip().endswith(TERM)
heads=[]
for i,l in enumerate(lines):
    if l.startswith(' ') : continue
    if not tl.match(l): continue
    two=l+' '+(lines[i+1] if i+1<len(lines) else '')
    if not re.search(RAR,two): continue
    if not re.match(r'^'+CAT+r'( \(.*?\))?,',l) and not re.match(r'^'+CAT+r' \([^)]*$',l): 
        continue
    # preceding name
    j=i-1
    if j<0 or not lines[j].strip(): continue
    if lines[j].rstrip().endswith(('.',':','?','!',';','”')) : continue
    name=lines[j].strip(); nj=j
    if j-1>=0 and lines[j-1].strip() and not lines[j-1].rstrip().endswith(TERM) and re.match(r'^[A-Z]',lines[j-1]) and not re.search(r'\S {2,}\S',lines[j-1]) and len(lines[j-1])<45 and (j-2<0 or isend(lines[j-2])):
        name=lines[j-1].strip()+' '+name; nj=j-1
    heads.append((nj,i,name))
print(len(heads))
for h in heads: print(h[2])
pickle.dump((lines,heads),open('magic_heads.pkl','wb'))
