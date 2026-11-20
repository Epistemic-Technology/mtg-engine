import re
R = {
"conditional spell clause on game state ('If you control ..., ' / 'If a creature died ...')": [
 ("kicker: extra targets or 'also' effects when kicked ('If this spell was kicked, destroy another target land')", r"if this spell was kicked", "Kicked spells that add targets or extra effects to the same object."),
 ("present-tense type check on a target/revealed card ('If it's a Vampire, it also gains lifelink')", r"^(\w[\w ]* — )?(if it's|if it was|if it has|if that creature (is|had|has)|if that permanent|if target creature has|if that permanent's)", "'If it's a <type>' / 'If that creature is a <type>' checked on resolution (SYNTAX only has past-tense 'if target 1 was <filter>')."),
 ("Addendum: 'if you cast this spell during your main phase'", r"during your main phase", "Cast-timing condition (Addendum)."),
 ("'If you control <X>, <effect>' where X or the effect is outside the supported condition", r"^(\w[\w ]* — )?if you control", "Supported shape `if you control Treefolk, draw 1 card`, but with counts, power filters, multiple types, or effects that don't follow."),
 ("life-total conditions in spells ('If you have 5 or less life' / fateful hour)", r"or less life|or more life", "Life thresholds as a spell condition."),
 ("other game-state conditions in spell clauses (cast/discard/turn history)", r".", "Spell clauses gated by turn history or other state."),
],
"counters: double counters / remove X counters / counter-count conditions": [
 ("ETB counters gated by a condition or scaled by a count (raid, ferocious, converge, 'for each card revealed')", r"enters with", "'This creature enters with a +1/+1 counter on it if you attacked this turn / for each ...'."),
 ("filter: 'creature with a +1/+1 counter on it' (targets and 'each creature you control with a counter' statics)", r"with (a|one or more) (\+1/\+1 )?counters? on (it|them)", "Filter word for permanents with counters; used by Ikoria/Kaldheim +1/+1-counter cards."),
 ("counts of counters on a permanent as a number ('for each scream counter', 'number of +1/+1 counters on it')", r"for each [^.]*counter|number of [^.]*counters", "Counter counts as an amount."),
 ("conditions on having a counter ('as long as it has a +1/+1 counter', 'if it had counters')", r"as long as (it|this \w+) has|if (it|this \w+) (has|had)", "Counter-presence conditions."),
 ("other counter manipulation (remove N from target, counter caps, double)", r".", "Removing counters from other permanents, caps, doubling."),
],
"Aura: triggers on the enchanted permanent (attacks/blocks/deals damage/is tapped for mana/its controller's upkeep)": [
 ("curse-style triggers: 'at the beginning of the upkeep/end step of enchanted permanent's controller'", r"(upkeep|end step) of enchanted", "Step triggers keyed to the enchanted permanent's controller."),
 ("mana-boost auras: 'whenever enchanted land is tapped for mana, its controller adds an additional'", r"tapped for mana", "Triggered mana abilities on land tap (CR 605.1b)."),
 ("Equipment triggers: 'whenever equipped creature attacks/deals combat damage'", r"equipped creature", "Triggers on the equipped creature."),
 ("Aura triggers: 'whenever enchanted creature attacks/blocks/becomes tapped/deals damage'", r".", "Triggers on the enchanted creature's actions."),
],
"'Otherwise' / 'instead' conditional branches in spells": [
 ("'Otherwise, ...' second branch", r"^otherwise", "Two-branch conditionals ('If ..., X. Otherwise, Y.')."),
 ("damage redirection: 'the next 1 damage ... is dealt to target creature instead'", r"is dealt to [^.]*instead", "Redirection shields (CR 615 / 614.9)."),
 ("self-replacement on dying ('If this creature would die, exile it / put it on top of library instead')", r"would die", "Replacement effects on this permanent dying, beyond regenerate/exile-instead for targets."),
 ("'<effect> instead if <condition>' upgrades (raid, metalcraft, ferocious, hellbent, landfall, night)", r".", "A spell's effect replaced by a bigger one under a condition."),
],
"each player / each opponent symmetric effects (each player sacrifices, returns, draws)": [
 ("untap during each other player's untap step", r"each other player's untap step", "Seedborn-style untaps."),
 ("each player/opponent chooses and acts ('each opponent sacrifices a creature of their choice', 'each player returns')", r"sacrifices|returns|may (discard|search)|reveals", "Per-player choices."),
 ("'each other player' / 'each opponent who' wording", r"each other player|each opponent who|for each opponent who", "Multiplayer wordings."),
 ("other per-player effects", r".", "Per-player effects not yet expressible."),
],
}
def refine(b,s):
    if b not in R: return b
    for sb,rx,_ in R[b]:
        if re.search(rx,s,re.I): return sb
    return b
RDESC={sb:d for v in R.values() for sb,_,d in v}
