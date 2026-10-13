"""Shared helpers + registry for class definitions."""
import sys; sys.path.insert(0,'.')
from lib import *
PB={'type':'stat','stat':'proficiency'}
def tab(k): return {'type':'table','key':k}
def fx(n): return {'type':'fixed','value':n}
def frm(e): return {'type':'formula','expr':e}
def sp(name): return idof('SPELL',name)
def res(id,name,max,recharge,**kw): return custom('resource',id=id,name=name,max=max,recharge=recharge,**kw)
def always(spell,level=None,mode='always_prepared',list=None,**kw): return custom('grant_spell',level=level,spell=sp(spell),mode=mode,counts_against_prepared=False,**({'spell_list':list} if list else {}),**kw)
def adv(roll,**kw): return custom('roll_advantage',roll=roll,**kw)
def sadv(**kw): return custom('save_advantage',**kw)
def resist(t): return grant('ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_'+t.upper(),'Condition')
def toggle(id,name,activation,duration,effects,**kw): return custom('toggle',id=id,name=name,activation=activation,duration=duration,effects=effects,**kw)
def expertise(n,level=None,**kw): return custom('expertise',level=level,count=n,**kw)
def skill_sel(name,skills,n=1,level=None,requirements=None): return select('Proficiency',name,supports=skill_range(skills),number=n,level=level,requirements=requirements)
def feat_sel(name,category,level,default=None): return custom('select_feat',name=name,level=level,category=category,**({'default':default} if default else {}))
def extra_attack(n,level=None): return stat('attacks:extra',str(n),bonus='extra attack',level=level)
def hp_stat(v,level=None,alt=None): return stat('hp',str(v),level=level,alt=alt)
def opt(id,name,desc,supports,rules=None,**data): return el(id,name,'Option',desc,supports=supports,rules=rules or [],data=data or None)
def ac_alt(key,label,abils,equipped='[armor:none]'):
    r=[stat('ac:%s:base'%key,'10')]
    for a in abils: r.append(stat('ac:%s:%s'%(key,a),'%s:modifier'%a))
    r.append(stat('ac:%s'%key,'ac:%s:base'%key))
    for a in abils: r.append(stat('ac:%s'%key,'ac:%s:%s'%(key,a)))
    r.append(stat('ac:calculation','ac:%s'%key,bonus='calculation',equipped=equipped,alt=label))
    return r
CD={}   # class name -> dict
class Cls(dict): pass
def defclass(name,**kw):
    CD[name]=Cls(kw); return CD[name]
SPELLSLOTS_FULL='ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_FULL'
