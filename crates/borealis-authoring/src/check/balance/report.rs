//! Compare one element with the baselines.

use borealis_content::Element;

use super::baselines::{Baselines, FEAT_ASI_FLOOR, FEAT_PROF_FLOOR};
use super::measure;
use super::{flag, BalanceFlag, BalanceReport, Summary};

fn summary_of(values: impl Iterator<Item = f64>) -> Summary {
    Summary::of(&values.collect::<Vec<_>>())
}

/// Measure `el` and grade every applicable metric against `baselines`.
pub fn balance_report(baselines: &Baselines, el: &Element) -> BalanceReport {
    let mut flags = Vec::new();
    let mut notes = Vec::new();
    match el.ty.as_str() {
        "Spell" => spell_flags(baselines, el, &mut flags, &mut notes),
        "Feat" => feat_flags(baselines, el, &mut flags),
        "Magic Item" => item_flags(baselines, el, &mut flags, &mut notes),
        "Race" | "Species" => species_flags(baselines, el, &mut flags),
        "Monster" => monster_flags(baselines, el, &mut flags, &mut notes),
        other => notes.push(format!("no balance model for element type {other:?}")),
    }
    BalanceReport {
        element_id: el.id.clone(),
        name: el.name.clone(),
        element_type: el.ty.clone(),
        flags,
        notes,
    }
}

fn spell_flags(b: &Baselines, el: &Element, flags: &mut Vec<BalanceFlag>, notes: &mut Vec<String>) {
    let m = measure::spell(el);
    if m.damage <= 0.0 {
        notes.push("no damage dice detected in the spell text; nothing to compare".into());
        return;
    }
    let (s, scope) = b.spell_damage(m.level, m.area);
    let kind = if m.area { "area" } else { "single-target" };
    flags.push(flag(
        "spell.damage_avg",
        m.damage,
        &s,
        false,
        &format!("level {} {kind} damage spells ({scope})", m.level),
    ));
}

fn feat_flags(b: &Baselines, el: &Element, flags: &mut Vec<BalanceFlag>) {
    let m = measure::feat(el);
    let asi = summary_of(b.feats.iter().map(|f| f.asi)).with_floor(FEAT_ASI_FLOOR);
    let prof = summary_of(b.feats.iter().map(|f| f.proficiencies)).with_floor(FEAT_PROF_FLOOR);
    flags.push(flag(
        "feat.asi_total",
        m.asi,
        &asi,
        false,
        "ability points granted by feats",
    ));
    flags.push(flag(
        "feat.proficiencies",
        m.proficiencies,
        &prof,
        false,
        "proficiencies granted by feats",
    ));
}

fn item_flags(b: &Baselines, el: &Element, flags: &mut Vec<BalanceFlag>, notes: &mut Vec<String>) {
    let bonus = measure::item_bonus(el);
    let Some(rarity) = measure::rarity(el) else {
        notes.push("magic item has no rarity setter; bonus cannot be compared".into());
        return;
    };
    if bonus <= 0.0 {
        notes.push("no numeric bonus found (name `+N` or a positive stat rule)".into());
        return;
    }
    let s = b.item_bonus(&rarity);
    flags.push(flag(
        "item.bonus",
        bonus,
        &s,
        false,
        &format!("bonuses of {rarity} magic items"),
    ));
}

fn species_flags(b: &Baselines, el: &Element, flags: &mut Vec<BalanceFlag>) {
    let m = measure::species(el);
    let ctx = "SRD species";
    let traits = summary_of(b.species.iter().map(|s| s.traits));
    let speed = summary_of(b.species.iter().map(|s| s.speed));
    let dark = summary_of(b.species.iter().map(|s| s.darkvision));
    let asi = summary_of(b.species.iter().map(|s| s.asi));
    flags.push(flag("species.trait_count", m.traits, &traits, false, ctx));
    flags.push(flag("species.speed", m.speed, &speed, false, ctx));
    flags.push(flag("species.darkvision", m.darkvision, &dark, false, ctx));
    flags.push(flag("species.asi_total", m.asi, &asi, false, ctx));
}

type MonsterGetter = fn(&measure::MonsterM) -> f64;

fn monster_flags(
    b: &Baselines,
    el: &Element,
    flags: &mut Vec<BalanceFlag>,
    notes: &mut Vec<String>,
) {
    let Some(m) = measure::monster(el) else {
        notes.push("monster data does not parse as a stat block with a valid CR".into());
        return;
    };
    let (pool, width) = b.monsters_near(m.cr_rank);
    if pool.is_empty() {
        notes.push("no monster pack enabled; monster metrics not graded".into());
        return;
    }
    let ctx = if width == 0 {
        format!("CR {} monsters", m.cr_label)
    } else {
        format!("CR {} monsters (widened by {width} CR step(s))", m.cr_label)
    };
    let metrics: [(&str, f64, MonsterGetter); 4] = [
        ("monster.hp", m.hp, |x| x.hp),
        ("monster.ac", m.ac, |x| x.ac),
        ("monster.attack_bonus", m.attack, |x| x.attack),
        ("monster.dpr", m.dpr, |x| x.dpr),
    ];
    for (name, value, get) in metrics {
        let s = summary_of(pool.iter().map(|x| get(x)));
        flags.push(flag(name, value, &s, true, &ctx));
    }
}
