//! Size and count caps, shared by the writer (never produce what the reader would refuse) and the
//! reader (never trust the sender).

/// Caps applied to a bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum zip entries (files and directories).
    pub max_entries: usize,
    /// Maximum items (characters plus packs).
    pub max_items: usize,
    /// Maximum uncompressed size of one file.
    pub max_file_bytes: u64,
    /// Maximum uncompressed size of all files together.
    pub max_total_bytes: u64,
    /// Maximum uncompressed size of `manifest.json`.
    pub max_manifest_bytes: u64,
    /// Maximum uncompressed/compressed ratio for entries larger than [`RATIO_FLOOR_BYTES`].
    pub max_ratio: u64,
    /// Maximum path depth inside a pack folder.
    pub max_depth: usize,
}

/// Entries at or below this size are exempt from the ratio check (tiny JSON compresses very well).
pub const RATIO_FLOOR_BYTES: u64 = 1 << 20;

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 20_000,
            max_items: 10_000,
            max_file_bytes: 64 << 20,
            max_total_bytes: 512 << 20,
            max_manifest_bytes: 8 << 20,
            max_ratio: 200,
            max_depth: 8,
        }
    }
}
