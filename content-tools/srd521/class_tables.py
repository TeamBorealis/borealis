import sys; sys.path.insert(0,'.')
import subprocess,re
PDF='../../../input/srd/SRD_CC_v5.2.1.pdf'
SLOTS9=['slot_1','slot_2','slot_3','slot_4','slot_5','slot_6','slot_7','slot_8','slot_9']
SPEC={
 'Barbarian':(28,['rages','rage_damage','weapon_mastery']),
 'Bard':(31,['bardic_die','cantrips','prepared']+SLOTS9),
 'Cleric':(36,['channel_divinity','cantrips','prepared']+SLOTS9),
 'Druid':(41,['wild_shape','cantrips','prepared']+SLOTS9),
 'Fighter':(47,['second_wind','weapon_mastery']),
 'Monk':(50,['martial_arts','focus_points','unarmored_movement']),
 'Paladin':(53,['channel_divinity','prepared']+SLOTS9[:5]),
 'Ranger':(58,['favored_enemy','prepared']+SLOTS9[:5]),
 'Rogue':(62,['sneak_attack']),
 'Sorcerer':(65,['sorcery_points','cantrips','prepared']+SLOTS9),
 'Warlock':(71,['invocations','cantrips','prepared','pact_slots','pact_slot_level']),
 'Wizard':(77,['cantrips','prepared']+SLOTS9),
}
def parse_table(cls):
    p,cols=SPEC[cls]
    t=subprocess.run(['pdftotext','-layout','-f',str(p),'-l',str(p),PDF,'-'],capture_output=True,text=True).stdout
    lines=t.split('\n')
    rows={}
    k=len(cols)
    for ln in lines:
        s=ln.strip().replace('+10 ft.','+10ft').replace('+15 ft.','+15ft').replace('+20 ft.','+20ft').replace('+25 ft.','+25ft').replace('+30 ft.','+30ft')
        m=re.match(r'^(\d{1,2})\s+\+(\d)\s+(.*)$',s)
        if not m: continue
        lvl=int(m.group(1))
        if lvl in rows: continue
        # only the left table: ignore if the right column also has text; the table lines start at col<20
        if len(ln)-len(ln.lstrip())>20: continue
        toks=m.group(3).split()
        # numeric columns are the last k tokens
        nums=toks[-k:]; feat=' '.join(toks[:-k])
        rows[lvl]={'pb':int(m.group(2)),'features':feat,'cols':nums}
    return cols,rows
def cell(v):
    if v in('—','-'): return 0
    if re.match(r'^\d+$',v): return int(v)
    return v
TABLES={}
for c in SPEC:
    cols,rows=parse_table(c)
    assert sorted(rows)==list(range(1,21)),(c,sorted(rows))
    TABLES[c]={'columns':cols,'pb':[rows[l]['pb'] for l in range(1,21)],
               'values':{col:[cell(rows[l]['cols'][i]) for l in range(1,21)] for i,col in enumerate(cols)},
               'features':[rows[l]['features'] for l in range(1,21)]}
if __name__=='__main__':
    for c,t in TABLES.items():
        print(c,t['pb'][:5]); 
        for col,v in t['values'].items(): print('  ',col,v)
        print('  feat L1:',t['features'][0],'| L20:',t['features'][19])
