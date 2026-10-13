import sys; sys.path.insert(0,'.')
from cd_part3 import *
import gen_origins as GO   # populates OUT (equipment, species, ...)
import parse_classes as P
from class_tables import TABLES
PARSED=P.PARSED
OUT.update(GO.OUT)

def extract_named_paras(lines, names):
    paras=paragraphs2(lines)
    out={};cur=None
    for p in paras:
        hit=None
        for n in names:
            if p.startswith(n+'. ') or p.startswith(n+' (') and re.match(r'^%s \(.*?\)\. '%re.escape(n),p):
                hit=n;break
        if hit:
            cur=hit; out[cur]=[re.sub(r'^%s( \(.*?\))?\. '%re.escape(hit),'',p,count=1)]
        elif cur: out[cur].append(p)
    return {k:'\n'.join('<p>%s</p>'%esc(x) for x in v) for k,v in out.items()}

CLS_ID=lambda c: 'ID_2024_CLASS_'+slug(c)
MC_ID=lambda c: 'ID_2024_MULTICLASS_'+slug(c)
def fid(c,name,level,seen):
    base='ID_2024_CLASS_FEATURE_%s_%s'%(slug(c),slug(name))
    if base in seen: base+='_%d'%level
    seen.add(base); return base
OLD_CLASS={'Barbarian':'ID_WOTC_PHB_CLASS_BARBARIAN','Bard':'ID_WOTC_PHB_CLASS_BARD','Cleric':'ID_WOTC_PHB_CLASS_CLERIC','Druid':'ID_WOTC_PHB_CLASS_DRUID','Fighter':'ID_WOTC_PHB_CLASS_FIGHTER','Monk':'ID_WOTC_PHB_CLASS_MONK','Paladin':'ID_WOTC_PHB_CLASS_PALADIN','Ranger':'ID_WOTC_PHB_CLASS_RANGER','Rogue':'ID_WOTC_PHB_CLASS_ROGUE','Sorcerer':'ID_WOTC_PHB_CLASS_SORCERER','Warlock':'ID_WOTC_PHB_CLASS_WARLOCK','Wizard':'ID_WOTC_PHB_CLASS_WIZARD'}
OLD_SUB={'Barbarian':'ID_WOTC_PHB_ARCHETYPE_PATH_OF_THE_BERSERKER','Bard':'ID_WOTC_PHB_ARCHETYPE_COLLEGE_OF_LORE','Cleric':'ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN','Druid':'ID_WOTC_PHB_ARCHETYPE_CIRCLEOFTHELAND','Fighter':'ID_WOTC_PHB_ARCHETYPE_CHAMPION','Monk':'ID_WOTC_PHB_ARCHETYPE_WAY_OF_THE_OPEN_HAND','Paladin':'ID_WOTC_PHB_ARCHETYPE_PALADIN_OATH_OF_DEVOTION','Ranger':'ID_WOTC_ARCHETYPE_RANGER_HUNTER','Rogue':'ID_WOTC_PHB_ARCHETYPE_THIEF','Sorcerer':'ID_WOTC_PHB_ARCHETYPE_DRACONIC_BLOODLINE','Warlock':'ID_WOTC_PHB_ARCHETYPE_OTHERWORLDLY_PATRON_FIEND','Wizard':'ID_WOTC_PHB_ARCHETYPE_WIZARD_SCHOOL_OF_EVOCATION'}
SUBMAP_KIND={'Barbarian':'equivalent','Bard':'equivalent','Cleric':'equivalent','Druid':'approximate','Fighter':'equivalent','Monk':'approximate','Paladin':'equivalent','Ranger':'approximate','Rogue':'equivalent','Sorcerer':'approximate','Warlock':'approximate','Wizard':'approximate'}

def spell_stats(c,sp_,T):
    """Engine-style slot/prepare increments + cantrip selects, from the parsed table"""
    cl=c.lower(); rules=[]
    V=T['values']
    def deltas(series):
        out=[];prev=0
        for lv,v in enumerate(series,1):
            if v!=prev: out.append((lv,v-prev))
            prev=v
        return out
    if sp_.get('pact'):
        counts=V['pact_slots'];lvls=V['pact_slot_level']
        for k in range(1,6):
            ser=[counts[i] if lvls[i]==k else 0 for i in range(20)]
            for lv,d in deltas(ser): rules.append(stat('%s:spellcasting:slots:%d'%(cl,k),str(d),level=lv))
    else:
        for k in range(1,10):
            key='slot_%d'%k
            if key in V:
                for lv,d in deltas(V[key]): rules.append(stat('%s:spellcasting:slots:%d'%(cl,k),str(d),level=lv))
    for lv,d in deltas(V['prepared']): rules.append(stat('%s:spellcasting:prepare'%cl,str(d),level=lv))
    if 'cantrips' in V:
        for lv,d in deltas(V['cantrips']):
            rules.append(select('Spell','Cantrip (%s %d)'%(c,lv),supports='$(spellcasting:list), 0',number=d,level=lv,spellcasting=c,allow_replace=True))
    if sp_.get('spellbook'):
        # Spellbook selection slots: six level-1 spells at Wizard level 1, two more per level after
        # (SRD 5.2.1 Wizard, Spellcasting: Spellbook / Expanding the Book). Picks fill the book.
        rules.append(select('Spell','Spellbook (%s 1)'%c,supports='$(spellcasting:list), 1',number=6,spellcasting=c))
        for lv in range(2,21):
            rules.append(select('Spell','Spellbook (%s %d)'%(c,lv),supports='$(spellcasting:list), $(spellcasting:slots)',number=2,level=lv,spellcasting=c))
    return rules

def feature_html(lines):
    return chunk_html2(lines)

def build_class(c):
    traits,secs=PARSED[c]; T=TABLES[c]; D=CD[c]
    cid=CLS_ID(c); mid=MC_ID(c)
    seen=set()
    path_c='classes/%s.json'%c.lower()
    sub_name=P.SUBNAME[c]
    sub_slug=slug(sub_name)
    sub_id='ID_2024_SUBCLASS_%s_%s'%(slug(c),sub_slug)
    # ----- class feature elements
    cls_feats=[];sub_feats=[]
    for s in secs:
        if s['kind']!='feature': continue
        (cls_feats if s['mode']=='class' else sub_feats).append(s)
    asi_levels=D.get('asi_levels',[4,8,12,16])
    # feature ids
    feat_rules=D['features']; 
    class_rules=[]
    # base proficiencies etc (gated)
    gate='!'+mid
    for a in D['saves']: class_rules.append(grant(SAVE_ID(a.capitalize()),'Proficiency',requirements=gate))
    n,skl=D['skills']
    if skl==ALL_SKILLS and c=='Bard': class_rules.append(select('Proficiency','Skill Proficiency (%s)'%c,supports='Skill',number=n,requirements=gate))
    else: class_rules.append(select('Proficiency','Skill Proficiency (%s)'%c,supports=skill_range(skl),number=n,requirements=gate))
    for w in D['weapons']:
        class_rules.append(grant(w,'Proficiency',requirements=gate) if isinstance(w,str) else dict(w,requirements=gate))
    for a in D['armor']: class_rules.append(grant(a,'Proficiency',requirements=gate))
    for t in D['tools']:
        tt=dict(t); tt['requirements']=gate; class_rules.append(tt)
    pid,opts=GO.parse_package(cid,c,traits['Starting Equipment'])
    class_rules.append(custom('equipment_package',ref=pid,name='Starting Equipment (%s)'%c,requirements=gate))
    emit('classes/%s.json'%c.lower(),el(pid,'Starting Equipment (%s)'%c,'Equipment Package','',data={'owner':cid,'options':opts}))
    # feature grants
    FEATIDS={}
    for s in cls_feats:
        key=s['name']
        f_id=fid(c,s['name'],s['level'],seen)
        FEATIDS[(s['name'],s['level'])]=f_id
        class_rules.append(grant(f_id,'Class Feature',level=s['level']))
    # Features declared in the table but without own heading (none expected)
    # ASI + boon grants
    asi_id='ID_2024_CLASS_FEATURE_%s_ABILITY_SCORE_IMPROVEMENT'%slug(c)
    boon_id='ID_2024_CLASS_FEATURE_%s_EPIC_BOON'%slug(c)
    # class element
    short=''
    ml=D['multiclass']
    mc_rules=[custom('multiclass_marker')] if False else [grant('ID_INTERNAL_GRANT_MULTICLASS','Grants')]
    mc_rules+=ml['grants']
    for a in D['saves']: pass
    prog={'proficiency_bonus':T['pb'],**{k:v for k,v in T['values'].items()}}
    ce=el(cid,c,'Class',
          '<p>%s</p>'%esc('Primary ability: %s. Hit Point Die: %s. Saving throws: %s.'%(traits['Primary Ability'],traits['Hit Point Die'],traits['Saving Throw'])),
          setters=[setter('hd','d%d'%D['hd']),setter('short','%s; %s'%(traits['Primary Ability'],traits['Armor Training']))],
          rules=class_rules,
          multiclass={'id':mid,'prerequisite':ml['prereq'],'requirements':ml['requirements'],'setters':[setter('multiclass proficiencies',', '.join(x for x in [traits.get('Weapon Proficiencies',''),traits.get('Armor Training',''),traits.get('Tool Proficiencies','')] if x))],'rules':mc_rules},
          data={'hit_die':D['hd'],'primary_abilities':D['primary'],'primary_ability_mode':D.get('primary_mode','all' if len(D['primary'])>1 else 'single'),
                'saving_throws':D['saves'],'subclass_level':3,'epic_boon_level':19,'asi_levels':asi_levels,
                'core_traits':traits,'progression':prog,'features_by_level':T['features'],
                'multiclass':{'prerequisite':ml['prereq'],'requirements':ml['requirements']},
                'spellcasting':({'ability':D['spell']['ability'].lower(),'list':D['spell']['list'],'model':'pact' if D['spell'].get('pact') else ('spellbook' if D['spell'].get('spellbook') else 'prepared'),
                                 'replace':D['spell']['replace'],'multiclass_slots_grant':D['spell']['slots'],'has_cantrips':D['spell'].get('cantrips',True)} if D['spell'] else None),
                'fixed_hp_per_level':{'Barbarian':7,'Fighter':6,'Paladin':6,'Ranger':6,'Sorcerer':4,'Wizard':4}.get(c,5)},
          maps_to={'dnd2014':OLD_CLASS[c],'kind':'equivalent'},
          supports=['Class'])
    emit(path_c,ce)
    # --- feature elements
    sp_=D['spell']
    for s in cls_feats:
        name=s['name']; lvl=s['level']
        f_id=FEATIDS[(name,lvl)]
        body=feature_html(s['lines'])
        key=name if name+'@%d'%lvl not in feat_rules else name+'@%d'%lvl
        rules=list(feat_rules.get(key,[]))
        extra={}
        if name=='Ability Score Improvement':
            rules=[feat_sel('Ability Score Improvement (%s %d)'%(c,l),'General',l,default='ID_2024_FEAT_ABILITY_SCORE_IMPROVEMENT') for l in asi_levels]
        if name=='Epic Boon': rules=[feat_sel('Epic Boon (%s)'%c,'Epic Boon',19)]
        if name=='Eldritch Invocations':
            inv=T['values']['invocations']; prev=0; rr=[]
            for lv,v in enumerate(inv,1):
                if v!=prev: rr.append(select('Option','Eldritch Invocation (Warlock %d)'%lv,supports='Eldritch Invocation',number=v-prev,level=lv)); prev=v
            rules=rr+[custom('replace_on_level',scope='eldritch invocations',count=1)]
        if sp_ and ((name=='Spellcasting') or (name=='Pact Magic')):
            rules=spell_stats(c,sp_,T)+rules
            rules.append(grant(sp_['slots'],'Grants',requirements='ID_INTERNAL_GRANT_MULTICLASS'))
            if sp_.get('spellbook'): rules.append(custom('spellbook',initial_spells=6,per_level=2,copy_cost_gp_per_level=50,copy_hours_per_level=2,prepare_from='spellbook'))
            if sp_.get('pact'): rules.append(custom('pact_magic',slots_recover=['short_rest','long_rest'],slot_level_table='pact_slot_level',slot_count_table='pact_slots'))
            rules.append(custom('prepared_spell_change',when='gain_level' if sp_['replace']=='level' else 'long_rest',count='any' if c in('Cleric','Druid','Wizard') else 'one'))
            extra['spellcasting']={'name':c,'ability':sp_['ability'],'prepare':True,'allow_replace':True,'list':{'supports':sp_['list'],'known':bool(sp_['known'])}}
            if sp_.get('focus'): rules.append(grant(sp_['focus'],'Proficiency'))
        e=el(f_id,name,'Class Feature',body,rules=rules,**extra,
             data={'class':c,'level':lvl})
        emit(path_c,e)
    # ASI/Boon: they come from headings, nothing else needed
    # --- subclass
    sub_secs=[s for s in secs if s['kind']=='subclass']
    sub_intro=feature_html(sub_secs[0]['lines'][1:]) if sub_secs else ''
    SUBF={}
    sub_rules=[]
    sseen=set()
    for s in sub_feats:
        sf_id='ID_2024_SUBCLASS_FEATURE_%s_%s_%s'%(slug(c),sub_slug,slug(s['name']))
        if sf_id in sseen: sf_id+='_%d'%s['level']
        sseen.add(sf_id)
        SUBF[(s['name'],s['level'])]=sf_id
        sub_rules.append(grant(sf_id,'Archetype Feature',level=s['level']))
    se=el(sub_id,sub_name,'Archetype',sub_intro,supports=[c+' Subclass'],rules=sub_rules,
          data={'class':cid,'class_name':c,'level_gained':3},maps_to={'dnd2014':OLD_SUB[c],'kind':SUBMAP_KIND[c]})
    path_s='classes/%s-%s.json'%(c.lower(),sub_slug.lower().replace('_','-'))
    emit(path_s,se)
    SR=D['subclass']['feature_rules']
    for s in sub_feats:
        sf_id=SUBF[(s['name'],s['level'])]
        rules=list(SR.get(s['name'],[]))
        body=feature_html(s['lines'])
        emit(path_s,el(sf_id,s['name'],'Archetype Feature',body,rules=rules,data={'class':c,'subclass':sub_name,'level':s['level']}))
    # --- sub options
    def build_subopts(spec,section_secs,target_path,default_group_types='Option'):
        for (fname,lvl,group,names,rulemap) in spec:
            key=fname.split('@')[0]
            sec=[s for s in section_secs if s['name']==key and s['level']==lvl]
            if not sec: sec=[s for s in section_secs if s['name']==key]
            if not sec: print('!! no feature for subopt',c,fname); continue
            texts=extract_named_paras(sec[0]['lines'],names)
            for nm in names:
                oid=idof('OPTION',group,nm)
                emit(target_path,opt(oid,nm,texts.get(nm,''),[group],rules=rulemap.get(nm,[]),class_name=c,group=group,source_feature=fname.split('@')[0],level=lvl))
                if nm not in texts: print('!! missing text',c,group,nm)
    build_subopts(D.get('subopts',[]),cls_feats,path_c)
    if isinstance(D.get('subopts_sub'),list): build_subopts(D['subopts_sub'],sub_feats,path_s)
    return FEATIDS
for c in CLASSES_ORDER if False else P.CLASSES: build_class(c)
