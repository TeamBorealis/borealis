//! Invented homebrew books shaped like the GitHub #11 reporter's content, and an app over a temp
//! data folder (shared by `book_edition.rs` and `homebrew_problems.rs`).
#![allow(dead_code)]

use super::contract_support::Contract;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_lib::content_dto::ImportPackRequest;
use borealis_lib::content_packs::NullSink;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::state::{AppPaths, AppState};
use serde_json::Value;
use std::path::PathBuf;

pub const GRIM: &str = "Grim Test: Player's Guide (2025)";
pub const OLD: &str = "Old Test Homebrew";

/// An invented book (see the module docs). `release` 0 leaves the date out.
pub fn book(source: &str, prefix: &str, class: &str, release: &str, marker: bool) -> String {
    let marker = if marker {
        "<set name=\"information\">D&amp;D 5.5e</set>"
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <info><name>{source}</name><update version="1.0.0"><file name="{prefix}.xml" url="https://example.invalid/{prefix}.xml"/></update></info>
  <element name="{source}" type="Source" source="Core" id="ID_{prefix}_SOURCE">
    <setters><set name="abbreviation">{prefix}</set><set name="url">https://example.invalid</set>
      <set name="third-party">true</set><set name="release">{release}</set>{marker}</setters>
  </element>
  <element name="{class}" type="Class" source="{source}" id="ID_{prefix}_CLASS">
    <supports>{class}</supports>
    <description><p>Invented.</p></description>
    <setters><set name="hd">d10</set><set name="short">Invented.</set></setters>
    <rules>
      <select type="Class Feature" name="Companion Boost" supports="Companion 4: Ability Boost" level="1" />
      <select type="Class Feature" name="Spirit Bond" supports="Spirit Bonded Senryōka" level="1" />
      <select type="Class Feature" name="Spider Gift" supports="Spider's Cunning" level="1" />
      <select type="Class Feature" name="Broken Gift" supports="Broken (Gift" level="1" />
      <grant type="Class Feature" id="ID_{prefix}_CF_ULTIMATE" level="1" />
    </rules>
  </element>
  <element name="Boost Strength" type="Class Feature" source="{source}" id="ID_{prefix}_CF_BOOST_STR">
    <supports>Companion 4: Ability Boost</supports><description><p>Invented.</p></description>
  </element>
  <element name="Boost Dexterity" type="Class Feature" source="{source}" id="ID_{prefix}_CF_BOOST_DEX">
    <supports>Companion 4: Ability Boost</supports><description><p>Invented.</p></description>
  </element>
  <element name="Ethereal Bond" type="Class Feature" source="{source}" id="ID_{prefix}_CF_ETHEREAL">
    <supports>Spirit Bonded Senryōka</supports><description><p>Invented.</p></description>
  </element>
  <element name="Web Walker" type="Class Feature" source="{source}" id="ID_{prefix}_CF_WEB">
    <supports>Spider's Cunning</supports><description><p>Invented.</p></description>
  </element>
  <element name="Ultimate Style" type="Class Feature" source="{source}" id="ID_{prefix}_CF_ULTIMATE">
    <requirements>Ultimate Fighter</requirements><description><p>Invented.</p></description>
  </element>
</elements>"#
    )
}

pub struct App {
    pub state: AppState,
    pub contract: Contract,
    pub data: PathBuf,
    _dir: Option<tempfile::TempDir>,
}

impl App {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let data = dir.path().to_path_buf();
        Self::at(data, Some(dir))
    }

    pub fn at(data: PathBuf, dir: Option<tempfile::TempDir>) -> Self {
        let paths = AppPaths {
            content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data: data.clone(),
        };
        Self {
            state: AppState::new(paths).expect("state"),
            contract: Contract::load(),
            data,
            _dir: dir,
        }
    }

    /// The same data folder, as after an app restart.
    pub fn restart(self) -> Self {
        let Self { data, _dir, .. } = self;
        Self::at(data, _dir)
    }

    pub fn import(&self, file: &str, xml: &str) -> Value {
        let r = self
            .state
            .import_pack(
                &ImportPackRequest {
                    file_name: file.into(),
                    content_base64: STANDARD.encode(xml),
                    format: None,
                },
                &NullSink,
            )
            .expect("import");
        serde_json::to_value(&r.report).expect("json")
    }

    pub fn sources(&self) -> Value {
        let v = serde_json::to_value(self.state.list_sources().expect("sources")).unwrap();
        let errors = self.contract.response("list_sources", &v);
        assert!(errors.is_empty(), "list_sources: {errors:?}");
        v
    }

    pub fn source(&self, name: &str) -> Value {
        self.sources()
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("no source {name}"))
    }

    pub fn hero(&self, ruleset: Ruleset) -> CharacterSnapshot {
        let request = NewCharacterRequest {
            name: "Hero".into(),
            ruleset,
            starting_level: 1,
            gender: String::new(),
            ability_method: AbilityMethodDto::Manual,
            options: OptionsPatch::default(),
            portrait: None,
            group: None,
        };
        self.state
            .create_character(ruleset, &request)
            .expect("create")
    }

    pub fn offered(&self, snap: &CharacterSnapshot, title: &str) -> Vec<String> {
        let slot = snap
            .slots
            .iter()
            .find(|s| s.title == title)
            .unwrap_or_else(|| panic!("no slot {title}"));
        self.state
            .get_slot_options(&snap.id, &slot.id, None, None, None)
            .expect("options")
            .items
            .iter()
            .map(|o| o.base.summary.name.clone())
            .collect()
    }

    pub fn pick_class(&self, snap: &CharacterSnapshot, id: &str) -> CharacterSnapshot {
        let slot = snap.slots.iter().find(|s| s.title == "Class").unwrap();
        self.state
            .apply_selection(&snap.id, &slot.id, id)
            .expect("pick class")
    }

    pub fn checked(&self, command: &str, snap: &CharacterSnapshot) -> Value {
        let v = serde_json::to_value(snap).expect("json");
        let errors = self.contract.response(command, &v);
        assert!(errors.is_empty(), "{command}: {errors:?}");
        v
    }
}
