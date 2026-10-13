"""cols.txt (see cols.sh) -> pages.pkl : list of (page, left_column_text, right_column_text)."""
import re,pickle
t=open('cols.txt').read()
pages=re.split(r'=====PAGE (\d+)\n',t)
out=[]
for i in range(1,len(pages),2):
    body=pages[i+1]
    left,_,right=body.partition('=====RIGHT\n')
    out.append((int(pages[i]),left,right))
pickle.dump(out,open('pages.pkl','wb'))
print(len(out),'pages')
