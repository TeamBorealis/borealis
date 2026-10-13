import pickle, re, os
HERE=os.path.dirname(os.path.abspath(__file__))
PAGES=pickle.load(open(os.path.join(HERE,'pages.pkl'),'rb'))  # list of (n,left,right)
FOOT=re.compile(r'^\s*(\d+\s+System Reference Document 5\.2\.1|System Reference Document 5\.2\.1\s+\d+)\s*$')
def stream(a,b,dedent=True):
    """lines of pages a..b in column reading order, footers removed, with ('\f',page) markers"""
    out=[]
    for n,l,r in PAGES:
        if n<a or n>b: continue
        for col in (l,r):
            lines=[ln.rstrip() for ln in col.split('\n') if not FOOT.match(ln)]
            ind=[len(x)-len(x.lstrip()) for x in lines if x.strip()]
            from collections import Counter as _C
            cc=_C(ind)
            m=min(cc.items(),key=lambda kv:(-kv[1],kv[0]))[0] if cc else 0
            # use 2nd-percentile style: min indent among lines (header lines are flush)
            for ln in lines:
                out.append((n,(ln[m:] if ln[:m].strip()=='' else ln.lstrip()) if ln.strip() else ''))
    return out
def text(a,b):
    return '\n'.join(s for _,s in stream(a,b))

import json
IDS2014=json.load(open(os.path.join(HERE,'ids2014.json')))
WORDS=None
def words():
    global WORDS
    if WORDS is None:
        WORDS=set(w.strip().lower() for w in open('/usr/share/dict/words',errors='ignore'))
    return WORDS
def slug(name):
    s=name.upper().replace('’','').replace("'",'')
    s=re.sub(r'[^A-Z0-9]+','_',s).strip('_')
    return s
def norm_name(n):
    return re.sub(r'[^a-z0-9]','',n.lower().replace('’',''))
def dehyph(a,b):
    """join line a ending with '-' and line b"""
    wa=re.search(r'([A-Za-z]+)-$',a)
    wb=re.match(r'([A-Za-z]+)',b)
    if wa and wb:
        j=(wa.group(1)+wb.group(1)).lower()
        if j in words(): return a[:-1]+b
        # keep hyphen when it is a real compound
        return a+b
    return a+' '+b
def join_lines(lines):
    out=''
    for ln in lines:
        ln=ln.strip()
        if not ln: continue
        if not out: out=ln
        elif out.endswith('-') and not out.endswith(' -'): out=dehyph(out,ln)
        else: out+=' '+ln
    return out
def esc(s):
    return s.replace('&','&amp;').replace('<','&lt;').replace('>','&gt;')
TERM=tuple('.:)?!"”;')
def paragraphs(lines, headrx=r"^[A-Z][A-Za-z’' ,\-/]{1,60}\. "):
    """lines are dedented per column; returns list of paragraph strings (hanging/indent heuristics)."""
    paras=[];cur=[]
    prev=''
    for ln in lines:
        if not ln.strip():
            if cur: paras.append(cur);cur=[]
            prev='';continue
        ind=len(ln)-len(ln.lstrip())
        st=ln.strip()
        newp=False
        if cur:
            pend=cur[-1].rstrip()
            if pend.endswith(TERM):
                if ind>=2 and ind<=4: newp=True
                elif re.match(headrx,st) and ind==0: newp=True
        if newp: paras.append(cur);cur=[]
        cur.append(ln)
    if cur: paras.append(cur)
    return [join_lines(p) for p in paras]

def build_vocab():
    global WORDS
    ws=set(w.strip().lower() for w in open('/usr/share/dict/words',errors='ignore'))
    for n,l,r in PAGES:
        for tok in re.findall(r"[A-Za-z]{3,}",l+' '+r):
            ws.add(tok.lower())
    WORDS=ws
build_vocab()

def chunk_html(lines):
    """blank-line separated chunks; chunks containing wide gaps become <pre>"""
    chunks=[];cur=[]
    for ln in lines:
        if not ln.strip():
            if cur: chunks.append(cur);cur=[]
        else: cur.append(ln)
    if cur: chunks.append(cur)
    # merge: a following chunk that is a paragraph continuation is not handled; keep simple
    html=[]
    pending=[]
    for ch in chunks:
        if any(re.search(r'\S {3,}\S',x) for x in ch):
            html.append('<pre>'+esc('\n'.join(ch))+'</pre>')
        else:
            for p in paragraphs(ch): html.append('<p>'+esc(p)+'</p>')
    return '\n'.join(html)
