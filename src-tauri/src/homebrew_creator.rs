//! The homebrew creator's backend (1.5.0; `specs/homebrew-creator-spec.md`). The commands in `commands_homebrew.rs`
//! delegate here. Until each command's work item lands (spec section 21), it answers [`not_implemented`]; the UI
//! shows the creator only behind `AppSettings::creator_enabled` (HC3), so 1.4.x builds never reach it.

use crate::error::{ApiError, ErrorCode};

/// The answer of a creator command whose work item has not landed: `requirement_failed` with
/// `details.notImplemented` = the command name (`docs/ui-api-contract.md`, Homebrew creator).
#[must_use]
pub fn not_implemented(command: &str) -> ApiError {
    let mut e = ApiError::new(
        ErrorCode::RequirementFailed,
        format!("The homebrew creator can't do this yet ({command} is not implemented)."),
    );
    e.details = Some(serde_json::json!({ "notImplemented": command }));
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_implemented_names_the_command_in_details() {
        let e = not_implemented("homebrew_kinds");
        assert_eq!(e.code, ErrorCode::RequirementFailed);
        assert!(e.message.contains("homebrew_kinds"), "{}", e.message);
        assert_eq!(
            e.details,
            Some(serde_json::json!({ "notImplemented": "homebrew_kinds" }))
        );
    }
}
