//! Builds one golden character in an [`AppState`] through the builder commands only
//! (`create_character`, `set_ability_method`, `set_base_score`, `apply_selection`, `level_up`,
//! `add_multiclass`, `add_item`, `equip_item`, `set_spell_prepared`, `choose_weapon_masteries`),
//! following the golden's step list in `goldens/mapping.json`.
//!
//! A step names its option by the spec's English name and by the element id that name resolved
//! to in the bundled pack (`id`). The id is never guessed: in resolve mode
//! (`GOLDEN_RESOLVE=<out file>`) the name is looked up among the options the slot offers, from the
//! golden's own SRD pack (or `Core`), and the id found is written into the mapping copy. In the
//! normal run a step's id must be offered by a matching open slot under the same normalised name.

use super::norm;
use borealis_lib::dto::Ruleset;
use borealis_lib::dto::{AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch};
use borealis_lib::dto_ext::EquipRequest;
use borealis_lib::state::AppState;
use serde_json::{json, Map, Value};

const PAGE: usize = 500;
const ABILITIES: [(&str, &str); 6] = [
    ("Strength", "str"),
    ("Dexterity", "dex"),
    ("Constitution", "con"),
    ("Intelligence", "int"),
    ("Wisdom", "wis"),
    ("Charisma", "cha"),
];

/// What happened while building: problems are findings of class (c) until explained.
#[derive(Default, Debug)]
pub struct BuildLog {
    /// Steps that could not be applied: `L<level> <step>: <reason>`.
    pub failed: Vec<String>,
    /// Required slots still open after a level's steps: `L<level> <title> [<type>] (<n> options)`.
    pub open: Vec<String>,
    /// Option names of open slots, for the dump (`slot title -> [(id, name, source)]`).
    pub open_options: Vec<Value>,
    /// Spec choices Borealis has no select for (builder gaps), from `note` steps with `gap`.
    pub gaps: Vec<String>,
    /// How steps were applied when it is not plain (replacement rule).
    pub notes: Vec<String>,
    /// Mapping steps with resolved ids (resolve mode).
    pub resolved: Vec<Value>,
}

pub struct Built {
    pub snap: CharacterSnapshot,
    pub log: BuildLog,
}

struct Ctx<'a> {
    app: &'a AppState,
    id: String,
    snap: CharacterSnapshot,
    sources: Vec<String>,
    resolve: bool,
    log: BuildLog,
    retry: Vec<(String, Value, usize)>,
}

/// Build `spec` with `mapping` (this golden's entry of mapping.json).
pub fn build(app: &AppState, spec: &Value, mapping: &Value, resolve: bool) -> Built {
    let edition = spec["edition"].as_str().unwrap_or_default();
    let rs = if edition == "2024" {
        Ruleset::Dnd2024
    } else {
        Ruleset::Dnd2014
    };
    let source = spec["ruleset"].as_str().unwrap_or_default().to_string();
    let request = NewCharacterRequest {
        name: spec["id"].as_str().unwrap_or("golden").to_string(),
        ruleset: rs,
        starting_level: 1,
        gender: String::new(),
        ability_method: AbilityMethodDto::Manual,
        options: OptionsPatch {
            feats: Some(true),
            multiclassing: Some(true),
            average_hp: Some(true),
            ..OptionsPatch::default()
        },
        portrait: None,
        group: None,
    };
    let snap = app
        .create_character(rs, &request)
        .expect("create character");
    let mut cx = Ctx {
        app,
        id: snap.id.clone(),
        snap,
        sources: vec![source, "Core".to_string()],
        resolve,
        log: BuildLog::default(),
        retry: Vec::new(),
    };
    cx.scores(spec);
    let levels = mapping["levels"].as_array().cloned().unwrap_or_default();
    let progression = spec["level_progression"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut resolved_levels = Vec::new();
    for (i, entry) in progression.iter().enumerate() {
        let level = i + 1;
        if level > 1 {
            cx.level_up(entry, level);
        }
        let steps = levels
            .get(i)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let done: Vec<Value> = steps.iter().map(|s| cx.step(s, level)).collect();
        resolved_levels.push(Value::Array(done));
        cx.record_open(level, false);
    }
    let post = mapping["post"].as_array().cloned().unwrap_or_default();
    let last = progression.len();
    let post_done: Vec<Value> = post.iter().map(|s| cx.step(s, last)).collect();
    cx.retry_failed(last);
    cx.record_open(last + 1, true);
    cx.log.resolved = vec![json!({"levels": resolved_levels, "post": post_done})];
    Built {
        snap: cx.snap,
        log: cx.log,
    }
}

impl Ctx<'_> {
    fn set(
        &mut self,
        r: Result<CharacterSnapshot, impl std::fmt::Debug>,
        what: &str,
        level: usize,
    ) -> bool {
        match r {
            Ok(s) => {
                self.snap = s;
                true
            }
            Err(e) => {
                self.log.failed.push(format!("L{level} {what}: {e:?}"));
                false
            }
        }
    }

    fn scores(&mut self, spec: &Value) {
        let method = spec["ability_scores"]["method"]
            .as_str()
            .unwrap_or_default();
        let m = if method.contains("point buy") {
            AbilityMethodDto::PointBuy
        } else {
            AbilityMethodDto::StandardArray
        };
        let r = self.app.set_ability_method(&self.id, m);
        self.set(r, "ability method", 1);
        for (name, key) in ABILITIES {
            let Some(v) = spec["ability_scores"]["assignment"][name].as_i64() else {
                self.log
                    .failed
                    .push(format!("L1 score {name}: missing in spec"));
                continue;
            };
            let v = i32::try_from(v).unwrap_or_default();
            let r = if m == AbilityMethodDto::StandardArray {
                self.app.assign_standard_array(&self.id, key, Some(v))
            } else {
                self.app.set_base_score(&self.id, key, v)
            };
            self.set(r, &format!("score {name}={v}"), 1);
        }
    }

    fn level_up(&mut self, entry: &Value, level: usize) {
        let class = entry["class"].as_str().unwrap_or_default();
        let existing = self
            .snap
            .level
            .classes
            .iter()
            .find(|c| {
                c.name
                    .as_deref()
                    .is_some_and(|n| norm::label(n) == norm::label(class))
            })
            .map(|c| c.slot_id.clone());
        let r = match existing {
            Some(slot) => self.app.level_up(&self.id, &slot),
            None => self.app.add_multiclass(&self.id),
        };
        self.set(r, &format!("level up {class}"), level);
    }

    /// Steps that failed only because of order (an invocation whose prerequisite cantrip is
    /// picked with the spells) are tried once more at the end.
    fn retry_failed(&mut self, level: usize) {
        let failed = std::mem::take(&mut self.retry);
        let before = std::mem::take(&mut self.log.failed);
        let mut still = Vec::new();
        for (msg, step, lv) in failed {
            let n = self.log.failed.len();
            self.step(&step, lv.max(level));
            if self.log.failed.len() > n {
                self.log.failed.truncate(n);
                still.push(msg);
            } else {
                self.log.notes.push(format!("{msg} -> applied on retry"));
            }
        }
        self.log.failed = before
            .into_iter()
            .filter(|m| still.contains(m) || !m.contains("no open slot"))
            .collect();
    }

    /// Apply one mapping step; returns the step with its resolved id.
    fn step(&mut self, step: &Value, level: usize) -> Value {
        let mut out = step.clone();
        if let Some(name) = step["pick"].as_str() {
            if let Some(id) = self.pick(name, step, level) {
                out["id"] = json!(id);
            }
        } else if let Some(name) = step["prepare"].as_str() {
            self.prepare(name, step, level);
        } else if let Some(name) = step["item"].as_str() {
            if let Some(id) = self.item(name, step, level) {
                out["id"] = json!(id);
            }
        } else if let Some(list) = step["masteries"].as_array() {
            self.masteries(list, level);
        } else if let Some(note) = step["note"].as_str() {
            if step["gap"].as_bool() == Some(true) {
                self.log.gaps.push(format!("L{level} {note}"));
            }
        } else {
            self.log
                .failed
                .push(format!("L{level} unknown step {step}"));
        }
        out
    }

    fn slot_matches(slot: &borealis_lib::dto::SelectionSlot, filter: &str) -> bool {
        filter.is_empty()
            || norm::name(&slot.element_type) == norm::name(filter)
            || norm::name(&slot.title).contains(&norm::name(filter))
    }

    fn options(&self, slot_id: &str) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        loop {
            let Ok(page) =
                self.app
                    .get_slot_options(&self.id, slot_id, None, Some(PAGE), Some(out.len()))
            else {
                return out;
            };
            let n = page.items.len();
            out.extend(
                page.items
                    .into_iter()
                    .filter(|o| o.disabled_reason.is_none())
                    .map(|o| (o.summary.id, o.summary.name, o.summary.source)),
            );
            if n == 0 || out.len() >= page.total {
                return out;
            }
        }
    }

    fn pick(&mut self, name: &str, step: &Value, level: usize) -> Option<String> {
        let filter = step["slot"].as_str().unwrap_or_default();
        let want_id = step["id"].as_str();
        // The tightest slot first (lowest level gained): a 1st-level spell goes to a level-1
        // slot before a slot that could also hold a 2nd-level one.
        let mut open: Vec<(u32, String)> = self
            .snap
            .slots
            .iter()
            .filter(|s| s.chosen.is_none() && Self::slot_matches(s, filter))
            .map(|s| (s.gained_on_level, s.id.clone()))
            .collect();
        open.sort_by_key(|(lv, _)| *lv);
        for (_, slot) in open {
            let hit = self.options(&slot).into_iter().find(|(id, n, src)| {
                let named = norm::name(n) == norm::name(name);
                match (want_id, self.resolve) {
                    (Some(w), false) => id == w && named,
                    _ => named && self.sources.iter().any(|s| s == src),
                }
            });
            if let Some((id, _, _)) = hit {
                let r = self.app.apply_selection(&self.id, &slot, &id);
                return self.set(r, &format!("pick {name}"), level).then_some(id);
            }
        }
        if let Some(id) = self.already(name, filter) {
            return Some(id);
        }
        if step["spell"].as_bool() == Some(true) {
            if let Some(id) = self.learn_by_replacement(name, filter, level) {
                return Some(id);
            }
        }
        if step["optional"].as_bool() != Some(true) {
            let msg = format!("L{level} pick '{name}' (slot '{filter}'): no open slot offers it");
            self.retry.push((msg.clone(), step.clone(), level));
            self.log.failed.push(msg);
        }
        None
    }

    /// A known spell no open slot can hold yet (a 2nd-level spell at the level that first allows
    /// one): the class's level-up rule lets a known spell be replaced by a new one. An open slot of
    /// the class is filled with its first option, then that pick is replaced (`replace_spell`),
    /// which reopens the slot at the current level, and the wanted spell is chosen there.
    fn learn_by_replacement(&mut self, name: &str, filter: &str, level: usize) -> Option<String> {
        let open: Vec<String> = self
            .snap
            .slots
            .iter()
            .filter(|s| {
                s.chosen.is_none() && s.element_type == "spell" && Self::slot_matches(s, filter)
            })
            .map(|s| s.id.clone())
            .collect();
        for slot in open {
            let allowed = self
                .snap
                .slots
                .iter()
                .any(|s| s.id == slot && s.allow_replace);
            let Some((filler, _, _)) = self.options(&slot).into_iter().next() else {
                continue;
            };
            let r = self.app.apply_selection(&self.id, &slot, &filler);
            if !self.set(r, "replacement filler", level) {
                continue;
            }
            let replaced = allowed
                || self
                    .snap
                    .slots
                    .iter()
                    .any(|s| s.id == slot && s.allow_replace);
            if replaced {
                if let Ok(s) = self.app.replace_spell(&self.id, &slot) {
                    self.snap = s;
                    let hit = self.options(&slot).into_iter().find(|(_, n, src)| {
                        norm::name(n) == norm::name(name) && self.sources.contains(src)
                    });
                    if let Some((id, _, _)) = hit {
                        let r = self.app.apply_selection(&self.id, &slot, &id);
                        if self.set(r, &format!("pick {name} by replacement"), level) {
                            self.log.notes.push(format!(
                                "L{level} {name}: learned by replacing a known spell"
                            ));
                            return Some(id);
                        }
                    }
                    continue;
                }
            }
            let r = self.app.clear_selection(&self.id, &slot);
            self.set(r, "undo replacement filler", level);
        }
        None
    }

    /// `name` is already on the character: chosen in a matching slot (an automatic or single
    /// option) or a known language.
    fn already(&self, name: &str, filter: &str) -> Option<String> {
        let chosen = self.snap.slots.iter().find_map(|s| {
            let c = s.chosen.as_ref()?;
            (Self::slot_matches(s, filter) && norm::name(&c.name) == norm::name(name))
                .then(|| c.id.clone())
        });
        let n = norm::name(name);
        let known = |v: &str| norm::name(v) == n;
        chosen
            .or_else(|| {
                self.snap
                    .stats
                    .languages
                    .iter()
                    .any(|l| known(l))
                    .then(|| format!("language:{name}"))
            })
            .or_else(|| {
                let tool = n
                    .strip_prefix("tool proficiency ")
                    .unwrap_or(&n)
                    .to_string();
                self.snap
                    .stats
                    .proficiencies
                    .iter()
                    .flat_map(|g| g.values.iter())
                    .any(|v| norm::name(v) == tool)
                    .then(|| format!("proficiency:{name}"))
            })
            .or_else(|| {
                self.snap
                    .spellcasting
                    .classes
                    .iter()
                    .flat_map(|c| c.spells.iter())
                    .find(|s| known(&s.name))
                    .map(|s| s.id.clone())
            })
    }

    fn prepare(&mut self, name: &str, step: &Value, level: usize) {
        let class = step["class"].as_str().unwrap_or_default();
        let found = self.snap.spellcasting.classes.iter().find_map(|c| {
            if !class.is_empty() && norm::label(&c.class_name) != norm::label(class) {
                return None;
            }
            c.spells
                .iter()
                .find(|s| norm::name(&s.name) == norm::name(name))
                .map(|s| (c.class_slot_id.clone(), s.id.clone(), s.prepared))
        });
        match found {
            Some((_, _, true)) => {}
            Some((slot, spell, false)) => {
                let r = self.app.set_spell_prepared(&self.id, &slot, &spell, true);
                self.set(r, &format!("prepare {name}"), level);
            }
            None => self.log.failed.push(format!(
                "L{level} prepare '{name}' ({class}): not in the class spell list"
            )),
        }
    }

    fn item(&mut self, name: &str, step: &Value, level: usize) -> Option<String> {
        let filter = borealis_lib::dto_ext::ItemFilter {
            category: None,
            query: Some(name.to_string()),
            source: None,
            limit: Some(PAGE),
            offset: Some(0),
            character_id: Some(self.id.clone()),
        };
        let want_id = step["id"].as_str();
        let page = self.app.search_items(&filter).ok()?;
        let hit = page.items.into_iter().find(|i| {
            let named = norm::name(&i.summary.name) == norm::name(name);
            match (want_id, self.resolve) {
                (Some(w), false) => i.summary.id == w && named,
                _ => named && self.sources.contains(&i.summary.source),
            }
        });
        let Some(hit) = hit else {
            self.log
                .failed
                .push(format!("L{level} item '{name}': not found"));
            return None;
        };
        let id = hit.summary.id.clone();
        let qty = u32::try_from(step["qty"].as_u64().unwrap_or(1)).unwrap_or(1);
        let before: Vec<String> = self
            .snap
            .inventory
            .items
            .iter()
            .map(|i| i.id.clone())
            .collect();
        let r = self.app.add_item(&self.id, &id, qty, false, None);
        if !self.set(r, &format!("add {name}"), level) {
            return Some(id);
        }
        let Some(how) = step["equip"].as_str() else {
            return Some(id);
        };
        let req: EquipRequest = match serde_json::from_value(json!(how)) {
            Ok(r) => r,
            Err(e) => {
                self.log.failed.push(format!("L{level} equip '{how}': {e}"));
                return Some(id);
            }
        };
        let uid = self
            .snap
            .inventory
            .items
            .iter()
            .find(|i| !before.contains(&i.id))
            .map(|i| i.id.clone());
        match uid {
            Some(uid) => {
                let r = self.app.equip_item(&self.id, &uid, req);
                self.set(r, &format!("equip {name} {how}"), level);
            }
            None => self
                .log
                .failed
                .push(format!("L{level} equip '{name}': no new line")),
        }
        Some(id)
    }

    fn masteries(&mut self, list: &[Value], level: usize) {
        let names: Vec<String> = list
            .iter()
            .filter_map(Value::as_str)
            .map(norm::label)
            .collect();
        let ids: Vec<String> = self
            .snap
            .inventory
            .items
            .iter()
            .filter(|i| names.contains(&norm::label(&i.name)))
            .map(|i| i.item_id.clone())
            .collect();
        let r = self.app.choose_weapon_masteries(&self.id, &ids);
        self.set(r, "weapon masteries", level);
    }

    fn record_open(&mut self, level: usize, last: bool) {
        let open: Vec<(String, String, String, bool)> = self
            .snap
            .slots
            .iter()
            .filter(|s| s.chosen.is_none() && !is_cosmetic(&s.element_type, &s.title))
            .map(|s| {
                (
                    s.id.clone(),
                    s.title.clone(),
                    s.element_type.clone(),
                    s.required,
                )
            })
            .collect();
        for (id, title, ty, required) in open {
            let opts = self.options(&id);
            if last && required && !opts.is_empty() {
                let line = format!("{title} [{ty}] ({} options)", opts.len());
                if !self.log.open.iter().any(|l| l.ends_with(&line)) {
                    self.log.open.push(format!("L{level} {line}"));
                }
            }
            if !last {
                continue;
            }
            let mut o = Map::new();
            o.insert("level".into(), json!(level));
            o.insert("slot".into(), json!(id));
            o.insert("title".into(), json!(title));
            o.insert("type".into(), json!(ty));
            o.insert("required".into(), json!(required));
            o.insert(
                "options".into(),
                json!(opts
                    .iter()
                    .take(80)
                    .map(|(i, n, s)| format!("{n} | {i} | {s}"))
                    .collect::<Vec<_>>()),
            );
            self.log.open_options.push(Value::Object(o));
        }
    }
}

/// Slots that never change a number on the sheet (alignment, deity, personality lines).
pub fn is_cosmetic(ty: &str, title: &str) -> bool {
    ty.eq_ignore_ascii_case("deity")
        || ["Alignment", "Personality Trait", "Ideal", "Bond", "Flaw"]
            .iter()
            .any(|t| title.eq_ignore_ascii_case(t))
}
