//! AI15: Borealis driven end to end without a window, through the CLI entry point and through the
//! MCP write mode (`serve_with` over in-memory streams), with the bundled SRD content. Both run
//! the same character flow; refusals (no write tools without `--allow-write`, paths outside the
//! library, picks the engine rejects) are checked on both fronts.
#![cfg(desktop)]

use borealis_lib::drive::cli;
use borealis_lib::mcp_mode;
use serde_json::{json, Value};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

const SPECIES: &str = "ID_2024_SPECIES_HUMAN";
const CLASS: &str = "ID_2024_CLASS_CLERIC";
const BACKGROUND: &str = "ID_2024_BACKGROUND_ACOLYTE";
const WEAPON: &str = "ID_2024_WEAPON_MACE";
const READ_ONLY_TOOLS: usize = 7;

fn content_dir() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../content")
        .to_string_lossy()
        .into_owned()
}

/// A failed call: the error code (when known) and the message.
#[derive(Debug)]
struct Refusal {
    message: String,
}

/// One front end: call a tool by name with JSON arguments.
trait Front {
    fn call(&mut self, tool: &str, args: Value) -> Result<Value, Refusal>;
}

// ---------------------------------------------------------------- CLI front

struct CliFront {
    library: PathBuf,
}

impl CliFront {
    /// Run a raw command line; returns (exit code, stdout, stderr).
    fn raw(&self, args: &[&str]) -> (i32, String, String) {
        let mut argv: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        argv.extend([
            "--library".into(),
            self.library.to_string_lossy().into_owned(),
            "--content-dir".into(),
            content_dir(),
        ]);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = cli::run_with(
            &argv,
            Path::new("/nonexistent/borealis"),
            &mut out,
            &mut err,
        );
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }
}

impl Front for CliFront {
    fn call(&mut self, tool: &str, args: Value) -> Result<Value, Refusal> {
        let json_args = args.to_string();
        let (code, out, err) = self.raw(&[tool, "--args", &json_args]);
        let value: Value = serde_json::from_str(&out).expect("JSON on stdout");
        if code == 0 {
            return Ok(value);
        }
        assert_eq!(code, cli::EXIT_FAILED, "{tool}: {err}");
        assert!(err.starts_with("borealis: "), "{err}");
        Err(Refusal {
            message: value["error"]["message"].as_str().unwrap().to_string(),
        })
    }
}

// ---------------------------------------------------------------- MCP front

struct ChanReader {
    rx: Receiver<Vec<u8>>,
    buf: Vec<u8>,
    pos: usize,
}

impl Read for ChanReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.buf.len() {
            match self.rx.recv() {
                Ok(b) => {
                    self.buf = b;
                    self.pos = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

struct ChanWriter {
    tx: Sender<Vec<u8>>,
    line: Vec<u8>,
}

impl Write for ChanWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.line.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if !self.line.is_empty() {
            let _ = self.tx.send(std::mem::take(&mut self.line));
        }
        Ok(())
    }
}

struct McpFront {
    to_server: Option<Sender<Vec<u8>>>,
    from_server: Receiver<Vec<u8>>,
    server: Option<std::thread::JoinHandle<Result<(), String>>>,
    next_id: u64,
}

impl McpFront {
    fn start(args: Vec<String>) -> Self {
        let (to_server, rx) = channel();
        let (tx, from_server) = channel();
        let server = std::thread::spawn(move || {
            let reader = BufReader::new(ChanReader {
                rx,
                buf: Vec::new(),
                pos: 0,
            });
            let writer = ChanWriter {
                tx,
                line: Vec::new(),
            };
            mcp_mode::serve_with(&args, Path::new("/nonexistent/borealis"), reader, writer)
        });
        Self {
            to_server: Some(to_server),
            from_server,
            server: Some(server),
            next_id: 0,
        }
    }

    fn write_mode(library: &Path) -> Self {
        Self::start(vec![
            "--mcp".into(),
            "--content-dir".into(),
            content_dir(),
            "--allow-write".into(),
            "--library".into(),
            library.to_string_lossy().into_owned(),
        ])
    }

    fn request(&mut self, method: &str, mut params: Value) -> Value {
        self.next_id += 1;
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {}
        });
        let msg = json!({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params});
        let mut line = msg.to_string().into_bytes();
        line.push(b'\n');
        self.to_server.as_ref().unwrap().send(line).unwrap();
        let reply = self
            .from_server
            .recv_timeout(std::time::Duration::from_secs(300))
            .expect("a response");
        let v: Value = serde_json::from_slice(&reply).unwrap();
        assert_eq!(v["id"], self.next_id);
        v
    }

    fn tool_names(&mut self) -> Vec<String> {
        let r = self.request("tools/list", json!({}));
        r["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    }

    fn stop(mut self) -> Result<(), String> {
        drop(self.to_server.take());
        self.server.take().unwrap().join().unwrap()
    }
}

impl Front for McpFront {
    fn call(&mut self, tool: &str, args: Value) -> Result<Value, Refusal> {
        let r = self.request("tools/call", json!({"name": tool, "arguments": args}));
        let result = &r["result"];
        assert!(r["error"].is_null(), "{tool}: {r}");
        let text = result["content"][0]["text"].as_str().unwrap().to_string();
        if result["isError"] == true {
            return Err(Refusal { message: text });
        }
        Ok(result
            .get("structuredContent")
            .cloned()
            .unwrap_or_else(|| serde_json::from_str(&text).expect("JSON text")))
    }
}

// ---------------------------------------------------------------- the flow

fn slot_id(f: &mut dyn Front, id: &str, titles: &[&str]) -> String {
    let slots = f
        .call("list_slots", json!({"id": id, "open_only": true}))
        .unwrap();
    slots["slots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| titles.contains(&s["title"].as_str().unwrap_or_default()))
        .unwrap_or_else(|| panic!("no open slot {titles:?}: {slots}"))["slotId"]
        .as_str()
        .unwrap()
        .to_string()
}

fn pick_offered(f: &mut dyn Front, id: &str, slot: &str, element: &str, query: &str) {
    let offered = f
        .call(
            "list_options",
            json!({"id": id, "slot_id": slot, "query": query}),
        )
        .unwrap();
    assert!(
        offered["options"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["id"] == element),
        "{element} offered: {offered}"
    );
    f.call(
        "pick",
        json!({"id": id, "slot_id": slot, "element_id": element}),
    )
    .unwrap_or_else(|e| panic!("pick {element}: {e:?}"));
}

/// Create a level-3 human cleric, equip a mace, prepare a spell, save, reopen, export a PDF.
fn full_flow(f: &mut dyn Front, library: &Path) {
    let made = f
        .call(
            "create_character",
            json!({"name": "Drive Test", "ruleset": "2024", "ability_method": "manual"}),
        )
        .unwrap();
    let id = made["id"].as_str().unwrap().to_string();
    assert_eq!(made["level"], 1);

    let race = slot_id(f, &id, &["Race", "Species"]);
    pick_offered(f, &id, &race, SPECIES, "Human");
    let class = slot_id(f, &id, &["Class"]);
    // The engine's own reason comes back for a pick it does not allow.
    let refused = f
        .call(
            "pick",
            json!({"id": id, "slot_id": class, "element_id": SPECIES}),
        )
        .unwrap_err();
    assert!(!refused.message.is_empty());
    pick_offered(f, &id, &class, CLASS, "Cleric");
    let background = slot_id(f, &id, &["Background"]);
    pick_offered(f, &id, &background, BACKGROUND, "Acolyte");

    let scores = json!({"str": 10, "dex": 12, "con": 14, "int": 8, "wis": 15, "cha": 13});
    let set = f
        .call("set_abilities", json!({"id": id, "scores": scores}))
        .unwrap();
    assert_eq!(set["base"]["wis"], 15);

    f.call("level_up", json!({"id": id})).unwrap();
    let up = f
        .call("level_up", json!({"id": id, "class_slot_id": "main"}))
        .unwrap();
    assert_eq!(up["level"], 3, "{up}");

    let found = f
        .call("search_items", json!({"id": id, "query": "Mace"}))
        .unwrap();
    assert!(
        found["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == WEAPON),
        "{found}"
    );
    let added = f
        .call("add_item", json!({"id": id, "item_id": WEAPON}))
        .unwrap();
    let uid = added["inventory"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["itemId"] == WEAPON)
        .expect("mace row")["uid"]
        .as_str()
        .unwrap()
        .to_string();
    let equipped = f.call("equip_item", json!({"id": id, "uid": uid})).unwrap();
    let row = |v: &Value| {
        v["inventory"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["uid"] == uid.as_str())
            .cloned()
            .unwrap()
    };
    assert!(!row(&equipped)["equipped"].is_null(), "{equipped}");

    let spells = f
        .call("get_character", json!({"id": id, "section": "spells"}))
        .unwrap();
    let spell = spells["classes"][0]["spells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["level"] == 1 && s["prepared"] == false && s["alwaysPrepared"] == false)
        .unwrap_or_else(|| panic!("an unprepared level-1 spell: {spells}"))["id"]
        .as_str()
        .unwrap()
        .to_string();
    let prepared = f
        .call("prepare_spell", json!({"id": id, "spell_id": spell}))
        .unwrap();
    let is_prepared = |v: &Value| {
        v["classes"][0]["spells"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == spell.as_str() && s["prepared"] == true)
    };
    assert!(is_prepared(&prepared["spells"]));

    f.call("save_character", json!({"id": id})).unwrap();
    f.call("close_character", json!({"id": id})).unwrap();
    let reopened = f.call("open_character", json!({"id": id})).unwrap();
    assert_eq!(reopened["level"], 3);
    assert!(reopened["abilities"]["wis"].as_i64().unwrap() >= 15);
    let inv = f
        .call("get_character", json!({"id": id, "section": "inventory"}))
        .unwrap();
    assert!(!row(&json!({"inventory": inv}))["equipped"].is_null());
    let spells = f
        .call("get_character", json!({"id": id, "section": "spells"}))
        .unwrap();
    assert!(is_prepared(&spells), "prepared spell kept after reopening");
    let listed = f.call("list_characters", json!({})).unwrap();
    assert_eq!(listed["count"], 1);

    let out = f
        .call(
            "export_sheet",
            json!({"id": id, "path": "sheets/drive-test.pdf"}),
        )
        .unwrap();
    let pdf = std::fs::read(library.join("sheets/drive-test.pdf")).expect("pdf written");
    assert!(pdf.starts_with(b"%PDF"), "a PDF");
    assert_eq!(out["bytes"].as_u64().unwrap() as usize, pdf.len());

    // Paths outside the library are refused.
    let e = f
        .call("export_sheet", json!({"id": id, "path": "../outside.pdf"}))
        .unwrap_err();
    assert!(e.message.contains("outside the library"), "{e:?}");
    assert!(!library.parent().unwrap().join("outside.pdf").exists());
}

#[test]
fn cli_drives_a_character_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library");
    let mut front = CliFront {
        library: library.clone(),
    };
    full_flow(&mut front, &library);
}

#[test]
fn mcp_write_mode_drives_a_character_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library");
    let mut front = McpFront::write_mode(&library);
    let names = front.tool_names();
    assert!(names.len() > READ_ONLY_TOOLS);
    assert_eq!(names[0], "search_content", "read-only tools first");
    for tool in ["create_character", "pick", "level_up", "export_sheet"] {
        assert!(names.iter().any(|n| n == tool), "{tool} listed");
    }
    full_flow(&mut front, &library);
    front.stop().unwrap();
}

#[test]
fn write_tools_are_absent_without_allow_write() {
    let mut front = McpFront::start(vec!["--mcp".into(), "--content-dir".into(), content_dir()]);
    assert_eq!(front.tool_names().len(), READ_ONLY_TOOLS);
    let r = front.request(
        "tools/call",
        json!({"name": "create_character", "arguments": {"name": "x", "ruleset": "2024"}}),
    );
    assert!(r["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Unknown tool"));
    front.stop().unwrap();
}

#[test]
fn write_mode_needs_both_flags() {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().to_string_lossy().into_owned();
    let strs = |a: &[&str]| a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
    assert!(mcp_mode::split_write_args(&strs(&["--mcp", "--allow-write"])).is_err());
    assert!(mcp_mode::split_write_args(&strs(&["--mcp", "--library", &lib])).is_err());
    let (rest, library) =
        mcp_mode::split_write_args(&strs(&["--mcp", "--allow-write", "--library", &lib])).unwrap();
    assert_eq!(rest, strs(&["--mcp"]));
    assert_eq!(library, Some(PathBuf::from(&lib)));
}

#[test]
fn cli_flags_help_and_usage_errors() {
    let dir = tempfile::tempdir().unwrap();
    let front = CliFront {
        library: dir.path().join("library"),
    };
    let strs = |a: &[&str]| a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
    assert!(cli::requested(&strs(&["help"])));
    assert!(cli::requested(&strs(&["create-character"])));
    assert!(!cli::requested(&strs(&["--mcp"])));
    assert!(!cli::requested(&strs(&["/some/file.json"])));

    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(
        cli::run_with(&strs(&["help"]), Path::new("/x"), &mut out, &mut err),
        0
    );
    let help = String::from_utf8(out).unwrap();
    for name in [
        "create_character",
        "list_options",
        "export_sheet",
        "import_5etools",
    ] {
        assert!(help.contains(name), "help lists {name}");
    }

    // Typed flags, kebab-case, the human form.
    let (code, out, _) = front.raw(&[
        "create-character",
        "--name",
        "Flags",
        "--ruleset",
        "2014",
        "--level",
        "2",
        "--feats",
    ]);
    assert_eq!(code, 0, "{out}");
    let made: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(made["level"], 2);
    let id = made["id"].as_str().unwrap();
    let (code, out, _) = front.raw(&[
        "set-abilities",
        "--id",
        id,
        "--scores",
        "str=12,dex=13",
        "--human",
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("Flags"), "{out}");

    // Usage errors exit 2; refusals exit 1.
    let (code, _, err) = front.raw(&["list-slots", "--id", id, "--bogus", "1"]);
    assert_eq!(code, cli::EXIT_USAGE, "{err}");
    let (code, _, _) = front.raw(&["level-up", "--id", id, "--class-slot-id", "nope"]);
    assert_eq!(code, cli::EXIT_FAILED);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = cli::run_with(
        &strs(&["list-characters"]),
        Path::new("/x"),
        &mut out,
        &mut err,
    );
    assert_eq!(code, cli::EXIT_USAGE, "--library is required");
}

const TEST_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <info><name>Drive Book</name>
    <update version="1.0.0"><file name="drive-book.xml" url="https://example.invalid/drive-book.xml"/></update>
  </info>
  <element name="Drive Gift" type="Feat" source="Drive Book" id="ID_DRIVE_FEAT_GIFT">
    <description><p>A hand-written feat for the drive tests.</p></description>
    <setters><set name="category">General</set></setters>
  </element>
</elements>"#;

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn content_imports_run_offline_from_inside_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("library");
    let mut front = CliFront {
        library: library.clone(),
    };
    let count = |front: &mut CliFront| {
        front.call("list_packs", json!({})).unwrap()["count"]
            .as_u64()
            .unwrap()
    };
    let before = count(&mut front);

    std::fs::write(library.join("drive-book.xml"), TEST_XML).unwrap();
    let imported = front
        .call("import_content_file", json!({"path": "drive-book.xml"}))
        .unwrap();
    assert_eq!(imported["converted"], 1, "{imported}");

    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/borealis-import-5etools/tests/fixtures/data");
    copy_tree(&fixture, &library.join("5e/data"));
    let scanned = front
        .call("scan_5etools", json!({"path": "5e/data"}))
        .unwrap();
    assert!(!scanned["sources"].as_array().unwrap().is_empty());
    let report = front
        .call("import_5etools", json!({"path": "5e/data"}))
        .unwrap();
    assert!(!report["packs"].as_array().unwrap().is_empty(), "{report}");
    let after = count(&mut front);
    assert!(after > before + 1, "{before} -> {after}");

    // Sources outside the library are refused before anything is read.
    let outside = dir.path().join("outside.xml");
    std::fs::write(&outside, TEST_XML).unwrap();
    let e = front
        .call(
            "import_content_file",
            json!({"path": outside.to_string_lossy()}),
        )
        .unwrap_err();
    assert!(e.message.contains("outside the library"), "{e:?}");
    let e = front
        .call("scan_5etools", json!({"path": fixture.to_string_lossy()}))
        .unwrap_err();
    assert!(e.message.contains("outside the library"), "{e:?}");
}
