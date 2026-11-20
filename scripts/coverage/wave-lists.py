import sys,re,collections
S=sys.argv[1]
A={
 'sagas':['layout saga','layout class','layout case'],
 'mdfc-prepare':['layout modal_dfc','layout prepare'],
 'cast-keywords':['keyword Plot','keyword Sneak','keyword Spree','keyword Gift','keyword Freerunning'],
 'bending-double':['keyword Earthbend','keyword Waterbend','keyword Firebending','keyword Double'],
 'std-keywords-a':['keyword Empower Jace','keyword Max speed','keyword Station','keyword Vivid','keyword Mobilize','keyword Repartee','keyword Mayhem','keyword Behold'],
 'std-keywords-b':['keyword Collect evidence','keyword Craft','keyword Suspect','keyword Manifest dread','keyword Blight','layout room','keyword Converge'],
 'pauper-keywords':['keyword Soulbond','keyword Sunburst','keyword Transmute','keyword Graft','keyword Multikicker','keyword Vanishing','keyword Daybound','keyword Disturb'],
}
cards=collections.defaultdict(dict)
for f in('pauper','standard','modern'):
    for line in open(f'{S}/skips-{f}.txt',encoding='utf-8'):
        m=re.match(r'skip\s+(.*?): ((?:keyword|layout|type) .*|sentence: .*|mismatch.*)$',line.rstrip('\n'))
        if m: cards[m.group(2)].setdefault(m.group(1),set()).add(f)
taken=set()
for a,rs in A.items():
    n=0
    with open(f'{S}/wave1/{a}.txt','w') as o:
        for r in rs:
            d=cards.get(r,{})
            o.write(f'# {r} ({len(d)} cards)\n')
            for name in sorted(d,key=lambda x:(-len(d[x]),x)): o.write(f'{name}\t{",".join(sorted(d[name]))}\n'); n+=1
            taken.add(r)
    print(a,n)
rest=[(len(d),r,d) for r,d in cards.items() if r not in taken]
rest.sort(key=lambda x:(-x[0],x[1]))
with open(f'{S}/wave1/templates-catchup.txt','w') as o:
    for c,r,d in rest:
        if c<3: break
        o.write(f'# {r} ({c} cards)\n')
        for name in sorted(d): o.write(f'{name}\t{",".join(sorted(d[name]))}\n')
print('catchup reasons',sum(1 for c,_,_ in rest if c>=3),'cards',sum(c for c,_,_ in rest if c>=3))
