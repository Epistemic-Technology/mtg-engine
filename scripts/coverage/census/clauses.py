from tmpl import *
KWS=r'\b(flying|deathtouch|defender|double strike|first strike|flash|haste|hexproof|indestructible|lifelink|menace|prowess|reach|trample|vigilance|shroud|fear|intimidate|shadow|horsemanship|skulk|wither|infect|plainswalk|islandwalk|swampwalk|mountainwalk|forestwalk)\b'
def ctmpl(c):
    c=tmpl(c)
    c=re.sub(KWS,'KW',c,flags=re.I)
    c=re.sub(r'\b(it|that creature|target creature|this|enchanted creature|equipped creature|target creature you control|another target creature|up to N target creature)\b','OBJ',c)
    c=c.strip(' .,;')
    c=re.sub(r'^(then|and) ','',c)
    return c.lower()
def split_clauses(s):
    s=AW.sub('',s); s=re.sub(r'^•\s*','',s)
    out=[]
    m=re.match(r'^([^:"]*\{[^:"]*|[^:"]*(Sacrifice|Discard|Exile|Tap|Remove|Pay|Return)[^:"]*): (.*)$',s)
    if m:
        for c in m.group(1).split(', '): out.append('COST '+c)
        s=m.group(3)
    m=re.match(r'^((When|Whenever|At) [^,]*), (if [^,]*, )?(.*)$',s)
    if m:
        out.append('TRIG '+m.group(1))
        if m.group(3): out.append('COND '+m.group(3))
        s=m.group(4)
    for c in re.split(r', then |, and |; | and |, ',s):
        if c.strip(): out.append(c)
    return [ctmpl(c) for c in out]
CL=collections.Counter()
for n,t in corpus.items():
    for s in sents(n,t):
        for c in split_clauses(s): CL[c]+=1
def recombination(s):
    cs=split_clauses(s)
    return bool(cs) and all(c in CL for c in cs)
def unknown_clauses(s):
    return [c for c in split_clauses(s) if c not in CL]
