//! Shared fixtures: SRD 5.1 engine, a fake runner (no CLI is ever started) and character helpers.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use borealis_ai::task::{AiRunner, BoxFuture};
use borealis_ai::{AiError, AiEvent, AiOutcome, AiRequest};
use borealis_content::{ContentStore, ContentStoreBuilder, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::{CharacterState, Choice, Engine};
use tokio::sync::mpsc::UnboundedSender;

pub const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
pub const GRAPPLER: &str = "ID_PHB_FEAT_GRAPPLER";

pub fn store() -> &'static ContentStore {
    static STORE: OnceLock<ContentStore> = OnceLock::new();
    STORE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let mut b = ContentStoreBuilder::new();
        for pack in ["core", "srd-5.1"] {
            b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
        }
        b.build()
    })
}

pub fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| Engine::new(store()))
}

/// A 2014 Fighter with the given base scores, taken to `level` in the main class.
pub fn fighter(level: u32, scores: [i32; 6]) -> CharacterState {
    let e = engine();
    let mut s = commands::new_character(e, Ruleset::Dnd2014);
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).expect("class slot");
    s = commands::select(e, &s, &key, Choice::Element(FIGHTER.into()))
        .expect("fighter")
        .state;
    s = commands::set_base_scores(e, &s, scores)
        .expect("scores")
        .state;
    for _ in 1..level {
        s = commands::level_up_main(e, &s, None).expect("level").state;
    }
    s
}

/// The only Martial Archetype the SRD fighter offers.
pub fn archetype_id() -> String {
    let s = fighter(3, [15, 12, 14, 10, 10, 10]);
    let snap = engine().derive(&s);
    snap.slots
        .iter()
        .find(|x| x.name == "Martial Archetype")
        .expect("archetype slot")
        .options[0]
        .id
        .clone()
}

/// Returns queued replies in order and records every prompt it saw.
pub struct FakeRunner {
    replies: Mutex<VecDeque<Result<String, AiError>>>,
    pub prompts: Mutex<Vec<String>>,
}

impl FakeRunner {
    pub fn new(replies: &[&str]) -> Self {
        Self {
            replies: Mutex::new(replies.iter().map(|r| Ok((*r).to_string())).collect()),
            prompts: Mutex::new(Vec::new()),
        }
    }

    pub fn failing() -> Self {
        Self {
            replies: Mutex::new(VecDeque::from([Err(AiError::ProviderNotFound("claude"))])),
            prompts: Mutex::new(Vec::new()),
        }
    }

    pub fn calls(&self) -> usize {
        self.prompts.lock().unwrap().len()
    }

    pub fn prompt(&self, i: usize) -> String {
        self.prompts.lock().unwrap()[i].clone()
    }
}

impl AiRunner for FakeRunner {
    fn run<'a>(
        &'a self,
        req: &'a AiRequest,
        _sink: Option<&'a UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        self.prompts.lock().unwrap().push(req.prompt.clone());
        let next = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(AiError::NoResult));
        Box::pin(async move {
            next.map(|text| AiOutcome {
                text,
                ..AiOutcome::default()
            })
        })
    }
}

/// Packs for homebrew/import checks: core, SRD 5.1 and the SRD 5.1 monsters (2014 rules).
pub fn pack_ctx() -> borealis_assist::PackContext {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut b = ContentStoreBuilder::new();
    for pack in ["core", "srd-5.1", "srd-5.1-monsters"] {
        b = b.add_pack_dir(&root.join(pack)).expect("pack loads");
    }
    borealis_assist::PackContext {
        base: b,
        ruleset: Ruleset::Dnd2014,
    }
}
