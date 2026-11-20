import json,re,collections,sys
from tmpl import *
from clauses import recombination, unknown_clauses
from catalog import P
from catalog2 import P2
from refine import refine, RDESC
P=P2+P
m=json.load(open('meta.json'))
kz=json.load(open('kwcov.json')); ZERO=set(kz['zero'])
# ability words: keywords that appear as "K — " in any pauper text
AWS=set()
for n,t in pauper.items():
    for k in m[n]['kw']:
        if re.search(r'(^|\n)'+re.escape(k)+r' — ',t): AWS.add(k)
GENERIC=[b for b,_,_ in P if b.startswith(("'Otherwise'","conditional spell clause","'if you do'","you may sacrifice","static 'as long as'","planeswalker-specific","type-filtered","'this turn' restrictions","sacrifice effects","destroy variants","'for each' quantities","hand reveal","discard variants","damage source is another object"))]
ORDER=[x for x in P if x[0] not in GENERIC]+[x for x in P if x[0] in GENERIC]
COMP=[(b,re.compile(r,re.I),d) for b,r,d in ORDER]
DESC={b:d for b,_,d in P}
DESC.update(RDESC)
def classify_sentence(n,s):
    tags=[]
    aw=None
    mm=re.match(r"^([A-Z][\w'!,-]*(?: [\w'!,-]+){0,3}) — (.*)$",s)
    body=s
    if mm and (mm.group(1) in AWS or mm.group(1) in m[n]['kw']):
        aw=mm.group(1); body=mm.group(2)
    body=re.sub(r'^•\s*','',body)
    for k in m[n]['kw']:
        if k in ZERO and k not in AWS and re.search(r'(?<![\w-])'+re.escape(k)+r'(?:s|es|ed|d|ing)?(?![\w-])',s,re.I):
            tags.append('keyword: '+k)
    if tags: return tags
    for b,r,d in COMP:
        if b in GENERIC: continue
        if r.search(body): return [b]
    if recombination(s): return ['LIKELY EXPRESSIBLE: recombination of supported clauses']
    for b,r,d in COMP:
        if b in GENERIC and r.search(body): return [b]
    if aw=='Max speed': return ['keyword: Start your engines!']
    if aw and aw in ZERO: return ['ability word: '+aw]
    kz=[k for k in m[n]['kw'] if k in ZERO and k not in AWS]
    if kz: return ['keyword: '+kz[0]]
    return ['UNCLASSIFIED']
def run():
    cardfeat={}; sentfeat=[]
    for n in unc:
        f=[]
        for s in novel(n):
            t=[refine(x,s) for x in classify_sentence(n,s)]
            sentfeat.append((n,s,t))
            for x in t:
                if x not in f: f.append(x)
        cardfeat[n]=f
    return cardfeat,sentfeat
if __name__=='__main__':
    cf,sf=run()
    cnt=collections.Counter(x for v in cf.values() for x in v)
    sole=collections.Counter(v[0] for v in cf.values() if len(v)==1)
    print('cards',len(cf),'no feature',sum(1 for v in cf.values() if not v))
    print('unclassified sentences',sum(1 for _,_,t in sf if t==['UNCLASSIFIED']),'of',len(sf))
    for b,c in cnt.most_common(70): print(f'{c:4d} {sole[b]:4d}  {b}')
    with open('unclassified.tsv','w') as f:
        for n,s,t in sf:
            if t==['UNCLASSIFIED']: f.write(f'{n}\t{s}\n')
