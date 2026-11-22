#!/usr/bin/env python3
"""Card lists for one coverage wave.

    wave-lists.py <wave-dir> <reports-dir>

<wave-dir> holds skips-{pauper,standard,modern}.txt (`mtg-cards author
--format f --verbose`). <reports-dir> holds the card agents' batch-*.txt
and report-*.tsv files from every earlier wave. Writes, in <wave-dir>:

  unsupported-all.tsv   the latest report for each still-uncovered card
  lists/<agent>.tsv     one list per feature agent (table AGENTS below)
  lists/unassigned.tsv  reported cards no agent's pattern matched
  batches/<name>.txt    card batches: never-tried cards of BATCH_FORMAT,
                        minus cards a feature agent owns and cards whose
                        Oracle text names a feature known to be missing
                        (SCREEN); the screened-out cards go to
                        batches/screened-out.tsv with the match

Edit AGENTS, SCREEN and the batch settings per wave. Run from the
repository root (it reads data/scryfall/oracle_cards.jsonl.gz).
"""
import collections, glob, gzip, json, os, random, re, sys

W, R = sys.argv[1], sys.argv[2]
FORMATS = ('pauper', 'standard', 'modern')

# agent -> (template skip reasons it owns (prefix match), pattern on the
# report detail). First match wins, in this order.
AGENTS = [
    ('template', [r'sentence: parse', r'mismatch'], None),
    ('transform', [r'sentence: .*transformed', r'sentence: .*transforms?\b'],
     r"transform|becomes? a .*(creature|copy)|become a copy|still a land|Zendikon|animate|that's both|and-colors|both (white|blue|black|red|green)"),
    ('mana', [r'sentence: .*[Ss]pend this mana', r'sentence: Add '],
     r'spend this mana|mana .*only|restricted mana|mana spent|amount of mana|mana abilit|any combination|add X mana|mana of any|color of mana|treasure'),
    ('counters-costs', [],
     r'counter|cost|kicker|\bpay|tax|sacrific|discard'),
    ('zones', [r'sentence: .*(top .* cards? of|[Ll]ook at|[Rr]eveal|from (a|your) graveyard|[Ss]earch)'],
     r'graveyard|exile|flashback|cast .* from|library|reveal|look at|search|scry|top card|mill|cop(y|ies)|hand'),
    ('rules', [], r'.'),
]
# Oracle text that names a feature nobody has built or is building;
# such a card is not worth a card agent's attempt.
SCREEN = (r'\bmutate\b|\bcompanion\b|\bhideaway\b|\bcloak|\bpiles?\b|choose a number|odd or even'
          r'|\bvote|face[- ]down|\bmeld|\bsticker|\battraction|roll (a|two|x) d\d|flip a coin'
          r'|transformed|transforms into|[Ss]pend this mana only|mana spent to cast|becomes a copy'
          r'|\bcommander\b|\bante\b|\bdraft\b|\bventure\b|\bdungeon\b|initiative|\bmonarch\b')
BATCH_FORMAT, BATCH_SIZE, BATCHES, SEED = 'standard', 35, 4, 8

fmt = collections.defaultdict(set)
reason = {}
for f in FORMATS:
    for line in open(f'{W}/skips-{f}.txt', encoding='utf-8'):
        m = re.match(r'skip\s+(.*?): ((?:keyword|layout|type) .*|sentence: .*|mismatch.*)$', line.rstrip('\n'))
        if m:
            fmt[f].add(m.group(1))
            reason[m.group(1)] = m.group(2)

def fm(n):
    return ','.join(f for f in FORMATS if n in fmt[f])

def rank(n):
    return (0 if n in fmt['pauper'] else 1 if n in fmt['standard'] else 2, n)

rep, tried = {}, set()
for p in sorted(glob.glob(f'{R}/report-*.tsv'), key=os.path.getmtime):
    b = re.search(r'report-(.*)\.tsv', p).group(1)
    for l in open(p, encoding='utf-8'):
        r = l.rstrip('\n').split('\t')
        if len(r) >= 2 and r[1] in ('unsupported', 'failed'):
            rep[r[0]] = (b, r[1], r[2] if len(r) > 2 else '')
for p in glob.glob(f'{R}/batch-*.txt'):
    tried |= {l.strip() for l in open(p, encoding='utf-8') if l.strip()}
with open(f'{W}/unsupported-all.tsv', 'w') as o:
    for n, (b, v, d) in sorted(rep.items()):
        if n in reason:
            o.write(f'{n}\t{b}\t{v}\t{d}\t{fm(n)}\n')

os.makedirs(f'{W}/lists', exist_ok=True)
os.makedirs(f'{W}/batches', exist_ok=True)
out = collections.defaultdict(list)
owned = set()
for n in sorted(reason):
    d = rep.get(n, ('', '', ''))[2]
    for a, prefixes, rx in AGENTS:
        if any(re.match(p, reason[n]) for p in prefixes):
            out[a].append((n, fm(n), 'template: ' + reason[n] + (' | report: ' + d if d else '')))
            owned.add(n)
            break
        if d and rx and re.search(rx, d, re.I):
            out[a].append((n, fm(n), d))
            owned.add(n)
            break
for a, _, _ in AGENTS:
    v = sorted(out[a], key=lambda x: rank(x[0]))
    open(f'{W}/lists/{a}.tsv', 'w').write(''.join('\t'.join(x) + '\n' for x in v))
    print(f'{a:16} {len(v):5}  pauper {sum(n in fmt["pauper"] for n, _, _ in v):4}  standard {sum(n in fmt["standard"] for n, _, _ in v):4}')

text = {}
for l in gzip.open('data/scryfall/oracle_cards.jsonl.gz', 'rt', encoding='utf-8'):
    c = json.loads(l)
    t = c.get('oracle_text') or ' // '.join(f.get('oracle_text', '') for f in c.get('card_faces', []))
    text[c['name']] = t
fresh = sorted(n for n in fmt[BATCH_FORMAT] if n not in tried and n not in owned
               and not re.match('keyword|layout|type', reason[n]))
keep, drop = [], []
for n in fresh:
    m = re.search(SCREEN, text.get(n, ''), re.I)
    (drop if m else keep).append((n, m.group(0) if m else ''))
open(f'{W}/batches/screened-out.tsv', 'w').write(''.join(f'{n}\t{m}\n' for n, m in drop))
names = [n for n, _ in keep]
random.seed(SEED)
random.shuffle(names)
for i in range(BATCHES):
    open(f'{W}/batches/h{i:02d}.txt', 'w').write('\n'.join(names[i * BATCH_SIZE:(i + 1) * BATCH_SIZE]) + '\n')
print(f'fresh {BATCH_FORMAT}: {len(fresh)}; screened out {len(drop)}; kept {len(keep)}; in batches {min(len(names), BATCHES * BATCH_SIZE)}')
