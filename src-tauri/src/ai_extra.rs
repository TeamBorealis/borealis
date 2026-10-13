//! Deterministic AI-side commands: apply a build plan, evaluate and suggest encounters (no model involved).

use crate::ai_dto::{camelize, snakeize};
use crate::ai_jobs::{constraints_from, parse_band, parse_ruleset, party_from};
use crate::dto::CharacterSnapshot;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use borealis_assist::{
    apply_plan, evaluate, monster_index, suggest_deterministic, Method, MonsterEntry, Plan,
};
use borealis_content::Ruleset;
use serde_json::{json, Value};
use std::collections::HashMap;

fn to_value<T: serde::Serialize>(v: &T) -> ApiResult<Value> {
    serde_json::to_value(v)
        .map(camelize)
        .map_err(ApiError::internal)
}

impl AppState {
    /// Apply the first levels of a plan (`up_to_level` inclusive) to an open character.
    ///
    /// # Errors
    /// `invalid_argument` for a malformed plan, `requirement_failed` when the engine refuses a step
    /// (the character is then unchanged), `not_found` if the character is not open.
    pub fn apply_plan(
        &self,
        id: &str,
        plan: Value,
        up_to_level: u32,
    ) -> ApiResult<CharacterSnapshot> {
        let plan: Plan = serde_json::from_value(snakeize(plan))
            .map_err(|e| ApiError::invalid(format!("Bad build plan: {e}")))?;
        self.mutate(id, |ctx, d| {
            let next = apply_plan(&ctx.hub.engine, &d.character.build, &plan, up_to_level)
                .map_err(|e| ApiError::requirement(e.to_string()))?;
            d.character.build = next;
            Ok(())
        })
    }

    fn encounter_context(&self, args: &Value) -> ApiResult<(Vec<u8>, Ruleset, Vec<MonsterEntry>)> {
        let (party, party_rs) = party_from(self, Some(args))?;
        let ruleset = match args.get("ruleset").and_then(Value::as_str) {
            Some(r) => parse_ruleset(r)?,
            None => party_rs.unwrap_or(self.settings()?.default_ruleset),
        };
        let pool = monster_index(&self.hub()?.store, ruleset);
        Ok((party, ruleset, pool))
    }

    /// Engine verdict for a hand-built encounter: `{ report, monsters }`.
    ///
    /// # Errors
    /// `invalid_argument` for an empty or unknown monster list or a bad party.
    pub fn encounter_evaluate(&self, args: &Value) -> ApiResult<Value> {
        let (party, _, pool) = self.encounter_context(args)?;
        let by_id: HashMap<&str, &MonsterEntry> = pool.iter().map(|m| (m.id.as_str(), m)).collect();
        let mut groups = Vec::new();
        let mut lines = Vec::new();
        for m in args
            .get("monsters")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = m.get("id").and_then(Value::as_str).unwrap_or_default();
            let count = m
                .get("count")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0);
            let (Some(e), Some(n)) = (by_id.get(id), count) else {
                return Err(ApiError::invalid(format!(
                    "Unknown monster or count for '{id}'."
                )));
            };
            groups.push((e.cr, n));
            lines.push(json!({"id": e.id, "name": e.name, "cr": e.cr, "count": n, "xpEach": e.xp}));
        }
        if groups.is_empty() {
            return Err(ApiError::invalid("Add at least one monster."));
        }
        let report = evaluate(&party, &groups, &Method::Srd521)
            .map_err(|e| ApiError::invalid(e.to_string()))?;
        Ok(json!({"report": to_value(&report)?, "monsters": lines}))
    }

    /// Deterministic encounter for a band.
    ///
    /// # Errors
    /// `invalid_argument` for a bad party, band or constraints that admit no monster.
    pub fn encounter_suggest(&self, args: &Value) -> ApiResult<Value> {
        let (party, _, pool) = self.encounter_context(args)?;
        let band = parse_band(
            args.get("band")
                .and_then(Value::as_str)
                .unwrap_or("moderate"),
        )?;
        let constraints = constraints_from(Some(args))?;
        let s = suggest_deterministic(&party, band, &constraints, &pool, &Method::Srd521)
            .map_err(|e| ApiError::invalid(e.to_string()))?;
        to_value(&s)
    }
}
