//! PF1 (B-235): a [`ContentStoreBuilder`] as compact bytes, so an app can keep the parsed packs and skip
//! reading and parsing every content file on the next launch.
//!
//! Layout: a MessagePack head (packs, indexes, diagnostics, editions, file count), then each pending file
//! as its own MessagePack chunk, every part prefixed with its length (little-endian `u64`). Files decode in
//! parallel and are put back in their original order. The bytes carry no integrity check or cache key:
//! callers wrap them (the app adds a key and a checksum) and must treat any decode error as "no cache".

use super::builder::{ContentStoreBuilder, PendingFile};
use super::{LoadedIndex, PackInfo};
use crate::issue::Issue;
use crate::ruleset::Ruleset;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A snapshot could not be written or read back.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// Encoding failed.
    #[error("snapshot encode: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    /// The bytes do not decode to a builder.
    #[error("snapshot decode: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
    /// The bytes end early or have trailing data.
    #[error("snapshot framing: {0}")]
    Framing(&'static str),
}

#[derive(Serialize, Deserialize)]
struct Head {
    packs: Vec<PackInfo>,
    indexes: Vec<LoadedIndex>,
    diagnostics: Vec<Issue>,
    source_editions: BTreeMap<String, Ruleset>,
    files: u64,
}

fn push_part(out: &mut Vec<u8>, part: &[u8]) {
    out.extend_from_slice(&(part.len() as u64).to_le_bytes());
    out.extend_from_slice(part);
}

fn take_part<'a>(bytes: &mut &'a [u8]) -> Result<&'a [u8], SnapshotError> {
    let (len, rest) = bytes
        .split_first_chunk::<8>()
        .ok_or(SnapshotError::Framing("truncated length"))?;
    let len = usize::try_from(u64::from_le_bytes(*len))
        .map_err(|_| SnapshotError::Framing("length too large"))?;
    if rest.len() < len {
        return Err(SnapshotError::Framing("truncated part"));
    }
    let (part, rest) = rest.split_at(len);
    *bytes = rest;
    Ok(part)
}

impl ContentStoreBuilder {
    /// Encode the builder (see the module docs for the layout).
    ///
    /// # Errors
    /// [`SnapshotError::Encode`] if a value cannot be encoded.
    pub fn to_snapshot(&self) -> Result<Vec<u8>, SnapshotError> {
        let head = Head {
            packs: self.packs.clone(),
            indexes: self.indexes.clone(),
            diagnostics: self.diagnostics.clone(),
            source_editions: self.source_editions.clone(),
            files: self.files.len() as u64,
        };
        let files: Vec<Vec<u8>> = self
            .files
            .par_iter()
            .map(rmp_serde::to_vec_named)
            .collect::<Result<_, _>>()?;
        let head = rmp_serde::to_vec_named(&head)?;
        let total = 8 + head.len() + files.iter().map(|f| 8 + f.len()).sum::<usize>();
        let mut out = Vec::with_capacity(total);
        push_part(&mut out, &head);
        for f in &files {
            push_part(&mut out, f);
        }
        Ok(out)
    }

    /// Decode bytes written by [`to_snapshot`](Self::to_snapshot) back into the same builder.
    ///
    /// # Errors
    /// [`SnapshotError`] for truncated, trailing or undecodable bytes.
    pub fn from_snapshot(mut bytes: &[u8]) -> Result<Self, SnapshotError> {
        let head: Head = rmp_serde::from_slice(take_part(&mut bytes)?)?;
        let count = usize::try_from(head.files)
            .map_err(|_| SnapshotError::Framing("file count too large"))?;
        let mut parts = Vec::with_capacity(count.min(bytes.len() / 8));
        for _ in 0..count {
            parts.push(take_part(&mut bytes)?);
        }
        if !bytes.is_empty() {
            return Err(SnapshotError::Framing("trailing bytes"));
        }
        let files: Vec<PendingFile> = parts
            .par_iter()
            .map(|part| rmp_serde::from_slice::<PendingFile>(part))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            files,
            packs: head.packs,
            indexes: head.indexes,
            diagnostics: head.diagnostics,
            source_editions: head.source_editions,
        })
    }
}
