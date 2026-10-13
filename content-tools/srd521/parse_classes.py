import sys; sys.path.insert(0,'.')
from lib import *
from classtext import class_stream
SUBNAME={'Barbarian':'Path of the Berserker','Bard':'College of Lore','Cleric':'Life Domain','Druid':'Circle of the Land','Fighter':'Champion','Monk':'Warrior of the Open Hand','Paladin':'Oath of Devotion','Ranger':'Hunter','Rogue':'Thief','Sorcerer':'Draconic Sorcery','Warlock':'Fiend Patron','Wizard':'Evoker'}
CLASSES=['Barbarian','Bard','Cleric','Druid','Fighter','Monk','Paladin','Ranger','Rogue','Sorcerer','Warlock','Wizard']
S=class_stream(28,82)
lines=[l for _,l in S]
pages=[p for p,_ in S]
# class start indices
starts={}
for i,l in enumerate(lines):
    if l.strip() in CLASSES and i+1<len(lines) and lines[i+1].strip()=='Core %s Traits'%l.strip() and l.strip() not in starts:
        starts[l.strip()]=i
order=sorted(starts.items(),key=lambda kv:kv[1])
bounds={}
for k,(c,i) in enumerate(order):
    bounds[c]=(i,order[k+1][1] if k+1<len(order) else len(lines))
CLASSTXT={c:lines[a:b] for c,(a,b) in bounds.items()}
CLASSPAGES={c:(pages[a],pages[b-1]) for c,(a,b) in bounds.items()}
HEAD=re.compile(r'^Level (\d+): (.+)$')
def parse_class(c):
    ls=CLASSTXT[c]
    # core traits table (until 'Becoming a')
    ci=next(i for i,l in enumerate(ls) if l.startswith('Core '))
    bi=next(i for i,l in enumerate(ls) if l.strip().startswith('Becoming a'))
    core=ls[ci+1:bi]
    traits={};cur=None
    KEYS=['Primary Ability','Hit Point Die','Saving Throw','Skill Proficiencies','Weapon Proficiencies','Armor Training','Tool Proficiencies','Starting Equipment','Proficiencies']
    for l in core:
        if not l.strip(): continue
        m=None
        for k in KEYS:
            if l.startswith(k+' ') or l.strip()==k:
                m=k;break
        if m and not l.startswith(' '):
            if m=='Proficiencies': cur=None; continue
            cur=m;traits[cur]=l[len(m):].strip();continue
        if cur: traits[cur]=join_lines([traits[cur],l])
    # sections
    sections=[]  # (kind, title, level, lines)
    cur=None
    sub=None
    i=bi
    body_start=next(i for i,l in enumerate(ls) if l.strip()==c+' Class Features')
    feats=[];subclass=None;cur=None
    SUBHDR=re.compile(r'^%s Subclass:\s*(.*)$'%c)
    j=body_start+1
    stop_titles=('Spell List','Options')
    mode='class'
    items=[]  # list of dicts {section:'class'|'sub'|'meta', ...}
    cur={'kind':'intro','lines':[]}
    secs=[cur]
    while j<len(ls):
        l=ls[j]
        m=HEAD.match(l)
        ms=SUBHDR.match(l)
        if ms:
            name=SUBNAME[c]
            cur={'kind':'subclass','name':name,'lines':[]};secs.append(cur);mode='sub'
        elif m and l[0]=='L' and not l.startswith(' '):
            cur={'kind':'feature','level':int(m.group(1)),'name':m.group(2).strip(),'lines':[],'mode':mode};secs.append(cur)
        elif re.match(r'^%s Spell List$'%c,l) or l.strip() in('Metamagic Options','Eldritch Invocation Options'):
            cur={'kind':'meta','name':l.strip(),'lines':[]};secs.append(cur)
        else:
            cur['lines'].append(l)
        j+=1
    return traits,secs
PARSED={c:parse_class(c) for c in CLASSES}
if __name__=='__main__':
    for c in CLASSES:
        t,secs=PARSED[c]
        print('==',c,CLASSPAGES[c])
        print({k:v for k,v in t.items()})
        for s in secs:
            if s['kind']=='intro': continue
            print('  ',s['kind'],s.get('mode',''),s.get('level',''),s['name'],len(s['lines']))
