import sys; sys.path.insert(0,'.')
from common import *
import subprocess, html
PDF='../../../input/srd/SRD_CC_v5.2.1.pdf'
TABLE_PAGES={28:'Barbarian',31:'Bard',36:'Cleric',41:'Druid',47:'Fighter',50:'Monk',53:'Paladin',58:'Ranger',62:'Rogue',65:'Sorcerer',71:'Warlock',77:'Wizard'}
def words(p):
    out=subprocess.run(['pdftotext','-bbox','-f',str(p),'-l',str(p),PDF,'-'],capture_output=True,text=True).stdout
    return [(float(a),float(b),float(c),float(d),html.unescape(w)) for a,b,c,d,w in re.findall(r'<word xMin="([\d.]+)" yMin="([\d.]+)" xMax="([\d.]+)" yMax="([\d.]+)">(.*?)</word>',out)]
def table_span(p):
    ws=words(p); cname=TABLE_PAGES[p]
    top=None
    for i,w in enumerate(ws):
        if w[4]=='Features' and i>0 and ws[i-1][4]==cname and abs(ws[i-1][1]-w[1])<2 and any(x[4]=='Proficiency' and x[1]>w[1]+5 and x[1]<w[1]+40 for x in ws):
            top=w[1]; break
    twenty=sorted([w for w in ws if w[4]=='20' and w[0]<110 and w[1]>top],key=lambda w:w[1])
    bottom=twenty[0][3]+14
    return top-4,bottom
def crop(p,x,w,y,h):
    t=subprocess.run(['pdftotext','-layout','-f',str(p),'-l',str(p),'-x',str(x),'-y',str(int(y)),'-W',str(w),'-H',str(int(h)),PDF,'-'],capture_output=True,text=True).stdout
    lines=[l.rstrip() for l in t.split('\n') if not FOOT.match(l)]
    return lines
def dedent(lines,base=None):
    from collections import Counter
    ind=[len(x)-len(x.lstrip()) for x in lines if x.strip()]
    if not ind: return lines
    cc=Counter(ind)
    m=min(cc.items(),key=lambda kv:(-kv[1],kv[0]))[0]
    return [(l[m:] if l[:m].strip()=='' else l.lstrip()) if l.strip() else '' for l in lines]
def class_page(p):
    if p not in TABLE_PAGES:
        return [s for n,s in stream(p,p)]
    top,bot=table_span(p)
    out=[]
    for x in (0,297):
        out+=dedent(crop(p,x,297,0,top))
        out+=dedent(crop(p,x,297,bot,783-bot))
    return out
def class_stream(a,b):
    out=[]
    for p in range(a,b+1):
        for l in class_page(p): out.append((p,l))
    return out
if __name__=='__main__':
    for p in (28,):
        print('\n'.join(l for l in class_page(p)))
