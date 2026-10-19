//! Value checks of the gated 5etools item audit: one equipped item against expectations from
//! its 5etools entry.

use crate::support::{self, ids, Base, RawItem, Tally, Verdict};
use crate::Ctx;
use borealis_content::{Element, Rule, Ruleset};
use borealis_engine::commands;
use borealis_engine::{CharacterSnapshot, CharacterState};
use serde_json::Value;

/// Why a card deck's effect check is wrong: the deck applied one of its card effects.
const DECK_EFFECT_APPLIED: &str = "a card deck applied a card effect (decks grant nothing)";

/// One equipped item on the test character, with its 5etools entry.
pub struct Check<'a> {
    pub cx: &'a Ctx<'a>,
    pub rs: Ruleset,
    pub base: &'a Base,
    pub el: &'a Element,
    pub raw: &'a RawItem,
    pub snap: &'a CharacterSnapshot,
    /// The state of `snap`.
    pub state: &'a CharacterState,
    pub uid: &'a str,
    /// The item attuned on the level-5 fighter (for a barred item `snap` is an eligible host).
    pub fighter_attuned: bool,
    /// The item's attack line on the level-5 fighter (the proficiency check is a fighter's).
    pub fighter_proficient: Option<bool>,
}

impl Check<'_> {
    pub fn all(&self, t: &mut Tally) {
        let id = self.el.id.as_str();
        self.cost_weight(t);
        self.attunement(t);
        match self.el.ty.as_str() {
            "Weapon" => self.weapon(t),
            "Armor" => self.armor(t),
            _ => {}
        }
        if let Some(n) = self.raw.int("bonusAc").filter(|_| self.el.ty != "Armor") {
            let got = self.snap.armor_class - self.base.snap.armor_class;
            self.effect(
                t,
                "AC bonus (non-armor, ring/cloak style)",
                id,
                verdict(got == n, || format!("AC {got:+} want {n:+}")),
            );
        }
        if let Some(n) = self.raw.int("bonusSavingThrow") {
            let deltas: Vec<i32> = (0..6)
                .map(|i| self.snap.abilities[i].save - self.base.snap.abilities[i].save)
                .collect();
            let ok = deltas.iter().all(|d| *d == n);
            self.effect(
                t,
                "saving throw bonus",
                id,
                verdict(ok, || format!("deltas {deltas:?} want {n:+}")),
            );
        }
        if let Some(n) = self.raw.int("bonusProficiencyBonus") {
            let got = self.snap.proficiency_bonus - self.base.snap.proficiency_bonus;
            self.effect(
                t,
                "proficiency bonus bonus",
                id,
                verdict_nm(got == n, got == 0, || format!("{got:+}")),
            );
        }
        self.abilities(t);
        self.defences(t);
        self.speed(t);
        self.charges(t);
        self.spells(t);
        self.rule_presence(t);
        if self.raw.code() == "A" || self.raw.code() == "AF" {
            self.ammunition(t);
        }
    }

    /// A card deck (`seeAlsoDeck`: Deck of Many Things, Deck of Wonder...): its effect fields
    /// sum up what single random cards do when drawn, and the importer keeps them as data only,
    /// as Aurora's Deck of Many Things grants nothing. Its effect checks are not modelled by
    /// design; a card effect that does apply is wrong.
    pub fn is_card_deck(&self) -> bool {
        self.raw.0.contains_key("seeAlsoDeck")
    }

    /// Records an effect check (bonuses, abilities, defences, speed, spells, granted rules).
    fn effect(&self, t: &mut Tally, cat: &str, id: &str, v: Verdict) {
        let v = match v {
            v if !self.is_card_deck() => v,
            Verdict::Correct => Verdict::Wrong(DECK_EFFECT_APPLIED.into()),
            Verdict::Wrong(_) | Verdict::NotModelled => Verdict::NotModelled,
        };
        t.record(cat, id, v);
    }

    fn cost_weight(&self, t: &mut Tally) {
        let id = self.el.id.as_str();
        let line = self.snap.inventory.items.iter().find(|l| l.uid == self.uid);
        let Some(line) = line else {
            t.record("cost", id, Verdict::Wrong("no inventory line".into()));
            return;
        };
        if let Some(v) = self.raw.num("value").filter(|v| *v > 0.0) {
            let want = v / 100.0;
            let ok = (line.cost_gp - want).abs() < 1e-6;
            t.record(
                "cost (gp)",
                id,
                verdict(ok, || format!("{} want {want}", line.cost_gp)),
            );
        }
        if let Some(w) = self.raw.num("weight").filter(|w| *w > 0.0) {
            let ok = (line.weight - w).abs() < 1e-6;
            t.record(
                "weight (lb)",
                id,
                verdict(ok, || format!("{} want {w}", line.weight)),
            );
        }
    }

    fn attunement(&self, t: &mut Tally) {
        let id = self.el.id.as_str();
        let want = self.raw.attunes();
        let line = self.snap.inventory.items.iter().find(|l| l.uid == self.uid);
        let Some(line) = line else { return };
        if want || line.requires_attunement {
            let ok = line.requires_attunement == want
                && (!want || line.attuned || line.equipped.is_none());
            t.record(
                "attunement required + attuned on equip",
                id,
                verdict(ok, || {
                    format!(
                        "requires {} attuned {} want {want}",
                        line.requires_attunement, line.attuned
                    )
                }),
            );
        }
        let Some(barred) = self.raw.fighter_cannot_attune() else {
            return;
        };
        // a level-5 fighter without a subclass is neither the named class nor a spellcaster
        let text_kept = self
            .el
            .setter("attunement")
            .and_then(|s| s.attribute("addition"))
            .is_some();
        let v = if !self.fighter_attuned {
            Verdict::Correct
        } else if text_kept {
            Verdict::NotModelled
        } else {
            Verdict::Wrong("restriction lost".into())
        };
        t.record(&format!("attunement restriction ({barred})"), id, v);
    }

    fn weapon(&self, t: &mut Tally) {
        let id = self.el.id.as_str();
        let Some(atk) = self.snap.attacks.iter().find(|a| a.uid == self.uid) else {
            t.record(
                "weapon base attack/damage",
                id,
                Verdict::Wrong("no attack line".into()),
            );
            return;
        };
        let props = self.raw.properties();
        let ranged = self.raw.code() == "R";
        let finesse = props.iter().any(|p| p == "F");
        // the equipped character's modifiers: the weapon may change a score itself
        let modifier = |i: usize| self.snap.abilities[i].modifier;
        let (s, d) = (modifier(0), modifier(1));
        let ability = if ranged || (finesse && d > s) { d } else { s };
        let n = self.raw.int("bonusWeapon").unwrap_or(0);
        let extra_atk = self.raw.int("bonusWeaponAttack").unwrap_or(0);
        let extra_dmg = self.raw.int("bonusWeaponDamage").unwrap_or(0);
        let prof = if atk.proficient {
            self.base.snap.proficiency_bonus
        } else {
            0
        };
        let want_atk = prof + ability + n + extra_atk;
        let dmg_type = self
            .raw
            .str("dmgType")
            .and_then(support::damage_type)
            .unwrap_or("");
        let dice = self.raw.str("dmg1").unwrap_or("");
        let want_dmg = support::dice_with_bonus(dice, ability + n + extra_dmg, dmg_type);
        let ok = atk.attack == want_atk && atk.damage == want_dmg;
        let why = || {
            format!(
                "{:+} {} want {want_atk:+} {want_dmg}",
                atk.attack, atk.damage
            )
        };
        let cat = if n != 0 || extra_atk != 0 || extra_dmg != 0 {
            "+N weapon attack/damage"
        } else {
            "weapon base attack/damage"
        };
        // a +N that shows up nowhere is not modelled; a wrong base formula is wrong
        let unchanged = atk.attack == want_atk - n - extra_atk;
        t.record(
            cat,
            id,
            if ok {
                Verdict::Correct
            } else if cat.starts_with('+') && unchanged {
                Verdict::NotModelled
            } else {
                Verdict::Wrong(why())
            },
        );
        let category = self.raw.str("weaponCategory").unwrap_or("");
        let proficient = self.fighter_proficient.unwrap_or(atk.proficient);
        if matches!(category, "simple" | "martial") && !self.raw.flag("firearm") {
            let why = || match self.el.setter_value("proficiency") {
                None => format!(
                    "not proficient: no proficiency setter (base {})",
                    self.raw
                        .str("baseItem")
                        .unwrap_or("-")
                        .split('|')
                        .next()
                        .unwrap_or("-")
                ),
                Some(p) => format!("not proficient: {p} not granted"),
            };
            t.record(
                "weapon proficiency (fighter, simple/martial)",
                id,
                verdict(proficient, why),
            );
        }
        if let Some(c) = self.raw.int("critThreshold") {
            let crit = i32::from(atk.crit_range);
            t.record(
                "crit threshold",
                id,
                verdict_nm(crit == c, crit == 20, || format!("{crit} want {c}")),
            );
        }
        // a core property is its tag; a book's own (`Evo|AU`) must be an imported Weapon
        // Property element the weapon supports
        let raw_props = self.raw.raw_properties();
        let held = |tag: &str| {
            self.el.supports.iter().any(|s| s == tag)
                && (tag.starts_with("ID_INTERNAL_")
                    || self
                        .cx
                        .ours
                        .get(tag)
                        .is_some_and(|(e, _)| e.ty == "Weapon Property"))
        };
        let missing: Vec<String> = raw_props
            .iter()
            .filter(|p| match support::property_tag(p) {
                Some(tag) => !held(&tag),
                None => true,
            })
            .cloned()
            .collect();
        if !props.is_empty() {
            let unmapped = missing.iter().all(|p| support::property_tag(p).is_none());
            t.record(
                "weapon properties",
                id,
                if missing.is_empty() {
                    Verdict::Correct
                } else if unmapped {
                    Verdict::NotModelled
                } else {
                    Verdict::Wrong(format!("missing {missing:?}"))
                },
            );
        }
        if let Some(m) = self.raw.mastery().filter(|_| self.rs == Ruleset::Dnd2024) {
            let got = atk.mastery.as_ref().map(|r| r.name.as_str());
            t.record(
                "weapon mastery (2024)",
                id,
                verdict(got == Some(m.as_str()), || format!("{got:?} want {m}")),
            );
        }
    }

    fn armor(&self, t: &mut Tally) {
        let id = self.el.id.as_str();
        let code = self.raw.code();
        let Some(ac) = self.raw.int("ac") else { return };
        let bonus = self.raw.int("bonusAc").unwrap_or(0);
        let dex = self.base.mods[1];
        let want = match code {
            "S" => self.base.snap.armor_class + ac + bonus,
            "LA" => ac + dex + bonus,
            "MA" if self.raw.0.get("dexterityMax") == Some(&Value::Null) => ac + dex + bonus,
            "MA" => ac + dex.min(2) + bonus,
            "HA" => ac + bonus,
            _ => return,
        };
        let got = self.snap.armor_class;
        let cat = match (code, bonus != 0) {
            ("S", false) => "shield AC",
            ("S", true) => "+N shield AC",
            (_, false) => "base armor AC (light/medium/heavy dex caps)",
            (_, true) => "+N armor AC",
        };
        t.record(
            cat,
            id,
            verdict(got == want, || format!("AC {got} want {want}")),
        );
        if code == "S" {
            self.shield_over_armor(t, ac + bonus);
        }
        if code != "S" {
            let stealth = self.raw.flag("stealth");
            t.record(
                "armor stealth disadvantage",
                id,
                verdict(self.snap.stealth_disadvantage == stealth, || {
                    format!("{} want {stealth}", self.snap.stealth_disadvantage)
                }),
            );
        }
        if let Some(need) = self.raw.int("strength") {
            self.strength_requirement(t, need);
        }
    }

    /// The shield's AC on top of a breastplate (AC 14 + Dex, max 2).
    fn shield_over_armor(&self, t: &mut Tally, shield_ac: i32) {
        let id = self.el.id.as_str();
        let armored = support::equip_state(
            self.cx.engine,
            &self.base.state,
            ids::breastplate(self.rs),
            None,
        )
        .and_then(|s| support::equipped(self.cx.engine, &s, &self.el.id, None));
        let Some(snap) = armored else { return };
        let want = 14 + self.base.mods[1].min(2) + shield_ac;
        let cat = if self.raw.int("bonusAc").is_some() {
            "+N shield AC over body armor"
        } else {
            "shield AC over body armor"
        };
        t.record(
            cat,
            id,
            verdict(snap.armor_class == want, || {
                format!("AC {} want {want}", snap.armor_class)
            }),
        );
    }

    /// Speed drops by 10 on a weak character (Str below the requirement), not on the base one.
    fn strength_requirement(&self, t: &mut Tally, need: i32) {
        let id = self.el.id.as_str();
        let weak = &self.base.weak;
        let walk = |s: &CharacterSnapshot| s.speed.walk;
        let strong_ok = self.base.scores[0] < need || walk(self.snap) == walk(&self.base.snap);
        let weak_snap = commands::add_item(self.cx.engine, &weak.state, &self.el.id, 1)
            .ok()
            .and_then(|o| {
                let uid = o.state.inventory.items.last()?.uid.clone();
                commands::equip(self.cx.engine, &o.state, &uid, None).ok()
            })
            .map(|o| self.cx.engine.derive(&o.state));
        let Some(ws) = weak_snap else {
            t.record(
                "armor strength requirement",
                id,
                Verdict::Wrong("weak character cannot equip".into()),
            );
            return;
        };
        let weak_ok = walk(&ws) == walk(&weak.snap) - 10;
        t.record(
            "armor strength requirement",
            id,
            verdict(strong_ok && weak_ok, || {
                format!(
                    "strong {} weak {} (base {})",
                    walk(self.snap),
                    walk(&ws),
                    walk(&weak.snap)
                )
            }),
        );
    }

    fn abilities(&self, t: &mut Tally) {
        let id = self.el.id.as_str();
        let Some(Value::Object(a)) = self.raw.0.get("ability") else {
            return;
        };
        for (key, v) in a {
            if key == "static" {
                for (abbr, n) in v.as_object().into_iter().flatten() {
                    let (Some(i), Some(n)) = (support::ability_index(abbr), v_i32(n)) else {
                        continue;
                    };
                    let base = self.base.snap.abilities[i].score;
                    let got = self.snap.abilities[i].score;
                    let want = base.max(n);
                    self.effect(
                        t,
                        "ability score set (\"set Str to 19\")",
                        id,
                        verdict_nm(got == want, got == base, || {
                            format!("{abbr} {got} want {want}")
                        }),
                    );
                }
            } else if key == "choose" || key == "from" || key == "count" || key == "amount" {
                let opened = self.snap.slots.iter().any(|s| s.owner.id == self.el.id);
                self.effect(
                    t,
                    "ability bonus, chosen ability",
                    id,
                    if opened {
                        Verdict::Correct
                    } else {
                        Verdict::NotModelled
                    },
                );
                break;
            } else if let (Some(i), Some(n)) = (support::ability_index(key), v_i32(v)) {
                let base = self.base.snap.abilities[i].score;
                let got = self.snap.abilities[i].score;
                let want = (base + n).min(20.max(base));
                self.effect(
                    t,
                    "ability score flat bonus",
                    id,
                    verdict_nm(got == want, got == base, || {
                        format!("{key} {got} want {want}")
                    }),
                );
            }
        }
    }

    fn defences(&self, t: &mut Tally) {
        let id = self.el.id.as_str();
        let d = &self.snap.defenses;
        for (key, list, cat) in [
            ("resist", &d.resistances, "damage resistance"),
            ("immune", &d.immunities, "damage immunity"),
            ("vulnerable", &d.vulnerabilities, "damage vulnerability"),
            (
                "conditionImmune",
                &d.condition_immunities,
                "condition immunity",
            ),
        ] {
            for entry in self.raw.list(key) {
                let Some(name) = entry.as_str() else {
                    self.effect(
                        t,
                        &format!("{cat} (choice/conditional form)"),
                        id,
                        Verdict::NotModelled,
                    );
                    continue;
                };
                let had = |l: &[borealis_engine::combat::DefenseEntry]| {
                    l.iter().any(|e| e.name == name && e.active)
                };
                let base_list = match key {
                    "resist" => &self.base.snap.defenses.resistances,
                    "immune" => &self.base.snap.defenses.immunities,
                    "vulnerable" => &self.base.snap.defenses.vulnerabilities,
                    _ => &self.base.snap.defenses.condition_immunities,
                };
                let got = had(list);
                self.effect(
                    t,
                    cat,
                    id,
                    verdict_nm(got, !got && !had(base_list), || format!("{name} missing")),
                );
            }
        }
    }

    fn speed(&self, t: &mut Tally) {
        let id = self.el.id.as_str();
        let Some(Value::Object(m)) = self.raw.0.get("modifySpeed") else {
            return;
        };
        let get = |s: &CharacterSnapshot, mode: &str| match mode {
            "walk" => s.speed.walk,
            "fly" => s.speed.fly,
            "climb" => s.speed.climb,
            "swim" => s.speed.swim,
            "burrow" => s.speed.burrow,
            _ => -1,
        };
        for (how, modes) in m {
            for (mode, v) in modes.as_object().into_iter().flatten() {
                // a bonus to every speed shows on the walking speed, which every character has
                let mode = if mode == "*" { "walk" } else { mode.as_str() };
                let before = get(&self.base.snap, mode);
                let got = get(self.snap, mode);
                if how == "multiply" {
                    self.speed_multiply(t, m, mode, v);
                    continue;
                }
                // `equal` + `multiply` of one mode (Eagle Whistle: fly at twice the walk while
                // blowing): the toggle's speed (on: `speed_multiply`); off, no speed of its own
                let multiplied = m.get("multiply").and_then(|x| x.get(mode)).is_some();
                if how == "equal" && multiplied {
                    self.effect(
                        t,
                        "speed change (modifySpeed)",
                        id,
                        verdict(got == before, || {
                            format!("{mode} {got} with the toggle off, want {before}")
                        }),
                    );
                    continue;
                }
                let want = match (how.as_str(), v) {
                    ("static", n) => v_i32(n).map(|n| before.max(n)),
                    ("equal", Value::String(other)) => Some(get(self.snap, other)),
                    ("bonus", n) => v_i32(n).map(|n| before + n),
                    _ => None,
                };
                let Some(want) = want else {
                    self.effect(t, "speed change (other form)", id, Verdict::NotModelled);
                    continue;
                };
                if want == before {
                    continue;
                }
                self.effect(
                    t,
                    "speed change (modifySpeed)",
                    id,
                    verdict_nm(got == want, got == before, || {
                        format!("{how} {mode} {got} want {want}")
                    }),
                );
            }
        }
    }

    /// `multiply` (Boots of Speed, Eagle Whistle): a toggle of the item; switched on, the speed
    /// becomes `factor` times itself, or times the speed an `equal` entry names.
    fn speed_multiply(
        &self,
        t: &mut Tally,
        m: &serde_json::Map<String, Value>,
        mode: &str,
        v: &Value,
    ) {
        let id = self.el.id.as_str();
        let cat = "speed change (other form)";
        let Some(factor) = v_i32(v) else {
            self.effect(t, cat, id, Verdict::NotModelled);
            return;
        };
        let get = |s: &CharacterSnapshot, mode: &str| match mode {
            "walk" => s.speed.walk,
            "fly" => s.speed.fly,
            "climb" => s.speed.climb,
            "swim" => s.speed.swim,
            "burrow" => s.speed.burrow,
            _ => -1,
        };
        let of = m
            .get("equal")
            .and_then(|e| e.get(mode))
            .and_then(Value::as_str)
            .unwrap_or(mode);
        let toggle = self
            .snap
            .toggles
            .iter()
            .find(|tg| tg.source == self.el.name || tg.id.contains(&self.el.id));
        let on = toggle
            .and_then(|tg| commands::set_toggle(self.cx.engine, self.state, &tg.id, true).ok());
        let Some(on) = on else {
            self.effect(t, cat, id, Verdict::NotModelled);
            return;
        };
        let toggled = self.cx.engine.derive(&on.state);
        let want = factor * get(self.snap, of);
        let got = get(&toggled, mode);
        self.effect(
            t,
            cat,
            id,
            verdict_nm(got == want, got == get(self.snap, mode), || {
                format!("{mode} {got} want {want}")
            }),
        );
    }

    fn charges(&self, t: &mut Tally) {
        let id = self.el.id.as_str();
        let Some(n) = self.raw.int("charges") else {
            return;
        };
        let res = self.snap.resources.iter().find(|r| {
            r.source == self.el.name || r.source == self.el.id || r.id.contains(&self.el.id)
        });
        let v = match res {
            Some(r) if r.max == n => Verdict::Correct,
            Some(r) => Verdict::Wrong(format!("max {} want {n}", r.max)),
            None => Verdict::NotModelled,
        };
        t.record("charges/uses", id, v);
    }

    fn spells(&self, t: &mut Tally) {
        let id = self.el.id.as_str();
        if !self.raw.0.contains_key("attachedSpells") {
            return;
        }
        let by_item = |s: &str| s == self.el.name || s == self.el.id;
        let free = self.snap.free_casts.iter().any(|f| by_item(&f.source));
        let casting = self.snap.spellcasting.iter().any(|s| by_item(&s.name));
        self.effect(
            t,
            "spell-granting item (attachedSpells)",
            id,
            if free || casting {
                Verdict::Correct
            } else {
                Verdict::NotModelled
            },
        );
    }

    /// Item properties that only a rule can carry: no rule of the right shape = not modelled.
    fn rule_presence(&self, t: &mut Tally) {
        let id = self.el.id.as_str();
        let stat_like = |needle: &str| {
            self.el.rules.iter().any(|r| match r {
                Rule::Stat(s) => s.name.contains(needle),
                _ => false,
            })
        };
        let granted = |ty: &str| {
            self.el.rules.iter().any(|r| match r {
                Rule::Grant(g) => g.ty.as_deref() == Some(ty),
                Rule::Select(s) => s.ty == ty,
                _ => false,
            })
        };
        for (key, cat, ok) in [
            ("bonusSpellAttack", "spell attack bonus", stat_like("spell")),
            ("bonusSpellSaveDc", "spell save DC bonus", stat_like("dc")),
            (
                "bonusAbilityCheck",
                "ability check bonus",
                stat_like("check"),
            ),
            (
                "bonusSavingThrowConcentration",
                "concentration save bonus",
                stat_like("concentration"),
            ),
            (
                "grantsProficiency",
                "grants proficiency",
                granted("Proficiency"),
            ),
            ("grantsLanguage", "grants language", granted("Language")),
        ] {
            let by_item = key == "grantsProficiency"
                && !ok
                && self.raw.0.contains_key(key)
                && self.el.setter_value("proficient") == Some("true");
            if by_item {
                // "proficient with this item": the worn line itself must be proficient
                let line = self.snap.inventory.items.iter().find(|l| l.uid == self.uid);
                let applied = line.is_some_and(|l| l.proficient_by_item);
                self.effect(
                    t,
                    cat,
                    id,
                    verdict(applied, || {
                        "proficient setter not applied to the line".into()
                    }),
                );
                continue;
            }
            if self.raw.0.contains_key(key) {
                self.effect(
                    t,
                    cat,
                    id,
                    if ok {
                        Verdict::Correct
                    } else {
                        Verdict::NotModelled
                    },
                );
            }
        }
    }

    /// `+N` ammunition: carried next to an equipped bow of the same edition, the bow's attack
    /// and damage should rise by N.
    fn ammunition(&self, t: &mut Tally) {
        let id = self.el.id.as_str();
        let Some(n) = self.raw.int("bonusWeapon") else {
            return;
        };
        let Some(launcher) = self.launcher() else {
            t.record("+N ammunition", id, Verdict::Wrong("no launcher".into()));
            return;
        };
        let Ok(o) = commands::add_item(self.cx.engine, &launcher.state, &self.el.id, 1) else {
            return;
        };
        let snap = self.cx.engine.derive(&o.state);
        let atk = |s: &CharacterSnapshot| s.attacks.first().map(|a| (a.attack, a.damage.clone()));
        let (before, after) = (atk(&launcher.snap), atk(&snap));
        let v = match (before, after) {
            (Some((a0, _)), Some((a1, _))) if a1 == a0 + n => Verdict::Correct,
            (Some((a0, _)), Some((a1, _))) if a1 == a0 => Verdict::NotModelled,
            other => Verdict::Wrong(format!("{other:?}")),
        };
        t.record("+N ammunition", id, v);
    }
}

impl Check<'_> {
    /// The fighter with the weapon that fires this ammunition equipped (a bow for arrows and
    /// unnamed ammunition, a crossbow for bolts, a sling for sling bullets, a pistol for other
    /// bullets, a blowgun for needles, a laser pistol for energy cells).
    fn launcher(&self) -> Option<support::Sub> {
        let name = self.el.name.to_lowercase();
        let weapon = if name.contains("bolt") {
            ids::light_crossbow(self.rs)
        } else if name.contains("sling") {
            ids::sling(self.rs)
        } else if name.contains("bullet") {
            ids::pistol(self.rs)
        } else if name.contains("needle") {
            ids::blowgun(self.rs)
        } else if name.contains("energy cell") {
            ids::laser_pistol(self.rs)
        } else {
            return Some(support::Sub {
                state: self.base.with_bow.state.clone(),
                snap: self.base.with_bow.snap.clone(),
            });
        };
        let state = support::equip_state(self.cx.engine, &self.base.state, weapon, None)?;
        Some(support::Sub {
            snap: self.cx.engine.derive(&state),
            state,
        })
    }
}

pub fn v_i32(v: &Value) -> Option<i32> {
    v.as_i64().and_then(|n| i32::try_from(n).ok())
}

pub fn verdict(ok: bool, why: impl FnOnce() -> String) -> Verdict {
    if ok {
        Verdict::Correct
    } else {
        Verdict::Wrong(why())
    }
}

/// Correct when `ok`; not modelled when the sheet shows no effect at all; wrong otherwise.
pub fn verdict_nm(ok: bool, no_effect: bool, why: impl FnOnce() -> String) -> Verdict {
    if ok {
        Verdict::Correct
    } else if no_effect {
        Verdict::NotModelled
    } else {
        Verdict::Wrong(why())
    }
}
