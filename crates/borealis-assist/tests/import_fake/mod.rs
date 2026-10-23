//! Shared fixtures of the import-anything tests: the SRD packs and a scripted fake provider
//! (no CLI is ever started).
#![allow(dead_code)]

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

use borealis_ai::task::{AiRunner, BoxFuture, RunOptions};
use borealis_ai::{AiError, AiEvent, AiOutcome, AiRequest, Attachment};
use borealis_assist::import::anything::ImportSource;
use borealis_assist::{AnythingRequest, Edition, VisionRunner};
use borealis_content::ContentStoreBuilder;
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

pub fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

pub fn base() -> ContentStoreBuilder {
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.2.1"] {
        b = b.add_pack_dir(&root().join(pack)).expect("pack loads");
    }
    b
}

/// One scripted rule: matches the task name and a substring of the prompt; the last reply sticks.
pub struct Rule {
    pub task: &'static str,
    pub needle: String,
    pub replies: VecDeque<String>,
}

#[derive(Default)]
pub struct Fake {
    pub rules: Mutex<Vec<Rule>>,
    pub log: Mutex<Vec<(String, String)>>,
    pub attachments_seen: Mutex<Vec<usize>>,
}

impl Fake {
    pub fn on(self, task: &'static str, needle: &str, replies: &[String]) -> Self {
        self.rules.lock().unwrap().push(Rule {
            task,
            needle: needle.to_string(),
            replies: replies.iter().cloned().collect(),
        });
        self
    }

    pub fn calls(&self, task: &str) -> usize {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|(t, _)| t == task)
            .count()
    }

    pub fn prompts(&self, task: &str) -> Vec<String> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|(t, _)| t == task)
            .map(|(_, p)| p.clone())
            .collect()
    }

    pub fn answer(&self, req: &AiRequest) -> Result<AiOutcome, AiError> {
        let system = req.system.clone().unwrap_or_default();
        let task = system
            .lines()
            .find_map(|l| l.strip_prefix("TASK: "))
            .unwrap_or("?")
            .to_string();
        self.log
            .lock()
            .unwrap()
            .push((task.clone(), req.prompt.clone()));
        let mut rules = self.rules.lock().unwrap();
        let rule = rules
            .iter_mut()
            .find(|r| r.task == task && req.prompt.contains(&r.needle))
            .ok_or(AiError::NoResult)?;
        let text = if rule.replies.len() > 1 {
            rule.replies.pop_front().unwrap()
        } else {
            rule.replies.front().cloned().ok_or(AiError::NoResult)?
        };
        Ok(AiOutcome {
            text,
            ..AiOutcome::default()
        })
    }
}

impl AiRunner for Fake {
    fn run<'a>(
        &'a self,
        req: &'a AiRequest,
        _sink: Option<&'a UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        let r = self.answer(req);
        Box::pin(async move { r })
    }
}

impl VisionRunner for Fake {
    fn run_with<'a>(
        &'a self,
        req: &'a AiRequest,
        attachments: &'a [Attachment],
        _sink: Option<&'a UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        self.attachments_seen
            .lock()
            .unwrap()
            .push(attachments.len());
        let r = self.answer(req);
        Box::pin(async move { r })
    }
}

pub fn opts() -> RunOptions<'static> {
    RunOptions {
        max_repairs: 2,
        sink: None,
    }
}

pub fn request(source: ImportSource, prefix: &str, edition: Edition) -> AnythingRequest {
    AnythingRequest {
        source,
        id_prefix: prefix.into(),
        source_name: "Homebrew".into(),
        pack_id: format!("{prefix}-import"),
        pack_name: format!("{prefix} (import)"),
        edition_hint: edition,
        edition_override: None,
    }
}

pub fn detect(kind: &str, name: &str, parent: Option<&str>, edition: &str) -> String {
    json!({"kind": kind, "edition": edition, "confidence": 0.9,
        "items": [{"kind": kind, "name": name, "parent_hint": parent}]})
    .to_string()
}

pub fn outline(name: &str, extra: Value, features: &[(u32, &str)]) -> String {
    let fs: Vec<Value> = features
        .iter()
        .map(|(l, n)| json!({"level": l, "name": n, "summary": format!("{n} effect"), "quote": format!("{n}.")}))
        .collect();
    let mut o = json!({"name": name, "summary": "x", "features": fs, "quote": name});
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        o[k] = v;
    }
    json!({"outline": o}).to_string()
}

pub fn feature(name: &str, extra: Value) -> String {
    let mut el = json!({"id": "ID_MODEL_CHOSE", "name": name, "type": "Class Feature",
        "description": format!("<p>{name}.</p>")});
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        el[k] = v;
    }
    json!({"element": el, "notes": []}).to_string()
}

/// A batched generation reply: `{"elements": [{element, notes}, ...]}` from single-element replies.
pub fn batch_reply(replies: &[String]) -> String {
    let els: Vec<Value> = replies
        .iter()
        .map(|r| serde_json::from_str(r).unwrap())
        .collect();
    json!({ "elements": els }).to_string()
}

/// The marker every batched feature prompt carries.
pub const BATCH_NEEDLE: &str = "in ONE answer";

pub fn el_reply(name: &str, ty: &str) -> String {
    json!({"element": {"id": "X", "name": name, "type": ty,
        "description": format!("<p>{name}.</p>")}, "notes": []})
    .to_string()
}

pub fn outline_of(name: &str, features: &[(u32, &str)]) -> Value {
    serde_json::from_str::<Value>(&outline(name, json!({}), features)).unwrap()["outline"].clone()
}
