//! Debug builds only (the module is `#[cfg(debug_assertions)]` in lib.rs): the S5 cross-platform
//! acceptance digest (specs/acceptance-imports-1.md, S5). It reads the loaded content (per-type
//! counts, per-pack counts and a digest of every element) and builds 12 fixed characters from it
//! with the builder commands: fixed scores, pinned origin and class ids, every other open slot
//! filled with its first option, levelled to [`LEVEL`]. Each character's snapshot (character and
//! instance ids normalised) is hashed. The imports are deterministic (IM1), so the same inputs give
//! the same digest on every platform.
//!
//! Phones have no way to call a command from the e2e engine, so a debug build also answers a
//! request file: when `<data>/acceptance-digest.request` exists at startup, the digest is written
//! to `<data>/acceptance-digest.json` once the content is loaded ([`answer_request`]).

use crate::dto::{AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use borealis_content::Ruleset;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::Instant;

/// Request file a test drops into the data folder before launching the app.
pub const REQUEST_FILE: &str = "acceptance-digest.request";
/// Where the answer to [`REQUEST_FILE`] is written.
pub const RESULT_FILE: &str = "acceptance-digest.json";
/// Format of the result (bump when the digest inputs change).
const VERSION: u32 = 1;
/// Level every character is built to (subclasses, the level-4 improvement and level-3 spells).
const LEVEL: u32 = 5;
const SCORES: [(&str, i32); 6] = [
    ("str", 15),
    ("dex", 14),
    ("con", 14),
    ("int", 12),
    ("wis", 12),
    ("cha", 10),
];
/// Upper bound of first-option fills per level.
const FILL_PASSES: usize = 80;
/// Slot types never filled automatically: a pinned id that is not offered stays visible.
const NOT_FILLED: [&str; 4] = ["Race", "Species", "Class", "Multiclass"];
/// Length of the hex run in generated ids (`c_<uuid simple>`).
const ID_HEX_LEN: usize = 32;

/// One fixed character: its key, edition and the ids picked in order (origin, then class).
struct Spec {
    key: &'static str,
    ruleset: Ruleset,
    picks: &'static [&'static str],
}

/// A-core (Aurora 2014), A-user (Aurora community classes and sub races on A-core races),
/// F-full 2014 (PHB, one with an A-core race) and F-full 2024 (XPHB). A-core has no 2024
/// content, so 2024 is covered by 5etools.
const CHARACTERS: [Spec; 12] = [
    Spec {
        key: "a-core-wizard-high-elf",
        ruleset: Ruleset::Dnd2014,
        picks: &[
            "ID_RACE_ELF",
            "ID_SUB_RACE_HIGH_ELF",
            "ID_WOTC_PHB_CLASS_WIZARD",
        ],
    },
    Spec {
        key: "a-core-fighter-mountain-dwarf",
        ruleset: Ruleset::Dnd2014,
        picks: &[
            "ID_SRD_RACE_DWARF",
            "ID_SUB_RACE_MOUNTAIN_DWARF",
            "ID_WOTC_PHB_CLASS_FIGHTER",
        ],
    },
    Spec {
        key: "a-core-warlock-tiefling",
        ruleset: Ruleset::Dnd2014,
        picks: &["ID_RACE_TIEFLING", "ID_WOTC_PHB_CLASS_WARLOCK"],
    },
    Spec {
        key: "a-user-witch-sylvan-elf",
        ruleset: Ruleset::Dnd2014,
        picks: &[
            "ID_RACE_ELF",
            "ID_SOLASTA_SUB_RACE_SYLVAN_ELF",
            "ID_MHP_CLASS_WITCH",
        ],
    },
    Spec {
        key: "a-user-savant-snow-dwarf",
        ruleset: Ruleset::Dnd2014,
        picks: &[
            "ID_SRD_RACE_DWARF",
            "ID_SOLASTA_SUB_RACE_SNOW_DWARF",
            "ID_LASERLL_CLASS_SAVANT",
        ],
    },
    Spec {
        key: "f-phb-fighter-hill-dwarf",
        ruleset: Ruleset::Dnd2014,
        picks: &[
            "ID_5ET_PHB_RACE_DWARF",
            "ID_5ET_PHB_SUBRACE_DWARF_HILL",
            "ID_5ET_PHB_CLASS_FIGHTER",
        ],
    },
    Spec {
        key: "f-phb-warlock-variant-human",
        ruleset: Ruleset::Dnd2014,
        picks: &[
            "ID_5ET_PHB_RACE_HUMAN",
            "ID_5ET_PHB_SUBRACE_HUMAN_VARIANT",
            "ID_5ET_PHB_CLASS_WARLOCK",
        ],
    },
    Spec {
        key: "f-phb-paladin-a-core-halfling",
        ruleset: Ruleset::Dnd2014,
        picks: &[
            "ID_RACE_HALFLING",
            "ID_SUB_RACE_STOUT_HALFLING",
            "ID_5ET_PHB_CLASS_PALADIN",
        ],
    },
    Spec {
        key: "f-xphb-wizard-human",
        ruleset: Ruleset::Dnd2024,
        picks: &["ID_5ET_XPHB_SPECIES_HUMAN", "ID_5ET_XPHB_CLASS_WIZARD"],
    },
    Spec {
        key: "f-xphb-fighter-dwarf",
        ruleset: Ruleset::Dnd2024,
        picks: &["ID_5ET_XPHB_SPECIES_DWARF", "ID_5ET_XPHB_CLASS_FIGHTER"],
    },
    Spec {
        key: "f-xphb-bard-elf",
        ruleset: Ruleset::Dnd2024,
        picks: &["ID_5ET_XPHB_SPECIES_ELF", "ID_5ET_XPHB_CLASS_BARD"],
    },
    Spec {
        key: "f-xphb-warlock-tiefling",
        ruleset: Ruleset::Dnd2024,
        picks: &["ID_5ET_XPHB_SPECIES_TIEFLING", "ID_5ET_XPHB_CLASS_WARLOCK"],
    },
];

/// One built character.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CharacterDigest {
    key: &'static str,
    ruleset: Ruleset,
    level: u32,
    /// Pinned ids no open slot offered.
    unpicked: Vec<&'static str>,
    /// Slots chosen / still open at the end.
    chosen: usize,
    open: usize,
    digest: String,
}

/// The digest of the loaded content and the 12 characters, as JSON.
///
/// # Errors
/// `internal` when the content or a session lock fails, or a character cannot be created.
pub fn compute(app: &AppState) -> ApiResult<Value> {
    let began = Instant::now();
    let content = content_part(app)?;
    let characters = CHARACTERS
        .iter()
        .map(|spec| build(app, spec))
        .collect::<ApiResult<Vec<_>>>()?;
    let characters = serde_json::to_value(characters).map_err(json_error)?;
    let mut out = json!({
        "version": VERSION,
        "counts": content.counts,
        "total": content.total,
        "packs": content.packs,
        "contentDigest": content.digest,
        "packDigests": content.pack_digests,
        "characters": characters,
    });
    let overall = sha256_hex(out.to_string().as_bytes());
    out["digest"] = Value::String(overall);
    // not part of the digest
    out["elapsedMs"] = json!(u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX));
    Ok(out)
}

/// Debug startup hook: answer a pending [`REQUEST_FILE`] (removed first, so a crash cannot loop).
pub fn answer_request(app: &AppState) {
    let request = app.paths.data.join(REQUEST_FILE);
    if !request.is_file() || std::fs::remove_file(&request).is_err() {
        return;
    }
    let out = compute(app).unwrap_or_else(|e| json!({ "error": e.message }));
    let target = app.paths.data.join(RESULT_FILE);
    let tmp = app.paths.data.join(format!("{RESULT_FILE}.tmp"));
    let written =
        std::fs::write(&tmp, out.to_string()).and_then(|()| std::fs::rename(&tmp, &target));
    if let Err(e) = written {
        tracing::warn!("acceptance digest not written: {e}");
    }
}

struct ContentPart {
    counts: BTreeMap<String, usize>,
    total: usize,
    packs: BTreeMap<String, usize>,
    /// Per pack folder: the digest of its elements (names the pack when `digest` differs).
    pack_digests: BTreeMap<String, String>,
    digest: String,
}

fn content_part(app: &AppState) -> ApiResult<ContentPart> {
    let hub = app.hub()?;
    let mut packs = BTreeMap::new();
    let mut lines = Vec::new();
    for stored in hub.store.iter() {
        let (pack, file) = pack_file(&stored.file);
        *packs.entry(pack.clone()).or_insert(0) += 1;
        let el = serde_json::to_string(&stored.element).map_err(json_error)?;
        let line = format!(
            "{}\u{0}{}\u{0}{file}\u{0}{el}",
            stored.element.id, stored.element.ty
        );
        lines.push((pack, line));
    }
    lines.sort_by(|a, b| a.1.cmp(&b.1));
    let mut hasher = Sha256::new();
    let mut per_pack: BTreeMap<String, Sha256> = BTreeMap::new();
    for (pack, line) in &lines {
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
        let h = per_pack.entry(pack.clone()).or_default();
        h.update(line.as_bytes());
        h.update(b"\n");
    }
    let pack_digests = per_pack
        .into_iter()
        .map(|(p, h)| (p, hex(&h.finalize())))
        .collect();
    let counts = hub
        .store
        .type_counts()
        .into_iter()
        .map(|(t, n)| (t.to_string(), n))
        .collect();
    Ok(ContentPart {
        counts,
        total: lines.len(),
        packs,
        pack_digests,
        digest: hex(&hasher.finalize()),
    })
}

/// The store names a file `<pack id>/<path in the pack>`; `/` on every platform.
fn pack_file(file: &str) -> (String, String) {
    let file = file.replace('\\', "/");
    let pack = file.split('/').next().unwrap_or_default().to_string();
    (pack, file)
}

fn build(app: &AppState, spec: &Spec) -> ApiResult<CharacterDigest> {
    let request = NewCharacterRequest {
        name: format!("Acceptance {}", spec.key),
        ruleset: spec.ruleset,
        starting_level: 1,
        gender: String::new(),
        ability_method: AbilityMethodDto::Manual,
        options: OptionsPatch {
            feats: Some(true),
            multiclassing: Some(false),
            average_hp: Some(true),
            ..OptionsPatch::default()
        },
        portrait: None,
        group: None,
    };
    let snap = app.create_character(spec.ruleset, &request)?;
    let id = snap.id.clone();
    let built = level_and_fill(app, spec, snap);
    let _ = app.close_character(&id, true);
    let _ = app.delete_character(&id);
    let snap = built?;
    let chosen = snap.slots.iter().filter(|s| s.chosen.is_some()).count();
    let open = snap.slots.len() - chosen;
    let unpicked = spec
        .picks
        .iter()
        .copied()
        .filter(|p| {
            !snap
                .slots
                .iter()
                .any(|s| s.chosen.as_ref().is_some_and(|c| c.id == *p))
        })
        .collect();
    Ok(CharacterDigest {
        key: spec.key,
        ruleset: spec.ruleset,
        level: snap.level.total,
        unpicked,
        chosen,
        open,
        digest: snapshot_digest(&snap)?,
    })
}

fn level_and_fill(
    app: &AppState,
    spec: &Spec,
    mut snap: CharacterSnapshot,
) -> ApiResult<CharacterSnapshot> {
    for (key, value) in SCORES {
        snap = app.set_base_score(&snap.id, key, value)?;
    }
    for level in 1..=LEVEL {
        if level > 1 {
            match app.level_up(&snap.id, "main") {
                Ok(s) => snap = s,
                Err(_) => break,
            }
        }
        for pick in spec.picks {
            snap = pick_offered(app, snap, pick);
        }
        snap = fill(app, snap);
    }
    Ok(snap)
}

/// Every offered element id of a slot: each version of every row (B-55 rows, fetched whole).
fn options(app: &AppState, id: &str, slot: &str) -> Vec<String> {
    app.get_slot_options(id, slot, None, None, None)
        .map(|page| {
            page.items
                .iter()
                .flat_map(|o| o.all_versions().iter().map(|v| v.summary.id.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Choose `target` in the first open slot that offers it (no change when none does).
fn pick_offered(app: &AppState, snap: CharacterSnapshot, target: &str) -> CharacterSnapshot {
    let chosen = snap
        .slots
        .iter()
        .any(|s| s.chosen.as_ref().is_some_and(|c| c.id == target));
    if chosen {
        return snap;
    }
    let slot = snap
        .slots
        .iter()
        .filter(|s| s.chosen.is_none())
        .find(|s| options(app, &snap.id, &s.id).iter().any(|o| o == target))
        .map(|s| s.id.clone());
    match slot.map(|s| app.apply_selection(&snap.id, &s, target)) {
        Some(Ok(next)) => next,
        _ => snap,
    }
}

/// Fill open slots with their first option, in snapshot order, as a user accepting defaults would.
fn fill(app: &AppState, mut snap: CharacterSnapshot) -> CharacterSnapshot {
    let mut refused: Vec<String> = Vec::new();
    for _ in 0..FILL_PASSES {
        let open = snap.slots.iter().find(|s| {
            s.chosen.is_none()
                && !NOT_FILLED
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(&s.element_type))
                && !refused.contains(&s.id)
        });
        let Some(slot) = open.map(|s| s.id.clone()) else {
            break;
        };
        let first = options(app, &snap.id, &slot).into_iter().next();
        match first.map(|o| app.apply_selection(&snap.id, &slot, &o)) {
            Some(Ok(next)) => snap = next,
            _ => refused.push(slot),
        }
    }
    snap
}

/// SHA-256 of the snapshot JSON without the session fields, with every generated id (a run of 32
/// hex digits) replaced by its order of first appearance.
fn snapshot_digest(snap: &CharacterSnapshot) -> ApiResult<String> {
    let mut value = serde_json::to_value(snap).map_err(json_error)?;
    if let Some(map) = value.as_object_mut() {
        for key in ["id", "displayPath", "dirty", "name"] {
            map.remove(key);
        }
    }
    let mut seen = Vec::new();
    normalise(&mut value, &mut seen);
    Ok(sha256_hex(value.to_string().as_bytes()))
}

fn normalise(value: &mut Value, seen: &mut Vec<String>) {
    match value {
        Value::String(s) => *s = normalise_ids(s, seen),
        Value::Array(items) => items.iter_mut().for_each(|v| normalise(v, seen)),
        Value::Object(map) => map.values_mut().for_each(|v| normalise(v, seen)),
        _ => {}
    }
}

fn normalise_ids(s: &str, seen: &mut Vec<String>) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        let run = bytes[i..]
            .iter()
            .take_while(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
            .count();
        if run >= ID_HEX_LEN {
            let token = &s[i..i + run];
            let n = seen.iter().position(|t| t == token).unwrap_or_else(|| {
                seen.push(token.to_string());
                seen.len() - 1
            });
            out.push_str(&format!("#{n}"));
            i += run;
        } else {
            let ch = s[i..].chars().next().unwrap_or_default();
            out.push(ch);
            i += ch.len_utf8().max(1);
        }
    }
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn json_error(e: serde_json::Error) -> ApiError {
    ApiError::internal(format!("acceptance digest: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_are_numbered_by_first_appearance() {
        let a = "c_0123456789abcdef0123456789abcdef";
        let b = "fedcba9876543210fedcba9876543210";
        let mut seen = Vec::new();
        assert_eq!(normalise_ids(a, &mut seen), "c_#0");
        assert_eq!(normalise_ids(&format!("{b}:{a}"), &mut seen), "#1:c_#0");
        assert_eq!(
            normalise_ids("ID_WOTC_PHB_CLASS_WIZARD", &mut seen),
            "ID_WOTC_PHB_CLASS_WIZARD"
        );
        assert_eq!(normalise_ids("deadbeef", &mut seen), "deadbeef");
    }

    #[test]
    fn a_stored_file_names_its_pack() {
        assert_eq!(
            pack_file("a-user/x.xml"),
            ("a-user".into(), "a-user/x.xml".into())
        );
        assert_eq!(
            pack_file("5etools-phb\\items.json"),
            ("5etools-phb".into(), "5etools-phb/items.json".into())
        );
    }
}
