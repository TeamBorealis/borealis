//! CRC-32 (IEEE 802.3, reflected, polynomial 0xEDB88320) used by the legacy slot checksum.

const fn table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

static TABLE: [u32; 256] = table();

/// CRC-32 of the bytes.
#[must_use]
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for b in bytes {
        c = TABLE[((c ^ u32::from(*b)) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

/// The legacy slot checksum: lower-case hex of the CRC-32 of
/// `ownerId + ruleName + ruleType + requiredLevel + slotNumber [+ supports for spell rules]`.
#[must_use]
pub fn slot_checksum(
    owner: &str,
    name: &str,
    ty: &str,
    level: u32,
    number: u32,
    supports: Option<&str>,
) -> String {
    let mut input = format!("{owner}{name}{ty}{level}{number}");
    if ty == "Spell" {
        if let Some(s) = supports {
            input.push_str(s);
        }
    }
    format!("{:08x}", crc32(input.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_known_vectors() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(
            slot_checksum("ID_LEVEL_1", "Race", "Race", 1, 1, None),
            "1597ef76"
        );
        assert_eq!(
            slot_checksum("ID_LEVEL_1", "Class", "Class", 1, 1, None),
            "33a7b15b"
        );
        assert_eq!(
            slot_checksum(
                "ID_WOTC_PHB_CLASS_FIGHTER",
                "Skill Proficiency (Fighter)",
                "Proficiency",
                1,
                2,
                None
            ),
            "c6352b30"
        );
        assert_eq!(
            slot_checksum(
                "ID_LEVEL_3",
                "Multiclass (Level 3)",
                "Multiclass",
                3,
                1,
                None
            ),
            "3c1b6721"
        );
    }

    #[test]
    fn supports_only_count_for_spells() {
        let a = slot_checksum("O", "N", "Spell", 1, 1, Some("Wizard,0"));
        let b = slot_checksum("O", "N", "Spell", 1, 1, None);
        assert_ne!(a, b);
        assert_eq!(
            slot_checksum("O", "N", "Item", 1, 1, Some("x")),
            slot_checksum("O", "N", "Item", 1, 1, None)
        );
    }
}
