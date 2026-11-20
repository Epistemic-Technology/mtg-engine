import re,csv,sys
import os
S=os.environ['CENSUS_ROOT']
def rd(p):
    d={}
    for line in open(p,encoding='utf-8'):
        parts=line.rstrip('\n').split('\t')
        if len(parts)>=3: d[parts[0]]=parts[2].replace('\\n','\n')
    return d
pauper=rd(S+'/pauper-oracle.tsv')
unc=rd(S+'/uncovered-oracle.tsv')
covered={k:v for k,v in pauper.items() if k not in unc}
backlog={}
for line in open(S+'/backlog.tsv',encoding='utf-8'):
    p=line.rstrip('\n').split('\t',1)
    if len(p)==2: backlog[p[0]]=p[1]
def strip_reminder(t): return re.sub(r'\s*\([^)]*\)','',t)
def norm_name(name,t):
    for n in name.split(' // '):
        t=t.replace(n,'~')
        first=n.split(',')[0]
        if len(first)>3: t=t.replace(first,'~')
    return t
