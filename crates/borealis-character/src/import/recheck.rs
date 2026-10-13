//! Re-resolving an imported character after more content has been installed.
//!
//! The importer keeps what it could not place: unmapped slots in `Character.unresolved` and the
//! unresolved references in `ImportInfo.missing`. [`recheck`] retries them against the current
//! engine content, so nothing needs to be re-imported.

use super::tree::{find_rule, place_slot, SlotNode};
use super::{Ctx, ImportReport, MissingContent, UnmappedSlot};
use crate::model::{Character, UnresolvedSelection};
use borealis_engine::Engine;

/// What a re-check changed and what is still outstanding.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecheckOutcome {
    /// References and slots that resolved this time.
    pub resolved: u32,
    /// Content references that are still not installed.
    pub still_missing: Vec<MissingContent>,
    /// Slots that still cannot be mapped.
    pub still_unmapped: Vec<UnmappedSlot>,
}

impl RecheckOutcome {
    /// True when the character was modified and should be saved.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.resolved > 0
    }
}

fn to_slot(u: &UnresolvedSelection) -> SlotNode {
    SlotNode {
        owner: u.owner.clone(),
        registered: u.chosen.clone(),
        ty: u.ty.clone(),
        name: u.name.clone(),
        ordinal: u.number.max(1),
        checksum: u.checksum.clone(),
        required_level: u.required_level,
        is_list: u.is_list,
        replace_level: u.replace_level,
        instance: u.instance.clone(),
    }
}

/// Retry the unresolved parts of an imported character against the current content.
///
/// Characters that were not imported (or had nothing missing) come back unchanged.
pub fn recheck(engine: &Engine, character: &mut Character) -> RecheckOutcome {
    let uni = engine.universe(character.build.ruleset);
    let mut ctx = Ctx {
        uni,
        state: std::mem::take(&mut character.build),
        report: ImportReport::default(),
        unresolved: Vec::new(),
    };
    let mut resolved = 0u32;
    let mut pending = std::mem::take(&mut character.unresolved);
    // Parents come before their children in file order, but a freshly placed slot can open its
    // children's owner, so repeat until nothing more resolves.
    loop {
        let before = pending.len();
        let mut still = Vec::new();
        for u in pending {
            let slot = to_slot(&u);
            match find_rule(&ctx, &slot) {
                Some(rule) => {
                    place_slot(&mut ctx, slot, rule);
                    resolved += 1;
                }
                None => still.push(u),
            }
        }
        pending = still;
        if pending.len() == before || pending.is_empty() {
            break;
        }
    }
    super::tidy_missing(
        &mut ctx.report.missing_content,
        &std::collections::HashMap::new(),
    );
    let mut missing = character
        .import
        .as_ref()
        .map(|i| i.missing.clone())
        .unwrap_or_default();
    for m in ctx.report.missing_content.drain(..) {
        if !missing.iter().any(|x| x.id == m.id) {
            missing.push(m);
        }
    }
    let mut kept = Vec::new();
    for m in missing {
        if ctx.uni.get(&m.id).is_some() {
            if m.context == "option" && !ctx.state.options.contains(&m.id) {
                ctx.state.options.push(m.id.clone());
            }
            resolved += 1;
        } else {
            kept.push(m);
        }
    }
    let still_unmapped = pending
        .iter()
        .map(|u| UnmappedSlot {
            owner: u.owner.clone(),
            ty: u.ty.clone(),
            name: u.name.clone(),
            chosen: u.chosen.clone(),
            reason: if ctx.uni.get(&u.owner).is_some() {
                "no matching select rule on the owner".into()
            } else {
                "the owner element is not installed".into()
            },
        })
        .collect();
    character.build = ctx.state;
    character.unresolved = pending;
    if let Some(info) = character.import.as_mut() {
        info.missing_content = u32::try_from(kept.len()).unwrap_or(u32::MAX);
        info.missing = kept.clone();
    }
    RecheckOutcome {
        resolved,
        still_missing: kept,
        still_unmapped,
    }
}

const SOURCE_TYPE_TOKENS: [&str; 22] = [
    "RACE",
    "SUBRACE",
    "CLASS",
    "ARCHETYPE",
    "FEAT",
    "SPELL",
    "BACKGROUND",
    "LANGUAGE",
    "PROFICIENCY",
    "RACIAL",
    "ITEM",
    "WEAPON",
    "ARMOR",
    "MAGIC",
    "FEATURE",
    "OPTION",
    "DEITY",
    "COMPANION",
    "VISION",
    "SOURCE",
    "SUPPORT",
    "GRANT",
];

/// Best-effort name of the source book an Aurora element id belongs to, e.g.
/// `ID_WOTC_PHB_RACE_HUMAN` -> `WOTC PHB`. Empty when the id has no recognisable shape.
#[must_use]
pub fn source_hint(id: &str) -> String {
    let Some(rest) = id.strip_prefix("ID_") else {
        return String::new();
    };
    let tokens: Vec<&str> = rest.split('_').collect();
    let end = tokens
        .iter()
        .position(|t| SOURCE_TYPE_TOKENS.contains(t))
        .unwrap_or(tokens.len().min(2));
    tokens[..end].join(" ")
}

/// A readable name derived from an Aurora element id, e.g. `ID_WOTC_TCOE_FEAT_SKILL_EXPERT` ->
/// `Skill Expert`, for content that is not installed and so has no recorded name.
#[must_use]
pub fn name_hint(id: &str) -> String {
    let rest = id.strip_prefix("ID_").unwrap_or(id);
    let tokens: Vec<&str> = rest.split('_').filter(|t| !t.is_empty()).collect();
    let after = tokens
        .iter()
        .rposition(|t| SOURCE_TYPE_TOKENS.contains(t))
        .map_or(tokens.len().min(2), |i| i + 1);
    tokens[after.min(tokens.len())..]
        .iter()
        .map(|t| {
            let lower = t.to_lowercase();
            let mut c = lower.chars();
            c.next().map_or_else(String::new, |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_hint_drops_source_and_type() {
        assert_eq!(name_hint("ID_WOTC_TCOE_FEAT_SKILL_EXPERT"), "Skill Expert");
        assert_eq!(name_hint("ID_PHB_RACE_HUMAN"), "Human");
    }
}
