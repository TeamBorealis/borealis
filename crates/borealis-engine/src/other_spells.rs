//! Aurora's "Other Spells" (U7, follow-up of #38).
//!
//! Aurora-format content grants many spells with no spellcasting name: a Warlock invocation's
//! at-will spell, a subclass feature's rituals or disciplines, a racial trait's innate spells
//! (`<grant type="Spell" id="..."/>`). Such a spell belongs to no class's spellcasting, and one
//! block per granting element would be wrong (a Warlock would get one block per invocation).
//! Aurora's sheet lists every spell on the character that no class page listed in one last
//! section, "Other Spells", with no ability, attack, save or prepare count, every spell
//! unprepared and named with where it came from (`Mage Armor (<invocation>)`, a racial trait's
//! spell after the race that granted the trait: [`source_label`]).
//!
//! Borealis builds the same section as one last block, [`OTHER_SPELLS`], marked
//! [`SpellcastingSnapshot::other`]: every live spell no earlier block lists (a class, an item, a
//! feat or species block). Its spells are known (Play lists them), not prepared, and have no
//! slots. The sheet prints it only beside another block (Aurora prints no spellcasting page for a
//! character without a spellcasting class), and the spell cards list its spells.

use crate::graph::Graph;
use crate::snapshot::{SpellEntry, SpellcastingSnapshot};
use crate::stats::StatSet;

/// Name of the block (Aurora's section title).
pub const OTHER_SPELLS: &str = "Other Spells";

/// Where a spell of the block came from, as Aurora appends it to the name: the granting
/// element's name; for a racial trait, the element that granted the trait (the race or
/// subrace); for an Aurora proxy item ("Additional Spell, Fireball"), the item name without the
/// spell's. None when nothing granted the spell.
#[must_use]
pub fn source_label(sp: &SpellEntry) -> Option<String> {
    let g = sp.granted_by.as_ref()?;
    let label = match &g.granted_by {
        Some(outer) if g.ty == "Racial Trait" => outer.clone(),
        _ if g.name.starts_with("Additional Spell,")
            || (g.name.starts_with("Additional ") && g.name.contains("Spell,")) =>
        {
            g.name
                .replace(&sp.name, "")
                .trim_matches(|c| c == ' ' || c == ',')
                .to_string()
        }
        _ => g.name.clone(),
    };
    let label = label.trim();
    (!label.is_empty()).then(|| label.to_string())
}

/// Is spell `id` already listed by a block (known, prepared or in a spellbook)? A spell that is
/// only on a class list it may be prepared from is not.
fn listed(out: &[SpellcastingSnapshot], id: &str) -> bool {
    out.iter().any(|b| {
        b.spells
            .iter()
            .any(|s| s.id == id && (s.known || s.prepared || s.always_prepared))
            || b.spellbook
                .as_ref()
                .is_some_and(|book| book.spells.iter().any(|s| s.id == id))
    })
}

/// Append the "Other Spells" block when a live spell is in no block yet (see the module docs).
/// Run last, after the class, item and granted blocks.
pub fn add_other_spells(graph: &Graph<'_>, stats: &StatSet, out: &mut Vec<SpellcastingSnapshot>) {
    let mut spells: Vec<SpellEntry> = Vec::new();
    for (_, n) in graph.nodes_of_type("Spell") {
        let id = n.e.el.id.as_str();
        if listed(out, id) || spells.iter().any(|s| s.id == id) {
            continue;
        }
        let mut entry = crate::item_spells::entry(graph, n);
        entry.prepared = false;
        entry.always_prepared = false;
        spells.push(entry);
    }
    if spells.is_empty() {
        return;
    }
    spells.sort_by(|a, b| a.level.cmp(&b.level).then_with(|| a.name.cmp(&b.name)));
    let mut block = crate::item_spells::block(OTHER_SPELLS.to_string(), None, stats);
    block.spells = spells;
    block.other = true;
    out.push(block);
}
