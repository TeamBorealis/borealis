import sys;sys.path.insert(0,'.')
from common import *
T=stream(107,175)
lines=[s for _,s in T]
hdrrx=re.compile(r'^(Level (\d) ([A-Z][a-z]+)|([A-Z][a-z]+) Cantrip)\s*(\(.*)?$')
idxs=[]
for i,l in enumerate(lines):
    m=hdrrx.match(l)
    if m and (m.group(5) is not None or True):
        # validate next line has Casting Time
        nxt=' '.join(lines[i+1:i+3])
        if 'Casting Time:' in nxt or ('(' in l and ')' not in l):
            idxs.append(i)
print(len(idxs))
spells=[]
for k,i in enumerate(idxs):
    j=i-1
    while not lines[j].strip(): j-=1
    name=lines[j].strip()
    end=(idxs[k+1]-1) if k+1<len(idxs) else len(lines)
    # trim trailing name of next spell
    e=end
    while e>i and not lines[e-1].strip(): e-=1
    block=lines[i:e-0]
    if k+1<len(idxs):
        # remove next spell's name line (the one just before next header)
        block=lines[i:idxs[k+1]]
        # drop trailing blanks and last nonblank (next name)
        while block and not block[-1].strip(): block.pop()
        block.pop()
        while block and not block[-1].strip(): block.pop()
    spells.append((name,block))
import pickle;pickle.dump(spells,open('spell_blocks.pkl','wb'))
for n,b in spells[:3]+spells[-2:]:
    print('##',n);print('\n'.join(b[:14]))
