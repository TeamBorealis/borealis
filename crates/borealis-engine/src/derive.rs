//! One evaluation of a character: graph expansion, statistic evaluation and the snapshot.

use crate::compat::{CompatEntry, CompatibilityReport, EditionBadge};
use crate::diag::{DiagSink, Diagnostic};
use crate::gear::{self, element_ref};
use crate::graph::{Graph, GraphInputs, Mgr, NodeId, Origin};
use crate::ids::{self, ABILITIES, SKILLS};
use crate::inventory;
use crate::levels::{self, PointBuy};
use crate::profile::{profile_for, ProficiencySource, RulesetProfile};
use crate::settings::EngineSettings;
use crate::snapshot::*;
use crate::spells;
use crate::state::{AbilityMethod, CharacterState, ClassSlot, SlotKey};
use crate::stats::{self, StatInputs, StatSet};
use crate::universe::{EditionStatus, UElem, Universe};
use borealis_content::Rule;
use std::collections::{BTreeMap, HashMap, HashSet};

const MAX_OUTER: usize = 6;
const MAX_ABILITY_PASSES: usize = 8;
const DEFAULT_ATTUNEMENT_LIMIT: u32 = 3;

/// A class progression found in the graph.
#[derive(Debug, Clone)]
pub(crate) struct ClassMgr {
    pub slot: ClassSlot,
    pub mgr: Mgr,
    pub node: Option<NodeId>,
    pub level: u32,
}

/// The result of running the pipeline once.
pub(crate) struct Evaluation<'u> {
    pub uni: &'u Universe,
    pub graph: Graph<'u>,
    pub stats: StatSet,
    pub diags: DiagSink,
    pub limit: u32,
    pub classes: Vec<ClassMgr>,
    pub hooks: &'static dyn RulesetProfile,
}

fn class_managers(graph: &Graph<'_>, state: &CharacterState) -> Vec<ClassMgr> {
    let mut out = vec![ClassMgr {
        slot: ClassSlot::Main,
        mgr: Mgr::MainClass,
        node: None,
        level: state.class_level(&ClassSlot::Main),
    }];
    for l in &state.levels {
        if let ClassSlot::Multi { starting_level } = l {
            if !out.iter().any(|c| c.slot == *l) {
                out.push(ClassMgr {
                    slot: l.clone(),
                    mgr: Mgr::Multi(*starting_level),
                    node: None,
                    level: state.class_level(l),
                });
            }
        }
    }
    for c in &mut out {
        let ty = if c.mgr == Mgr::MainClass {
            "Class"
        } else {
            "Multiclass"
        };
        c.node = graph
            .nodes_of_type(ty)
            .find(|(_, n)| n.mgr == c.mgr)
            .map(|(i, _)| i);
    }
    out
}

fn build_inputs<'u>(
    graph: &Graph<'u>,
    state: &CharacterState,
    classes: &[ClassMgr],
    use_max: bool,
    attuned: u32,
    trace: bool,
) -> StatInputs<'u> {
    let total = i32::try_from(graph.total_level()).unwrap_or(20);
    let average = state.has_option(ids::OPT_AVERAGE_HP);
    let mut seeds: Vec<(String, String, i32)> = Vec::new();
    let mut s = |g: &str, l: &str, v: i32| seeds.push((g.to_string(), l.to_string(), v));
    s("level", "Character", total);
    s("level:half", "Character", total / 2);
    s("level:half:up", "Character", total / 2 + total % 2);
    s("level:quarter", "Character", total / 4);
    s(
        "level:quarter:up",
        "Character",
        total / 4 + i32::from(total % 4 != 0),
    );
    for c in classes {
        let Some(n) = c.node else { continue };
        let el = graph.nodes[n].e;
        let name = el.el.name.trim().to_lowercase();
        let lvl = i32::try_from(c.level).unwrap_or(0);
        s(&format!("level:{name}"), "Class", lvl);
        s(&format!("level:{name}:half"), "Class", lvl / 2);
        s(&format!("level:{name}:half:up"), "Class", lvl / 2 + lvl % 2);
        let die = el.hit_die().unwrap_or(8);
        let hp = levels::class_hit_points_with(
            die,
            c.level,
            c.slot == ClassSlot::Main,
            average,
            levels::rolls_for(&state.hp_rolls, &c.slot),
            graph.tables().fixed_hp(&el.el.name),
        );
        s(&format!("hp:{name}"), &el.el.name, hp);
        s("hp", &el.el.name, hp);
    }
    if let ProficiencySource::Table(t) = profile_for(state.ruleset).proficiency_source() {
        let level = graph.total_level().clamp(1, 20);
        let bonus = graph
            .tables()
            .proficiency_bonus(level)
            .unwrap_or(t[(level - 1) as usize]);
        s("proficiency", "Level", bonus);
    }
    s(
        "attunement:current",
        "Attuned items",
        i32::try_from(attuned).unwrap_or(0),
    );
    for (skill, _) in SKILLS {
        s(&format!("{skill}:passive"), "Initial", 10);
    }
    let (rules, skipped) = if trace {
        graph.stat_rules_with_skipped(None)
    } else {
        (graph.active_stat_rules(None), Vec::new())
    };
    StatInputs {
        rules,
        skipped,
        trace,
        seeds,
        builtin: stats::builtin_groups(),
        base_scores: state.abilities.base,
        use_ability_max: use_max,
        merges: spells::spell_merges(graph),
    }
}

impl<'u> Evaluation<'u> {
    pub(crate) fn run(
        uni: &'u Universe,
        state: &CharacterState,
        settings: &EngineSettings,
        trace: bool,
    ) -> Self {
        let hr = &state.house_rules;
        let use_max = hr
            .use_default_ability_maximum
            .unwrap_or(settings.use_default_ability_maximum);
        let limit_override = hr.attunement_limit.or(settings.attunement_limit);
        let mut limit = limit_override.unwrap_or(DEFAULT_ATTUNEMENT_LIMIT);
        let mut abilities = state.abilities.base;
        let mut stat_sums: HashMap<String, i32> = HashMap::new();
        let hooks = profile_for(state.ruleset);
        for iter in 0..MAX_OUTER {
            let mut diags = DiagSink::default();
            uni.report(&mut diags);
            let active = inventory::active_items(uni, &state.inventory, limit, &mut diags);
            let mut graph = Graph::build(
                uni,
                state,
                GraphInputs {
                    abilities,
                    stat_sums: stat_sums.clone(),
                    items: active.roots.clone(),
                    equip: active.equip.clone(),
                    carried: active.carried.clone(),
                },
            );
            let mut classes = class_managers(&graph, state);
            for _ in 0..MAX_ABILITY_PASSES {
                let inputs = build_inputs(
                    &graph,
                    state,
                    &classes,
                    use_max,
                    active.attuned_count,
                    false,
                );
                let ab = stats::evaluate_abilities(&inputs);
                if ab == graph.abilities {
                    break;
                }
                graph.abilities = ab;
                graph.run();
                classes = class_managers(&graph, state);
            }
            let inputs = build_inputs(
                &graph,
                state,
                &classes,
                use_max,
                active.attuned_count,
                trace,
            );
            let stats = stats::evaluate_all(&inputs, &mut diags);
            let new_limit = limit_override.unwrap_or_else(|| {
                let m = stats.sum("attunement:max");
                if m > 0 {
                    u32::try_from(m).unwrap_or(DEFAULT_ATTUNEMENT_LIMIT)
                } else {
                    DEFAULT_ATTUNEMENT_LIMIT
                }
            });
            let sums = stats.sums();
            let stable = graph.abilities == abilities
                && new_limit == limit
                && (!graph.stat_used.get() || sums == stat_sums);
            if stable || iter + 1 == MAX_OUTER {
                // Option filters run after expansion: give them this pass's statistic sums. The
                // expansion itself did not depend on them (stable: either it read no statistic
                // or the sums are unchanged).
                if graph.stat_sums != sums {
                    graph.stat_sums = sums;
                    graph.version += 1;
                }
                for d in graph.diags.items() {
                    diags.push(d.clone());
                }
                if !stable {
                    diags.push(Diagnostic::new(
                        crate::diag::code::EVAL_NOT_CONVERGED,
                        crate::diag::Severity::Warning,
                        "requirements and statistics did not settle",
                    ));
                }
                return Self {
                    uni,
                    graph,
                    stats,
                    diags,
                    limit,
                    classes,
                    hooks,
                };
            }
            abilities = graph.abilities;
            limit = new_limit;
            stat_sums = sums;
        }
        unreachable!("the outer loop always returns")
    }

    /// Ability generation rules: the profile's values overridden by the content tables.
    fn generation(&self) -> crate::profile::AbilityGeneration {
        self.uni
            .tables()
            .generation(self.hooks.ability_generation())
    }

    fn ability_mods(&self) -> ([i32; 6], [i32; 6]) {
        let mut scores = [0; 6];
        let mut mods = [0; 6];
        for (i, a) in ABILITIES.iter().enumerate() {
            scores[i] = self.stats.sum(&format!("{a}:score"));
            mods[i] = self.stats.sum(&format!("{a}:modifier"));
        }
        (scores, mods)
    }

    fn abilities(&self, state: &CharacterState, prof: i32) -> Vec<AbilitySnapshot> {
        let use_max_has = |a: &str| {
            self.stats
                .group(&format!("{a}:max"))
                .is_some_and(|g| !g.contribs.is_empty())
        };
        ABILITIES
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let save_prof = self.stats.sum(&format!("{a}:save:proficiency"));
                let save_misc = self.stats.sum(&format!("{a}:save:misc"))
                    + self.stats.sum(crate::play_rules::D20_GROUP);
                let modifier = self.stats.sum(&format!("{a}:modifier"));
                AbilitySnapshot {
                    name: (*a).to_string(),
                    base: state.abilities.base[i],
                    additional: self.stats.sum(a) + self.stats.sum(&format!("{a}:boon")),
                    override_score: self.stats.sum(&format!("{a}:score:set")),
                    maximum: {
                        let ordinary = if use_max_has(a) {
                            self.stats.sum(&format!("{a}:max"))
                        } else {
                            20
                        };
                        if self.stats.sum(&format!("{a}:boon")) == 0 {
                            ordinary
                        } else {
                            ordinary.max(self.stats.sum(&format!("{a}:boon:cap")))
                        }
                    },
                    score: self.stats.sum(&format!("{a}:score")),
                    modifier,
                    save_proficiency: save_prof,
                    save_misc,
                    save: save_prof + modifier + save_misc,
                    save_proficient: save_prof > 0 && prof > 0,
                }
            })
            .collect()
    }

    fn skills(&self, mods: &[i32; 6], prof: i32) -> Vec<SkillSnapshot> {
        let half = half_proficiency_skills(&self.graph);
        let unproficient_bonus =
            !crate::rule_view::live_rules(&self.graph, &["skill_bonus_unproficient"]).is_empty();
        SKILLS
            .iter()
            .map(|(name, ab)| {
                let p = self.stats.sum(&format!("{name}:proficiency"));
                let misc = self.stats.sum(&format!("{name}:misc"))
                    + self.stats.sum(crate::play_rules::D20_GROUP)
                    + crate::attack_rules::skill_bonus(&self.graph, name, p, mods, prof);
                let total = p + mods[*ab] + misc;
                SkillSnapshot {
                    name: (*name).to_string(),
                    ability: ABILITIES[*ab].to_string(),
                    proficiency: p,
                    misc,
                    total,
                    proficient: p > 0,
                    expertise: prof > 0 && p >= 2 * prof,
                    half_proficiency: p <= 0 && (unproficient_bonus || half.contains(*name)),
                    passive: self.stats.sum(&format!("{name}:passive")) + total,
                }
            })
            .collect()
    }

    fn class_snapshots(&self, state: &CharacterState) -> Vec<ClassSnapshot> {
        let average = state.has_option(ids::OPT_AVERAGE_HP);
        self.classes
            .iter()
            .map(|c| {
                let el = c.node.map(|n| self.graph.nodes[n].e);
                let die = el.and_then(|e| e.hit_die());
                let archetypes = c
                    .node
                    .map(|root| {
                        self.graph
                            .preorder()
                            .into_iter()
                            .filter(|n| {
                                self.graph.nodes[*n].mgr == c.mgr
                                    && self.graph.nodes[*n].e.el.ty == "Archetype"
                            })
                            .map(|n| element_ref(self.graph.nodes[n].e))
                            .filter(|_| root < self.graph.nodes.len())
                            .collect()
                    })
                    .unwrap_or_default();
                ClassSnapshot {
                    slot: c.slot.clone(),
                    class: el.map(element_ref),
                    level: c.level,
                    hit_die: die,
                    hit_points: die.map_or(0, |d| {
                        levels::class_hit_points_with(
                            d,
                            c.level,
                            c.slot == ClassSlot::Main,
                            average,
                            levels::rolls_for(&state.hp_rolls, &c.slot),
                            el.and_then(|e| self.uni.tables().fixed_hp(&e.el.name)),
                        )
                    }),
                    archetypes,
                }
            })
            .collect()
    }

    /// Keys of the snapshot's slots that are not rejected, without building the snapshot: the
    /// same walk as [`Self::slots`] (live nodes in pre-order, select rules only). Option lists
    /// never change which slots exist or are rejected.
    pub(crate) fn active_slot_keys(&self) -> HashSet<SlotKey> {
        let mut out = HashSet::new();
        for id in self.graph.preorder() {
            let n = &self.graph.nodes[id];
            for slot in &n.slots {
                if matches!(n.e.el.rules[slot.rule], Rule::Select(_)) && slot.rejected.is_none() {
                    out.insert(slot.key.clone());
                }
            }
        }
        out
    }

    fn slots(
        &self,
        state: &CharacterState,
        casters: &[SpellcastingSnapshot],
        include_options: bool,
    ) -> Vec<SlotSnapshot> {
        let mut out = Vec::new();
        for id in self.graph.preorder() {
            let n = &self.graph.nodes[id];
            for (si, slot) in n.slots.iter().enumerate() {
                let Rule::Select(rule) = &n.e.el.rules[slot.rule] else {
                    continue;
                };
                let options = if include_options && !rule.is_list() {
                    self.graph.slot_options(id, si)
                } else {
                    Vec::new()
                };
                let chosen = slot.chosen.map(|c| element_ref(self.graph.nodes[c].e));
                let satisfied = chosen.is_some() || slot.list_choice.is_some();
                let optional = rule.optional.unwrap_or(false);
                let default = rule.default_id.clone();
                // engine-optional-sole-option SO-3: a default, a single option and a choice
                // (`optional` plays no part; a sole option without a default stays open)
                let has_default = default.as_deref().is_some_and(|d| !d.trim().is_empty());
                let locked = include_options && has_default && satisfied && options.len() == 1;
                let all_known = !satisfied
                    && !optional
                    && !rule.is_list()
                    && (if include_options {
                        options.is_empty()
                    } else {
                        !self.graph.slot_has_options(id, si)
                    })
                    && self.graph.slot_all_known(id, si);
                let retrain =
                    self.retrain_info(state, casters, slot, rule, self.graph.mgr_level(n.mgr));
                out.push(SlotSnapshot {
                    key: slot.key.clone(),
                    owner: element_ref(n.e),
                    name: rule.name.clone(),
                    ty: rule.ty.clone(),
                    level: rule.level.unwrap_or(1),
                    number: rule.number.unwrap_or(1),
                    optional,
                    chosen,
                    list_choice: slot.list_choice,
                    list_items: rule
                        .items
                        .iter()
                        .filter(|i| {
                            slot.list_choice == Some(i.id)
                                || self.graph.list_item_allowed(id, slot.rule, i.id)
                        })
                        .map(|i| (i.id, i.text.clone()))
                        .collect(),
                    requires_selection: !optional && !satisfied && !all_known,
                    all_known,
                    locked,
                    default,
                    rejected: slot.rejected.clone(),
                    missing: slot.missing.clone(),
                    spellcasting: rule.spellcasting.clone(),
                    prepared: rule.prepared.unwrap_or(false),
                    options: options.iter().map(|o| option_ref(o)).collect(),
                    replaceable: retrain.replaceable,
                    gained_level: retrain.gained_level,
                    replaced: retrain.replaced,
                });
            }
        }
        out
    }

    fn element_tree(&self) -> Vec<RegisteredElement> {
        let order = self.graph.preorder();
        let index: HashMap<NodeId, usize> =
            order.iter().enumerate().map(|(i, n)| (*n, i)).collect();
        order
            .iter()
            .map(|n| {
                let node = &self.graph.nodes[*n];
                RegisteredElement {
                    element: element_ref(node.e),
                    index: index[n],
                    parent: node.parent.and_then(|p| index.get(&p).copied()),
                    acquired: match node.origin {
                        Origin::Root | Origin::Manual => "root",
                        Origin::Granted(_) => "granted",
                        Origin::Selected(_) => "selected",
                    }
                    .to_string(),
                }
            })
            .collect()
    }

    pub(crate) fn snapshot(
        mut self,
        state: &CharacterState,
        settings: &EngineSettings,
        include_options: bool,
        provenance: bool,
    ) -> CharacterSnapshot {
        let (scores, mods) = self.ability_mods();
        let prof = self.stats.sum("proficiency");
        let level = state.level();
        let thresholds = self.thresholds();
        let next_xp = thresholds.get(level as usize).copied();
        let classes = self.class_snapshots(state);
        let total_hp = self.stats.sum("hp") + self.stats.sum("hp:starting");
        let mut diags = std::mem::take(&mut self.diags);
        let spellcasting = spells::spellcasting_snapshots(
            &self.graph,
            self.uni,
            &self.stats,
            state,
            self.hooks,
            &mut diags,
        );
        for area in self.hooks.unimplemented_areas() {
            diags.push(Diagnostic::new(
                crate::profile::UNIMPLEMENTED_CODE,
                crate::diag::Severity::Warning,
                format!(
                    "{} does not implement {area} yet; 2014 behaviour or no effect applies",
                    self.hooks.label()
                ),
            ));
        }
        let multiclass_slots =
            spells::multiclass_slots(&self.graph, self.uni, self.hooks, &mut diags);
        let free_casts = crate::spell_rules::free_casts(&self.graph, prof, &mods);
        let save_dcs = crate::spell_rules::save_dcs(&self.graph, &mods, prof, &spellcasting);
        let inventory = gear::inventory_snapshot(
            self.uni,
            &self.graph,
            &self.stats,
            state,
            self.limit,
            scores[0],
            &mut diags,
        );
        let masteries = crate::play_rules::mastery_picks(&self.graph);
        let martial_arts = crate::play_rules::martial_arts(&self.graph);
        let attacks = gear::attacks_with(
            &gear::AttackCtx {
                uni: self.uni,
                graph: &self.graph,
                stats: &self.stats,
                masteries: &masteries,
                martial_arts: martial_arts.as_ref(),
            },
            state,
            prof,
            &mods,
        );
        let mut attacks = attacks;
        attacks.extend(crate::spell_attacks::cantrip_attacks(
            &self.graph,
            self.uni,
            &self.stats,
            &mods,
            &spellcasting,
        ));
        attacks.extend(crate::breath::breath_attacks(
            &self.graph,
            &self.stats,
            &mods,
        ));
        let mut resources = crate::play_rules::resource_snapshots(&self.graph, &self.stats, &mods);
        resources.extend(crate::spell_rules::free_cast_resources(
            &self.graph,
            &free_casts,
        ));
        let resource_costs = crate::play_rules::resource_costs(&self.graph);
        let toggles = crate::play_rules::toggle_snapshots(&self.graph, &self.stats);
        let exhaustion = crate::play_rules::exhaustion_snapshot(&self.graph, &self.stats);
        let defenses = crate::combat::defenses(&self.graph, &toggles);
        let senses = crate::combat::senses(&self.graph, &self.stats);
        let advantages = crate::combat::advantages(&self.graph, &toggles);
        let mut notes = crate::combat::notes(&self.graph, &self.stats, &mods, &scores);
        notes.extend(crate::notes_d::notes(&self.graph, &self.stats, &mods));
        let wild_shape = crate::wild_shape::wild_shape(
            &self.graph,
            self.uni,
            &self.stats,
            &mods,
            &state.play.wild_shape_forms,
        );
        let equipment_packages =
            crate::packages::packages(&self.graph, self.uni, &state.play.equipment_choices);
        let carry = crate::combat::carry(&self.graph, scores[0]);
        let initiative_advantage = self.graph.has(ids::INITIATIVE_ADVANTAGE)
            || advantages
                .iter()
                .any(|a| a.active && a.roll == "initiative");
        let inspiration_sources = crate::play_rules::inspiration_sources(&self.graph);
        let inspiration = self.graph.runtime.inspiration;
        let mut origin = BTreeMap::new();
        for ty in [
            "Race",
            "Species",
            "Sub Race",
            "Background",
            "Alignment",
            "Deity",
            "Size",
        ] {
            if let Some((_, n)) = self.graph.nodes_of_type(ty).next() {
                origin.insert(ty.to_string(), element_ref(n.e));
            }
        }
        let slots = self.slots(state, &spellcasting, include_options);
        let mut invalid = Vec::new();
        for s in &slots {
            let missing = s.missing.clone().map(crate::state::Choice::Element);
            if let Some(c) = s.rejected.clone().or(missing) {
                invalid.push(InvalidSelection {
                    slot: s.key.clone(),
                    choice: c,
                });
            }
        }
        let stat_map: BTreeMap<String, StatSnapshot> = self
            .stats
            .iter()
            .filter(|(_, g)| !g.contribs.is_empty())
            .map(|(k, g)| {
                (
                    k.clone(),
                    StatSnapshot {
                        value: g.sum(),
                        sources: g.contribs.clone(),
                    },
                )
            })
            .collect();
        let prov = if provenance {
            let all = diags.items();
            stat_map
                .keys()
                .map(|k| (k.clone(), self.trace_of(k, all)))
                .collect()
        } else {
            BTreeMap::new()
        };
        let (features, sheet_summary) = self.features();
        let budget = state
            .house_rules
            .point_buy_budget
            .or(settings.point_buy_budget)
            .unwrap_or(self.generation().point_buy_budget);
        let point_buy = (state.abilities.method == AbilityMethod::PointBuy).then(|| {
            let tables = self.uni.tables();
            let spent: i32 = state
                .abilities
                .base
                .iter()
                .map(|v| tables.point_cost(*v, levels::point_cost))
                .sum();
            let remaining = budget - spent;
            PointBuy {
                budget,
                spent: budget - remaining,
                remaining,
            }
        });
        let abilities = self.abilities(state, prof);
        CharacterSnapshot {
            ruleset: state.ruleset,
            profile: ProfileInfo {
                ability_score_sources: self.hooks.ability_score_sources().to_vec(),
                feat_at_level_1: self.hooks.feat_at_level_1(),
                half_caster_rounding: self.hooks.half_caster_rounding(),
                default_subclass_level: self.hooks.default_subclass_level(),
                standard_array: self.generation().standard_array.to_vec(),
                point_buy_costs: self.uni.tables().point_buy_costs.clone(),
                languages_at_creation: self.hooks.creation_languages(),
            },
            level,
            experience: state.experience,
            next_level_xp: next_xp,
            level_up_available: next_xp.is_some_and(|x| state.experience >= x),
            proficiency_bonus: prof,
            concentration_save: abilities[2].save
                + self.stats.sum(crate::stats::CONCENTRATION_SAVE_GROUP),
            abilities,
            skills: self.skills(&mods, prof),
            armor_class: self.stats.sum("ac"),
            initiative: self.stats.sum("initiative") + self.stats.sum(crate::play_rules::D20_GROUP),
            speed: crate::speed_toggles::apply(&self.graph, self.speeds()),
            hit_points: total_hp,
            classes,
            origin,
            slots,
            elements: self.element_tree(),
            stats: stat_map,
            spellcasting,
            multiclass_slots,
            free_casts,
            save_dcs,
            resources,
            resource_costs,
            toggles,
            exhaustion,
            inspiration,
            inspiration_sources,
            martial_arts,
            masteries,
            inventory,
            attacks,
            point_buy,
            can_multiclass: self.graph.has(ids::MULTICLASS_PREREQ),
            stealth_disadvantage: self.graph.has(ids::STEALTH_DISADVANTAGE),
            initiative_advantage,
            defenses,
            senses,
            advantages,
            notes,
            carry,
            wild_shape,
            equipment_packages,
            invalid_selections: invalid,
            diagnostics: diags.into_vec(),
            compatibility: self.compatibility(),
            features,
            sheet_summary,
            provenance: prov,
        }
    }

    /// The speeds before toggles, with the bonus to every speed the character has
    /// ([`crate::stats::SPEED_ALL_GROUP`], never a new speed).
    fn speeds(&self) -> Speeds {
        let all = self.stats.sum(crate::stats::SPEED_ALL_GROUP);
        let other = |group: &str| {
            let v = self.reduced_speed(group);
            if v > 0 {
                (v + all).max(0)
            } else {
                v
            }
        };
        Speeds {
            walk: (self.stats.sum("speed") + all).max(0),
            fly: other("speed:fly"),
            climb: other("speed:climb"),
            swim: other("speed:swim"),
            burrow: other("speed:burrow"),
        }
    }

    /// A special speed with the exhaustion reduction (no change when the speed is 0).
    fn reduced_speed(&self, group: &str) -> i32 {
        let base = self.stats.sum(group);
        if base <= 0 {
            return base;
        }
        let cut = self.stats.sum("speed") - self.base_walk();
        (base + cut).max(0)
    }

    /// Walking speed without the exhaustion rules (what the stat would be at level 0).
    fn base_walk(&self) -> i32 {
        let penalty =
            crate::play_rules::exhaustion_snapshot(&self.graph, &self.stats).speed_penalty;
        self.stats.sum("speed") - penalty
    }

    /// Foreign-edition elements registered on the character, one entry per element id.
    fn compatibility(&self) -> CompatibilityReport {
        let mut seen = std::collections::HashSet::new();
        let entries = self
            .graph
            .preorder()
            .into_iter()
            .filter_map(|n| {
                let e = self.graph.nodes[n].e;
                let EditionStatus::Foreign {
                    from,
                    converted,
                    notes,
                    ..
                } = &e.edition
                else {
                    return None;
                };
                seen.insert(e.el.id.clone()).then(|| CompatEntry {
                    element: element_ref(e),
                    from: *from,
                    converted: *converted,
                    notes: notes
                        .iter()
                        .cloned()
                        .chain(
                            self.increase_switched_off(n)
                                .then(|| crate::adapter::INCREASE_NOT_APPLIED.to_string()),
                        )
                        .collect(),
                })
            })
            .collect();
        CompatibilityReport {
            strict: !self.graph.allow_cross_edition,
            entries,
        }
    }

    /// IM9a: the node carries the IM9 increase choice but has no slot for it, because the
    /// character's background gives its own increase (the choice's requirement fails).
    fn increase_switched_off(&self, n: NodeId) -> bool {
        let node = &self.graph.nodes[n];
        node.e.el.rules.iter().enumerate().any(|(ri, r)| {
            matches!(r, Rule::Select(s) if s.name == crate::adapter::FLEXIBLE_INCREASE_NAME)
                && node.slots.iter().all(|sl| sl.rule != ri)
        })
    }

    /// XP thresholds from the Level elements (falls back to the standard table).
    pub(crate) fn thresholds(&self) -> Vec<u32> {
        thresholds(self.uni)
    }
}

/// The picker badge of a foreign-edition option.
/// The option row of a candidate element.
pub(crate) fn option_ref(o: &UElem) -> OptionRef {
    OptionRef {
        id: o.el.id.clone(),
        name: o.el.name.clone(),
        source: o.el.source.clone(),
        level: (o.el.ty == "Spell").then(|| o.spell_level().unwrap_or(0)),
        edition: edition_badge(o),
    }
}

fn edition_badge(o: &UElem) -> Option<EditionBadge> {
    match &o.edition {
        EditionStatus::Native => None,
        EditionStatus::Foreign {
            from,
            converted,
            notes,
            ..
        } => Some(EditionBadge {
            from: *from,
            converted: *converted,
            notes: notes.clone(),
        }),
    }
}

/// XP thresholds of levels 1..=20 from the `Level` elements.
pub(crate) fn thresholds(uni: &Universe) -> Vec<u32> {
    let table = uni
        .tables()
        .xp
        .unwrap_or_else(|| profile_for(uni.ruleset()).xp_thresholds());
    (1..=20u32)
        .map(|n| {
            uni.get(&ids::level_id(n))
                .and_then(|e| e.el.setter_value("experience"))
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(table[(n - 1) as usize])
        })
        .collect()
}

/// Skills that an active stat rule gives half proficiency (`<skill>:misc` or
/// `<skill>:proficiency` = `proficiency:half`, `proficiency:half:up`: Jack of All Trades,
/// Remarkable Athlete in Aurora-format content).
fn half_proficiency_skills(graph: &Graph<'_>) -> std::collections::HashSet<String> {
    graph
        .active_stat_rules(None)
        .into_iter()
        .filter(|st| {
            matches!(&st.value, crate::universe::Val::Ref { name, neg: false }
                if name.starts_with("proficiency:half"))
        })
        .filter_map(|st| {
            st.name
                .strip_suffix(":misc")
                .or_else(|| st.name.strip_suffix(":proficiency"))
                .map(str::to_string)
        })
        .collect()
}
