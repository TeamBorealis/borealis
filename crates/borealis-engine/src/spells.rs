//! Spellcasting: per-class slots, DC, attack, prepared limits and the pure multiclass
//! slot computation.

use crate::diag::{code, DiagSink, Diagnostic, Severity};
use crate::eval::{supports_match, NoDynamics};
use crate::graph::{Graph, NodeId, Origin};
use crate::ids;
use crate::profile::{CasterRounding, RulesetProfile, SpellPreparation};
use crate::snapshot::{
    ArcanumSnapshot, MulticlassSlots, SpellEntry, SpellbookSnapshot, SpellcastingSnapshot,
};
use crate::spell_rules::{class_rules, progression};
use crate::state::CharacterState;
use crate::stats::StatSet;
use crate::universe::{URule, UStat, Universe};

/// Caster levels contributed by a class level for a slot-source kind (the suffix of
/// `ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_<KIND>`).
#[must_use]
pub fn caster_contribution(kind: &str, class_level: u32, rounding: CasterRounding) -> u32 {
    let l = class_level;
    match kind {
        "FULL" => l,
        "HALF" => match rounding {
            CasterRounding::Down => l / 2,
            CasterRounding::Up => l.div_ceil(2),
        },
        "HALF_UP" => l.div_ceil(2),
        "THIRD" => l / 3,
        "THIRD_UP" => (l + 1) / 3,
        "FOURTH" => l / 4,
        "FOURTH_UP" => (l + 2) / 4,
        "FIFTH" => l / 5,
        _ => 0,
    }
}

/// Slots for a combined caster level from the cumulative `multiclass:spellcasting:slot:N` rules.
#[must_use]
pub fn pooled_slots(caster_level: u32, table: &[&UStat]) -> [i32; 9] {
    let mut out = [0; 9];
    for r in table {
        if r.level > caster_level {
            continue;
        }
        let Some(n) = r
            .name
            .strip_prefix("multiclass:spellcasting:slot:")
            .and_then(|n| n.parse::<usize>().ok())
        else {
            continue;
        };
        if let (1..=9, crate::universe::Val::Lit(v)) = (n, &r.value) {
            out[n - 1] += v;
        }
    }
    out
}

/// Slot-source nodes (everything but `SOLO`) and their kind.
fn slot_sources<'g>(graph: &'g Graph<'_>) -> Vec<(NodeId, &'g str)> {
    graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.alive)
        .filter_map(|(i, n)| {
            let kind = n.e.el.id.strip_prefix(ids::SLOTS_PREFIX)?;
            (kind != "SOLO").then_some((i, kind))
        })
        .collect()
}

/// Pooled multiclass slots, when two or more slot sources exist (pure: nothing is mutated).
#[must_use]
pub fn multiclass_slots(
    graph: &Graph<'_>,
    uni: &Universe,
    hooks: &dyn RulesetProfile,
    diags: &mut DiagSink,
) -> Option<MulticlassSlots> {
    let sources = slot_sources(graph);
    if sources.len() < 2 {
        return None;
    }
    let caster_level: u32 = sources
        .iter()
        .map(|(n, kind)| {
            caster_contribution(
                kind,
                graph.mgr_level(graph.nodes[*n].mgr),
                hooks.half_caster_rounding(),
            )
        })
        .sum();
    let Some(table) = uni.get(ids::MULTICLASS_SPELLCASTING) else {
        diags.push(Diagnostic::new(
            code::MULTICLASS_NO_SLOT_TABLE,
            Severity::Warning,
            "multiclass spell slot table element is missing",
        ));
        return None;
    };
    let stats: Vec<&UStat> = table
        .rules
        .iter()
        .filter_map(|r| match r {
            URule::Stat(s) => Some(&**s),
            _ => None,
        })
        .collect();
    Some(MulticlassSlots {
        caster_level,
        slots: pooled_slots(caster_level, &stats),
    })
}

/// Scaled damage of a cantrip at `level` (cantrips only).
pub(crate) fn cantrip_damage(e: &crate::universe::UElem, level: u32) -> Option<String> {
    if spell_level(e) != 0 {
        return None;
    }
    crate::cantrips::scaled_damage(&e.el.description, level)
}

/// The element that granted a spell node, with its own granter and owning class.
pub(crate) fn spell_grant(
    graph: &Graph<'_>,
    node: &crate::graph::Node<'_>,
) -> Option<crate::snapshot::SpellGrant> {
    let parent = &graph.nodes[node.parent?];
    let class_name = std::iter::successors(parent.parent, |id| graph.nodes[*id].parent)
        .map(|id| &graph.nodes[id].e.el)
        .find(|el| el.ty == "Class")
        .map(|el| el.name.clone());
    Some(crate::snapshot::SpellGrant {
        name: parent.e.el.name.clone(),
        ty: parent.e.el.ty.clone(),
        granted_by: parent.parent.map(|id| graph.nodes[id].e.el.name.clone()),
        class_name,
    })
}

/// The element behind an extension block: a `Grants` carrier stands for its granter.
fn extension_grant(graph: &Graph<'_>, node: NodeId) -> crate::snapshot::SpellGrant {
    let mut n = &graph.nodes[node];
    if n.e.el.ty == "Grants" {
        if let Some(p) = n.parent {
            n = &graph.nodes[p];
        }
    }
    crate::snapshot::SpellGrant {
        name: n.e.el.name.clone(),
        ty: n.e.el.ty.clone(),
        granted_by: n.parent.map(|id| graph.nodes[id].e.el.name.clone()),
        class_name: None,
    }
}

/// Spell-id `<extend prepared>` entries: always prepared once the class reaches their level.
fn add_prepared_extensions(
    graph: &Graph<'_>,
    uni: &Universe,
    extensions: &[(NodeId, &borealis_content::SpellcastingList)],
    top: u32,
    spells: &mut Vec<SpellEntry>,
) {
    for (node, l) in extensions {
        if l.prepared != Some(true) || !l.is_spell_id() {
            continue;
        }
        let id = l.supports.trim();
        let Some(e) = uni.get(id) else { continue };
        let level = spell_level(e);
        if level == 0 || level > top || spells.iter().any(|s| s.id == id) {
            continue;
        }
        spells.push(SpellEntry {
            id: id.to_string(),
            name: e.el.name.clone(),
            level,
            prepared: true,
            always_prepared: true,
            known: true,
            damage: None,
            granted_by: Some(extension_grant(graph, *node)),
        });
    }
}

pub(crate) fn spell_level(e: &crate::universe::UElem) -> u32 {
    e.spell_level().unwrap_or(0)
}

/// Pact slot level of a 2014 `slots:count` caster: the `<class>:spellcasting:slot` stat, or, when
/// the content has none (Aurora Profane Soul), the highest per-level slot group that holds the
/// pact slots (`engine.md` 10.5: exactly one level has `count` slots).
fn pact_slot_level(slot_stat: i32, slots: &[i32; 9]) -> i32 {
    if slot_stat > 0 {
        return slot_stat;
    }
    slots
        .iter()
        .rposition(|s| *s > 0)
        .and_then(|i| i32::try_from(i + 1).ok())
        .unwrap_or(slot_stat)
}

/// Snapshot of every spellcasting class.
#[must_use]
pub fn spellcasting_snapshots(
    graph: &Graph<'_>,
    uni: &Universe,
    stats: &StatSet,
    state: &CharacterState,
    profile: &dyn RulesetProfile,
    diags: &mut DiagSink,
) -> Vec<SpellcastingSnapshot> {
    let mut out: Vec<SpellcastingSnapshot> = Vec::new();
    let all_infos = graph.spell_infos();
    for info in &all_infos {
        if info.sc.extend == Some(true)
            || info.name.is_empty()
            || out.iter().any(|o| o.name == info.name)
        {
            continue;
        }
        let lc = info.name.to_lowercase();
        let abbr: String = info
            .sc
            .ability
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .chars()
            .take(3)
            .collect();
        let prepare = info.sc.prepare == Some(true);
        let rules = class_rules(graph, &info.name);
        let class_lvl =
            u32::try_from(graph.class_levels.get(&lc).copied().unwrap_or(0)).unwrap_or(0);
        let table_model = profile.spell_preparation() == SpellPreparation::ClassTable;
        let mut slots = [0; 9];
        for (i, s) in slots.iter_mut().enumerate() {
            *s = stats.sum(&format!("{lc}:spellcasting:slots:{}", i + 1));
        }
        let table_pact = if table_model && rules.pact {
            progression(uni, &info.name, "pact_slots", class_lvl)
                .zip(progression(uni, &info.name, "pact_slot_level", class_lvl))
                .filter(|p| p.0 > 0)
        } else {
            None
        };
        let pact = table_pact.or_else(|| {
            stats
                .exists(&format!("{lc}:spellcasting:slots:count"))
                .then(|| {
                    (
                        stats.sum(&format!("{lc}:spellcasting:slots:count")),
                        pact_slot_level(stats.sum(&format!("{lc}:spellcasting:slot")), &slots),
                    )
                })
                .filter(|p| p.0 > 0)
        });
        if pact.is_some() {
            // Pact Magic slots are a separate pool (2014 and 2024): they recover on a short or
            // long rest and never appear as ordinary slots, even when the content also writes
            // them to the per-level slot groups (`engine.md` 10.5).
            slots = [0; 9];
        }
        let mut spells: Vec<SpellEntry> = Vec::new();
        // spells picked in this class's selection slots (Spellbook (Wizard n), Evocation Savant)
        // are written in the spellbook; always-prepared picks and cantrips are not
        let mut picked_into_book: Vec<SpellEntry> = Vec::new();
        for (_, n) in graph
            .nodes_of_type("Spell")
            .filter(|(_, n)| n.spell_class.as_deref() == Some(info.name.as_str()))
        {
            let always = n.always_prepared;
            let entry = SpellEntry {
                id: n.e.el.id.clone(),
                name: n.e.el.name.clone(),
                level: spell_level(n.e),
                prepared: always || state.prepared_for(&info.name).contains(&n.e.el.id),
                always_prepared: always,
                known: true,
                damage: cantrip_damage(n.e, graph.total_level()),
                granted_by: spell_grant(graph, n),
            };
            if rules.spellbook.is_some()
                && !always
                && entry.level > 0
                && matches!(n.origin, Origin::Selected(_))
            {
                picked_into_book.push(entry.clone());
            }
            // a known spell picked again by a prepared select (Spell Mastery) is one entry
            if let Some(known) = spells.iter_mut().find(|s| s.id == entry.id) {
                known.prepared |= entry.prepared;
                known.always_prepared |= entry.always_prepared;
                continue;
            }
            spells.push(entry);
        }
        let book_ids: Vec<String> = state
            .spellbook_for(&info.name)
            .iter()
            .map(|e| e.0.clone())
            .collect();
        let book_entry = |id: &String| {
            let e = uni.get(id)?;
            Some(SpellEntry {
                id: id.clone(),
                name: e.el.name.clone(),
                level: spell_level(e),
                prepared: state.prepared_for(&info.name).contains(id),
                always_prepared: false,
                known: true,
                damage: cantrip_damage(e, graph.total_level()),
                granted_by: None,
            })
        };
        let book: Vec<SpellEntry> = if rules.spellbook.is_some() {
            let copied: Vec<SpellEntry> = book_ids
                .iter()
                .filter(|id| !picked_into_book.iter().any(|p| &p.id == *id))
                .filter_map(book_entry)
                .collect();
            picked_into_book.into_iter().chain(copied).collect()
        } else {
            Vec::new()
        };
        for b in &book {
            if !spells.iter().any(|s| s.id == b.id) {
                spells.push(b.clone());
            }
        }
        let from_list = prepare && info.sc.list.as_ref().is_some_and(|l| l.known == Some(true));
        let top = graph.highest_slot_level(&info.name, class_lvl.max(1));
        // extension blocks (patron lists, feat/species lists) that apply to this class (10.4)
        let extensions: Vec<(NodeId, &borealis_content::SpellcastingList)> = all_infos
            .iter()
            .filter(|i| {
                i.sc.extend == Some(true)
                    && (i.sc.all == Some(true) || i.name.eq_ignore_ascii_case(&info.name))
            })
            .flat_map(|i| i.sc.extends.iter().map(move |l| (i.node, l)))
            .collect();
        add_prepared_extensions(graph, uni, &extensions, top, &mut spells);
        let extra_lists: Vec<borealis_content::SpellcastingList> = rules
            .extend_lists
            .iter()
            .map(borealis_content::SpellcastingList::new)
            .collect();
        let candidate_entries =
            |known_ids: &[SpellEntry], lists: &[&borealis_content::SpellcastingList]| {
                let mut out = Vec::new();
                for cand in uni.by_type("Spell").filter(|c| {
                    graph.offers_edition(c) && !graph.restricted.is_restricted(&c.el.source)
                }) {
                    let lvl = spell_level(cand);
                    if lvl == 0 || lvl > top || known_ids.iter().any(|s| s.id == cand.el.id) {
                        continue;
                    }
                    if lists.iter().any(|l| {
                        if l.is_spell_id() {
                            return l.supports.trim() == cand.el.id;
                        }
                        matches!(l.expr(), Ok(Some(e)) if supports_match(&e, cand, &NoDynamics))
                    }) {
                        out.push(SpellEntry {
                            id: cand.el.id.clone(),
                            name: cand.el.name.clone(),
                            level: lvl,
                            prepared: state.prepared_for(&info.name).contains(&cand.el.id),
                            always_prepared: false,
                            known: false,
                            damage: cantrip_damage(cand, graph.total_level()),
                            granted_by: None,
                        });
                    }
                }
                out
            };
        let book_candidates = if rules.spellbook.is_some() {
            let lists: Vec<&borealis_content::SpellcastingList> = info
                .sc
                .list
                .iter()
                .chain(info.sc.extends.iter())
                .chain(extensions.iter().map(|e| e.1))
                .collect();
            candidate_entries(&book, &lists)
        } else {
            Vec::new()
        };
        if from_list {
            let lists: Vec<&borealis_content::SpellcastingList> = info
                .sc
                .list
                .iter()
                .chain(info.sc.extends.iter())
                .chain(extensions.iter().map(|e| e.1))
                .chain(extra_lists.iter())
                .collect();
            let found = candidate_entries(&spells, &lists);
            spells.extend(found);
        }
        spells.sort_by(|a, b| a.level.cmp(&b.level).then_with(|| a.name.cmp(&b.name)));
        let prepared_count = u32::try_from(
            spells
                .iter()
                .filter(|s| s.prepared && !s.always_prepared && s.level > 0)
                .count(),
        )
        .unwrap_or(0);
        let limit_name = format!("{lc}:spellcasting:prepare");
        let table_limit = (prepare && table_model)
            .then(|| progression(uni, &info.name, "prepared", class_lvl))
            .flatten();
        let prepare_limit = table_limit.or_else(|| {
            (prepare && stats.exists(&limit_name)).then(|| {
                let raw = stats.sum(&limit_name);
                let (limit, raised) = profile.prepared_limit(raw);
                if raised {
                    diags.push(
                        Diagnostic::new(
                            code::LEGACY_PREPARE_MIN_ONE,
                            Severity::Info,
                            format!("{} prepares at least one spell; raw value {raw}", info.name),
                        )
                        .at(info.name.clone()),
                    );
                }
                limit
            })
        });
        let spellbook = rules.spellbook.map(|r| SpellbookSnapshot {
            capacity: r.initial + r.per_level * class_lvl.saturating_sub(1),
            spells: book.clone(),
            candidates: book_candidates.clone(),
            copy_cost_gp_per_level: r.cost_gp,
            copy_hours_per_level: r.hours,
        });
        let arcanum = rules
            .arcanum
            .iter()
            .map(|&(spell_level, at)| ArcanumSnapshot {
                spell_level,
                class_level: at,
                unlocked: class_lvl >= at,
                spell: spells
                    .iter()
                    .find(|s| s.level == spell_level && s.always_prepared && s.known)
                    .map(|s| s.id.clone()),
                recharge: rules.arcanum_recharge.clone(),
            })
            .collect();
        let prepare_source = if spellbook.is_some() {
            "spellbook"
        } else if from_list {
            "class_list"
        } else {
            ""
        };
        out.push(SpellcastingSnapshot {
            name: info.name.clone(),
            ability: info.sc.ability.clone().unwrap_or_default(),
            prepare,
            allow_replace: info.sc.allow_replace == Some(true),
            dc: stats.sum(&format!("{lc}:spellcasting:dc")),
            attack: stats.sum(&format!("{lc}:spellcasting:attack")),
            slots,
            pact,
            prepare_limit,
            prepared_count,
            spells,
            prepare_from_list: from_list,
            prepare_source: prepare_source.into(),
            spellbook,
            arcanum,
            prepared_change: rules.prepared_change.clone(),
            ritual_source: rules.ritual_source.clone(),
            extended_lists: rules.extend_lists.clone(),
        });
        let _ = abbr;
    }
    crate::item_spells::add_item_blocks(graph, stats, &mut out);
    crate::granted_casting::add_granted_blocks(graph, stats, &mut out);
    out
}

/// Per-class merge sources `(target, source)` for DC and attack groups.
#[must_use]
pub fn spell_merges(graph: &Graph<'_>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for info in graph.spell_infos() {
        if info.sc.extend == Some(true) || info.name.is_empty() {
            continue;
        }
        let abbr: String = info
            .sc
            .ability
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .chars()
            .take(3)
            .collect();
        if abbr.is_empty() {
            continue;
        }
        let lc = info.name.to_lowercase();
        for kind in ["attack", "dc"] {
            let pair = (
                format!("{lc}:spellcasting:{kind}"),
                format!("spellcasting:{kind}:{abbr}"),
            );
            if !out.contains(&pair) {
                out.push(pair);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pact_slot_level_prefers_the_level_stat_then_the_slot_group() {
        let mut groups = [0; 9];
        assert_eq!(pact_slot_level(3, &groups), 3);
        assert_eq!(pact_slot_level(0, &groups), 0, "nothing to read");
        groups[3] = 2;
        assert_eq!(pact_slot_level(0, &groups), 4, "Profane Soul at 19");
        assert_eq!(pact_slot_level(5, &groups), 5, "the stat wins");
    }

    #[test]
    fn contributions_follow_the_internal_tables() {
        let d = CasterRounding::Down;
        let kinds = [
            "FULL",
            "HALF",
            "HALF_UP",
            "THIRD",
            "THIRD_UP",
            "FOURTH",
            "FOURTH_UP",
            "FIFTH",
        ];
        let expect: [[u32; 8]; 5] = [
            // level 1, 2, 3, 4, 5 -> contributions per kind in `kinds` order
            [1, 0, 1, 0, 0, 0, 0, 0],
            [2, 1, 1, 0, 1, 0, 1, 0],
            [3, 1, 2, 1, 1, 0, 1, 0],
            [4, 2, 2, 1, 1, 1, 1, 0],
            [5, 2, 3, 1, 2, 1, 1, 1],
        ];
        for (lvl, row) in expect.iter().enumerate() {
            for (k, e) in kinds.iter().zip(row) {
                assert_eq!(
                    caster_contribution(k, lvl as u32 + 1, d),
                    *e,
                    "{k} at {}",
                    lvl + 1
                );
            }
        }
        assert_eq!(
            caster_contribution("HALF", 5, CasterRounding::Up),
            3,
            "2024 rounding hook"
        );
        assert_eq!(caster_contribution("THIRD_UP", 20, d), 7);
        assert_eq!(caster_contribution("FOURTH_UP", 18, d), 5);
        assert_eq!(caster_contribution("SOLO", 18, d), 0);
    }
}
