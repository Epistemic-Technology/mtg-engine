import os,re,json,collections
from classify import *
OUT=os.environ['CENSUS_ROOT']+'/census'
cf,sf=run()
L='LIKELY EXPRESSIBLE: recombination of supported clauses'
NONE='NO MISSING FEATURE FOUND: every sentence matches supported phrasing (re-attempt)'
UNC='UNCLASSIFIED: one-off phrasing (see unclassified.tsv)'
feats={}
for n,v in cf.items():
    f=[x for x in v if x!=L]
    f=[UNC if x=='UNCLASSIFIED' else x for x in f]
    feats[n]=f if f else [NONE]
# descriptions
desc=dict(DESC)
desc[NONE]="No sentence needs anything new: each sentence (or each of its clauses) already appears in an authored card. Re-run the author on these; backlog reasons for many are stale."
desc[UNC]="Sentences no pattern recognised; mostly single-card wordings. See census/unclassified.tsv."
def reminder(k):
    for n,t in pauper.items():
        mm=re.search(re.escape(k)+r'[^(\n]*\(([^)]*)\)',t,re.I)
        if mm: return mm.group(1)
    return ''
for b in set(x for v in feats.values() for x in v):
    if b.startswith('keyword: '):
        k=b[9:]; r=reminder(k)
        desc[b]=f"Keyword {k}"+(f": {r}" if r else " (no reminder text; see CR 702/701).")
    elif b.startswith('ability word: '):
        k=b[14:]
        ex=[s for n,s,t in sf if t==[b]][:1]
        desc[b]=f"The condition/trigger behind ability word {k}, e.g. \"{ex[0][:160] if ex else ''}\""
cnt=collections.Counter(x for v in feats.values() for x in v)
sole=collections.Counter(v[0] for v in feats.values() if len(v)==1)
def slug(b):
    s=re.sub(r'[^a-z0-9]+','-',b.lower()).strip('-')
    return s[:70].rstrip('-')
os.makedirs(OUT+'/cards',exist_ok=True)
for f in os.listdir(OUT+'/cards'): os.remove(OUT+'/cards/'+f)
rows=[]
for b in cnt:
    so=sorted(n for n,v in feats.items() if v==[b])
    ot=sorted(n for n,v in feats.items() if b in v and len(v)>1)
    ex=(so+ot)[:3]
    rows.append((sole[b],cnt[b],b,ex,desc.get(b,'').replace('\t',' ').replace('\n',' ')))
    with open(f'{OUT}/cards/{slug(b)}.txt','w') as f:
        f.write(f'# {b}\n# sole blocker ({len(so)}):\n'+''.join(x+'\n' for x in so)+f'# also needs other features ({len(ot)}):\n'+''.join(x+'\n' for x in ot))
rows.sort(key=lambda r:(-r[0],-r[1],r[2]))
with open(OUT+'/buckets.tsv','w') as f:
    f.write('bucket\tcards_needing\tsole_blocker\texamples\tneeded\tcard_file\n')
    for so,c,b,ex,d in rows: f.write(f'{b}\t{c}\t{so}\t{"; ".join(ex)}\t{d}\tcards/{slug(b)}.txt\n')
with open(OUT+'/card_features.tsv','w') as f:
    f.write('card\tn_features\tfeatures\n')
    for n in sorted(feats): f.write(f'{n}\t{0 if feats[n]==[NONE] else len(feats[n])}\t{" | ".join(feats[n])}\n')
with open(OUT+'/unclassified.tsv','w') as f:
    for n,s,t in sf:
        if t==['UNCLASSIFIED']: f.write(f'{n}\t{s}\n')
with open(OUT+'/sentences.tsv','w') as f:
    f.write('card\tbucket\tsentence\n')
    for n,s,t in sf: f.write(f'{n}\t{" | ".join(t)}\t{s}\n')
dist=collections.Counter(0 if v==[NONE] else len(v) for v in feats.values())
print('cards',len(feats),'dist',sorted(dist.items()))
print('buckets',len(rows))
for so,c,b,ex,d in rows[:45]: print(f'{so:4d} {c:4d}  {b}')
