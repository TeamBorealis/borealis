//! Deterministic safe ids for Aurora ids that Borealis' id rules reject.
//!
//! Shared by the migrator (which rewrites content) and the character importer (which maps
//! ids found in `.dnd5e` files onto the migrated content), so both agree without sharing state.

use crate::validate::ID_FORBIDDEN;

/// True when `id` contains characters that are operators in the expression languages.
#[must_use]
pub fn is_unsafe_id(id: &str) -> bool {
    id.contains(ID_FORBIDDEN)
}

/// `id` with every forbidden character replaced by `_`.
#[must_use]
pub fn safe_id(id: &str) -> String {
    id.chars()
        .map(|c| if ID_FORBIDDEN.contains(&c) { '_' } else { c })
        .collect()
}

/// The collision-proof form: [`safe_id`] plus a stable 32-bit FNV-1a hash of the original.
#[must_use]
pub fn hashed_safe_id(id: &str) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for b in id.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    format!("{}_{h:08x}", safe_id(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_and_hashed_forms_are_stable() {
        assert!(is_unsafe_id("ID_A'S_(B)"));
        assert!(!is_unsafe_id("ID_A"));
        assert_eq!(safe_id("ID_A'S_(B)"), "ID_A_S__B_");
        assert_eq!(hashed_safe_id("x|y"), hashed_safe_id("x|y"));
        assert_ne!(hashed_safe_id("x|y"), hashed_safe_id("x!y"));
    }
}
