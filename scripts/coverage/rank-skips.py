import sys,re,collections
S=sys.argv[1]
by=collections.defaultdict(lambda:{f:set() for f in('pauper','standard','modern')})
for f in('pauper','standard','modern'):
    for line in open(f'{S}/skips-{f}.txt',encoding='utf-8'):
        m=re.match(r'skip\s+(.*?): (keyword |layout |type |sentence: |mismatch|.*)(.*)$',line.rstrip('\n'))
        if not m: continue
        name,kind,rest=m.groups()
        r=(kind+rest).strip()
        if r.startswith('sentence: parse:'): r='sentence: parse error'
        by[r][f].add(name)
rows=[]
for r,d in by.items():
    u=d['pauper']|d['standard']|d['modern']
    rows.append((len(u),len(d['pauper']),len(d['standard']),len(d['modern']),r))
rows.sort(reverse=True)
with open(f'{S}/backlog-ranked.tsv','w') as o:
    o.write('union\tpauper\tstandard\tmodern\treason\n')
    for x in rows: o.write('\t'.join(map(str,x))+'\n')
print(len(rows),'reasons; singletons',sum(1 for x in rows if x[0]==1))
for x in rows[:70]: print('%4d %4d %4d %4d  %s'%(x[0],x[1],x[2],x[3],x[4][:90]))
