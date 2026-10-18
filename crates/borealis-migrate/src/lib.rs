//! XML to JSON content migrator (library + `borealis-migrate` CLI).
//!
//! Converts Aurora element files and `.index` manifests the user already owns into the
//! `borealis-content` JSON model. Nothing is redistributed and no network access is made:
//! remote update URLs are only stored as metadata.

pub mod cli;
pub mod convert;
pub mod dom;
pub mod edition;
pub mod error;
pub mod idnorm;
pub mod load_reject;
pub mod pipeline;
pub mod report;

pub use convert::{convert_xml, Document, FileConversion};
pub use error::MigrateError;
pub use pipeline::{migrate_path, write_output, MigrateOptions, MigrateOutput, OutputFile};
pub use report::{FileReport, MigrationReport, Stats};
