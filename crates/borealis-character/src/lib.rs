//! Character model, native JSON files (`borealis-character`) and the `.dnd5e` importer.

pub mod convert;
pub mod crc;
pub mod format;
pub mod import;
pub mod model;

pub use convert::{
    apply_conversion, convert_to_2024, convert_to_2024_with, Category, ConversionItem,
    ConversionPreview, ConvertError, KeyStats, MappingIndex, Outcome,
};
pub use format::{upgrade_with, FormatError, Migration, MIGRATIONS};
pub use import::{
    import_dnd5e, name_hint, recheck, source_hint, ImportError, ImportOutcome, ImportReport,
    MissingContent, RecheckOutcome, UnmappedSlot,
};
pub use model::{
    Appearance, Character, Details, FeatureText, ImportInfo, ManualAttack, Meta, Notes,
    Organization, Personality, Portrait, UnresolvedSelection, FORMAT, FORMAT_VERSION,
};
