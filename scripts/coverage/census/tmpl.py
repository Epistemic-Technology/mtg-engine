from load import *
import collections,json
NUM=r'\b(?:a|an|one|two|three|four|five|six|seven|eight|nine|ten|X|\d+)\b'
COL=r'\b(?:white|blue|black|red|green|nonwhite|nonblue|nonblack|nonred|nongreen)\b'
SUBS=json.load(open(os.environ['CENSUS_ROOT']+'/census/work/subtypes.json'))
SUBRE=re.compile(r'\b(?:non-?)?(?:'+'|'.join(sorted(map(re.escape,SUBS),key=len,reverse=True))+r')s?\b')
corpus=json.load(open(os.environ['CENSUS_ROOT']+'/census/work/corpus.json'))
def sents(name,t):
    t=strip_reminder(norm_name(name,t))
    out=[]
    for para in t.split('\n'):
        para=para.strip()
        if not para: continue
        for s in re.split(r'(?<=[.!])\s+(?=[A-Z{"•])',para):
            s=s.strip()
            if s: out.append(s)
    return out
AW=re.compile(r"^(?:[A-Z][\w'!,-]*(?: [\w'!,-]+){0,3}) — (?=[A-Z{])")
def tmpl(s):
    s=AW.sub('',s)
    s=re.sub(r'\{[WUBRG2]/P\}','{M}',s)
    s=re.sub(r'\{[WUBRG2]/[WUBRG]\}','{M}',s)
    s=re.sub(r'\{(?:[WUBRG]|\d+)\}','{M}',s)
    s=re.sub(r'(\{M\})+','{M}',s)
    s=re.sub(r'[+-][\dX]+/[+-][\dX]+','+N/+N',s)
    s=re.sub(r'\b\d+/\d+\b','N/N',s)
    s=SUBRE.sub('SUB',s)
    s=re.sub(COL,'COLOR',s)
    s=re.sub(NUM,'N',s)
    s=re.sub(r'\bthis (creature|artifact|enchantment|land|Aura|Equipment|Vehicle|permanent|spell|card)\b','this',s)
    s=s.replace('~','this')
    return s
cov=collections.Counter()
for n,t in corpus.items():
    for s in sents(n,t): cov[tmpl(s)]+=1
def novel(n):
    return [s for s in sents(n,unc[n]) if tmpl(s) not in cov]
def norm_name2(name,t):
    for n in name.split(' // '):
        t=t.replace(n,'~')
        first=n.split(',')[0]
        if first!=n and len(first)>2: t=re.sub(r'\b'+re.escape(first)+r'\b','SHORTNAME',t)
    return t
def sents(name,t):
    t=strip_reminder(norm_name2(name,t))
    out=[]
    for para in t.split('\n'):
        para=para.strip()
        if not para: continue
        for s in re.split(r'(?<=[.!])\s+(?=[A-Z{"•])',para):
            s=s.strip()
            if s: out.append(s)
    return out
cov=collections.Counter()
for n,t in corpus.items():
    for s in sents(n,t): cov[tmpl(s)]+=1
NEW=set(json.load(open(os.environ['CENSUS_ROOT']+'/census/work/newly_covered.json')))
for k in NEW: unc.pop(k,None)
def novel(n):
    return [s for s in sents(n,unc[n]) if tmpl(s) not in cov]
