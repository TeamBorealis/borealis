//! The drive tools as an MCP extension (`borealis --mcp --allow-write --library <dir>`): the
//! read-only server registers them next to its own tools; calls go to [`Drive::call`].

use super::{specs, Drive};
use borealis_mcp::ToolExtension;
use serde_json::Value;

const INSTRUCTIONS: &str = "Write mode: character tools over one library folder. Typical flow: create_character, list_slots (open_only), list_options for a slot, pick, set_abilities, level_up, search_items/list_base_items (generic +N weapons and armor)/add_item/equip_item, prepare_spell, get_character, export_sheet. Every change is validated by the engine and saved; a refused change explains why and changes nothing.";

/// The drive tools for the MCP server.
pub struct DriveTools {
    drive: Drive,
    names: Vec<String>,
}

impl DriveTools {
    #[must_use]
    pub fn new(drive: Drive) -> Self {
        Self {
            drive,
            names: specs::names(),
        }
    }

    /// The drive behind the tools.
    #[must_use]
    pub fn drive(&self) -> &Drive {
        &self.drive
    }
}

impl ToolExtension for DriveTools {
    fn list(&self) -> Vec<Value> {
        specs::all()
    }

    fn call(&self, name: &str, args: &Value) -> Option<Result<Value, String>> {
        if !self.names.iter().any(|n| n == name) {
            return None;
        }
        Some(self.drive.call(name, args).map_err(|e| e.to_string()))
    }

    fn instructions(&self) -> Option<&str> {
        Some(INSTRUCTIONS)
    }
}
