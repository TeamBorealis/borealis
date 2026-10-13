import sys; sys.path.insert(0,'.')
import parse_classes as P
c=sys.argv[1]
t,secs=P.PARSED[c]
for s in secs:
    if s['kind']=='intro': continue
    if s['kind']=='meta' and s['name'].endswith('Spell List'): continue
    print('###',s['kind'],s.get('level',''),s['name'])
    print('\n'.join(l for l in s['lines'] if l.strip()))
