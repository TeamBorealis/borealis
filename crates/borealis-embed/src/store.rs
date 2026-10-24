//! Per-corpus vector store (spec sec. 7): `vectors-<model_sha8>-<dims>.bin` plus a `keys.json` sidecar.
//!
//! **Binary file** (little-endian), [`HEADER_BYTES`] of header then `count` rows:
//!
//! | Bytes | Field |
//! |---|---|
//! | 8 | magic [`MAGIC`] |
//! | 4 | format version ([`FORMAT_VERSION`]) |
//! | 32 | model SHA-256 (raw) |
//! | 16 | quantisation label, ASCII, zero padded |
//! | 4 | dims |
//! | 4 | prompt version |
//! | 4 | row count |
//! | 32 | SHA-256 of the `keys.json` bytes (binds the sidecar to this file) |
//! | 32 | SHA-256 of the row bytes (corruption check) |
//!
//! Each row is an `f32` scale followed by `dims` `i8` components: component `i` is `q[i] * scale`, with
//! `scale = max|x| / 127`. Rows are unit vectors before quantisation, so the dot product with an f32 query is
//! `scale * sum(q[i] * query[i])`: exact brute force, no ANN (YAGNI below ~200k chunks).
//!
//! **Sidecar** `keys.json`: `{format, rows: [{key, hash, pack}]}`, row `n` describes binary row `n`. `key` is the
//! search `doc_key`, `hash` the chunk's content hash, `pack` the pack (or campaign) it came from, so a pack can be
//! updated or dropped on its own (spec sec. 9).
//!
//! **Lifecycle.** [`VectorStore::open`] deletes vector files of other models / dims, and wipes and starts empty on
//! any check failure or a quant / prompt-version change ([`OpenStatus::Rebuilt`]): vectors are derived data.
//! [`VectorStore::save`] writes both files to `.tmp` and renames the binary first; a crash between the two
//! renames is recovered from `keys.json.tmp` on the next open.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::embedder::Embedder;
use crate::error::{EmbedError, Result};

/// File magic.
pub const MAGIC: [u8; 8] = *b"BRLSVEC\0";
/// Binary format version.
pub const FORMAT_VERSION: u32 = 1;
/// Sidecar name.
pub const KEYS_FILE: &str = "keys.json";
/// Sidecar format tag.
pub const KEYS_FORMAT: &str = "borealis-vector-keys/1";
/// Header size in bytes.
pub const HEADER_BYTES: usize = 8 + 4 + 32 + QUANT_BYTES + 4 + 4 + 4 + 32 + 32;
const QUANT_BYTES: usize = 16;
const TMP_SUFFIX: &str = ".tmp";
const I8_MAX: f32 = 127.0;

/// What a store file is valid for: vectors rebuild when any field changes (spec sec. 9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreId {
    model_sha256: [u8; 32],
    quant: String,
    dims: usize,
    prompt_version: u32,
}

impl StoreId {
    /// # Errors
    /// [`EmbedError::Corrupt`] for a SHA-256 that is not 64 hex digits or a quant label over 16 ASCII bytes.
    pub fn new(
        model_sha256_hex: &str,
        quant: &str,
        dims: usize,
        prompt_version: u32,
    ) -> Result<Self> {
        let model_sha256 = decode_hex32(model_sha256_hex).ok_or_else(|| {
            EmbedError::Corrupt(format!("bad model SHA-256 {model_sha256_hex:?}"))
        })?;
        if !quant.is_ascii() || quant.len() > QUANT_BYTES || quant.contains('\0') {
            return Err(EmbedError::Corrupt(format!("bad quant label {quant:?}")));
        }
        Ok(Self {
            model_sha256,
            quant: quant.to_string(),
            dims,
            prompt_version,
        })
    }

    /// The id for vectors produced by `embedder`.
    ///
    /// # Errors
    /// As [`Self::new`].
    pub fn for_embedder(embedder: &dyn Embedder) -> Result<Self> {
        let m = embedder.model();
        Self::new(
            &m.sha256,
            &m.quant,
            embedder.dims(),
            embedder.prompt_version(),
        )
    }

    /// `vectors-<model_sha8>-<dims>.bin`.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!(
            "vectors-{}-{}.bin",
            &encode_hex(&self.model_sha256)[..8],
            self.dims
        )
    }

    #[must_use]
    pub fn dims(&self) -> usize {
        self.dims
    }
}

/// How [`VectorStore::open`] found the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenStatus {
    /// Existing vectors loaded.
    Loaded,
    /// No store yet: empty.
    Created,
    /// The files were unusable (or for another quant / prompt version) and were wiped: empty.
    Rebuilt { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct KeyRow {
    key: String,
    hash: String,
    pack: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct KeysFile {
    format: String,
    rows: Vec<KeyRow>,
}

/// Vectors of one corpus, in memory, with exact search.
#[derive(Debug)]
pub struct VectorStore {
    dir: PathBuf,
    id: StoreId,
    keys: Vec<KeyRow>,
    scales: Vec<f32>,
    data: Vec<i8>,
    index: HashMap<String, usize>,
    dirty: bool,
}

impl VectorStore {
    /// Open (or create) the store for `id` in `dir` (one directory per corpus).
    ///
    /// # Errors
    /// I/O errors creating the directory or deleting files. Unreadable or mismatched files are not errors: they
    /// are wiped and reported as [`OpenStatus::Rebuilt`].
    pub fn open(dir: &Path, id: StoreId) -> Result<(Self, OpenStatus)> {
        fs::create_dir_all(dir)?;
        remove_other_vector_files(dir, &id.file_name())?;
        let empty = Self {
            dir: dir.to_path_buf(),
            id,
            keys: Vec::new(),
            scales: Vec::new(),
            data: Vec::new(),
            index: HashMap::new(),
            dirty: false,
        };
        let bin = empty.bin_path();
        if !bin.exists() {
            remove_if_exists(&dir.join(KEYS_FILE))?;
            return Ok((empty, OpenStatus::Created));
        }
        match empty.load() {
            Ok(loaded) => Ok((loaded, OpenStatus::Loaded)),
            Err(e) => {
                let reason = e.to_string();
                tracing::warn!(%reason, "vector store unusable, rebuilding");
                let empty = empty.wiped()?;
                Ok((empty, OpenStatus::Rebuilt { reason }))
            }
        }
    }

    /// Delete every vector file and sidecar in `dir` ("Delete model and semantic indexes", spec sec. 8).
    ///
    /// # Errors
    /// I/O errors other than "not found".
    pub fn delete_all(dir: &Path) -> Result<()> {
        remove_other_vector_files(dir, "")?;
        remove_if_exists(&dir.join(KEYS_FILE))?;
        remove_if_exists(&dir.join(format!("{KEYS_FILE}{TMP_SUFFIX}")))
    }

    #[must_use]
    pub fn id(&self) -> &StoreId {
        &self.id
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Path of the binary file.
    #[must_use]
    pub fn bin_path(&self) -> PathBuf {
        self.dir.join(self.id.file_name())
    }

    /// True when `key` is stored with exactly `hash` (nothing to embed).
    #[must_use]
    pub fn contains(&self, key: &str, hash: &str) -> bool {
        self.index
            .get(key)
            .is_some_and(|&i| self.keys[i].hash == hash)
    }

    /// Packs with at least one row.
    #[must_use]
    pub fn packs(&self) -> BTreeSet<&str> {
        self.keys.iter().map(|k| k.pack.as_str()).collect()
    }

    /// `(key, hash, pack)` of every row, in row order.
    pub fn rows(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.keys
            .iter()
            .map(|k| (k.key.as_str(), k.hash.as_str(), k.pack.as_str()))
    }

    /// Add or replace the vector of `key` (a unit vector of the store's dims).
    ///
    /// # Errors
    /// [`EmbedError::VectorLength`].
    pub fn upsert(&mut self, pack: &str, key: &str, hash: &str, vector: &[f32]) -> Result<()> {
        let dims = self.id.dims;
        if vector.len() != dims {
            return Err(EmbedError::VectorLength {
                got: vector.len(),
                want: dims,
            });
        }
        let (scale, q) = quantise(vector);
        let row = KeyRow {
            key: key.to_string(),
            hash: hash.to_string(),
            pack: pack.to_string(),
        };
        match self.index.get(key) {
            Some(&i) => {
                self.keys[i] = row;
                self.scales[i] = scale;
                self.data[i * dims..(i + 1) * dims].copy_from_slice(&q);
            }
            None => {
                self.index.insert(key.to_string(), self.keys.len());
                self.keys.push(row);
                self.scales.push(scale);
                self.data.extend_from_slice(&q);
            }
        }
        self.dirty = true;
        Ok(())
    }

    /// Drop the row of `key`. Returns whether it existed.
    pub fn remove(&mut self, key: &str) -> bool {
        let Some(i) = self.index.remove(key) else {
            return false;
        };
        let dims = self.id.dims;
        let last = self.keys.len() - 1;
        if i != last {
            self.keys.swap(i, last);
            self.scales.swap(i, last);
            let (head, tail) = self.data.split_at_mut(last * dims);
            head[i * dims..(i + 1) * dims].copy_from_slice(&tail[..dims]);
            self.index.insert(self.keys[i].key.clone(), i);
        }
        self.keys.pop();
        self.scales.pop();
        self.data.truncate(last * dims);
        self.dirty = true;
        true
    }

    /// Drop the rows of `pack` for which `drop(key, hash)` is true. Returns how many went.
    pub fn remove_where(&mut self, pack: &str, mut drop: impl FnMut(&str, &str) -> bool) -> usize {
        let doomed: Vec<String> = self
            .keys
            .iter()
            .filter(|k| k.pack == pack && drop(&k.key, &k.hash))
            .map(|k| k.key.clone())
            .collect();
        doomed.iter().filter(|k| self.remove(k)).count()
    }

    /// Drop every row of `pack` (pack removed or disabled).
    pub fn remove_pack(&mut self, pack: &str) -> usize {
        self.remove_where(pack, |_, _| true)
    }

    /// The `k` best rows by dot product with `query` (a unit vector of the store's dims), best first. Ties keep
    /// row order.
    #[must_use]
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(&str, f32)> {
        let dims = self.id.dims;
        if query.len() != dims || k == 0 || self.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(usize, f32)> = self
            .data
            .chunks_exact(dims)
            .zip(&self.scales)
            .enumerate()
            .map(|(i, (row, &scale))| {
                let dot: f32 = row.iter().zip(query).map(|(&q, &x)| f32::from(q) * x).sum();
                (i, dot * scale)
            })
            .collect();
        let order = |a: &(usize, f32), b: &(usize, f32)| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0));
        if scored.len() > k {
            scored.select_nth_unstable_by(k - 1, order);
            scored.truncate(k);
        }
        scored.sort_unstable_by(order);
        scored
            .into_iter()
            .map(|(i, s)| (self.keys[i].key.as_str(), s))
            .collect()
    }

    /// Write both files if anything changed since the last save (the indexing checkpoint).
    ///
    /// # Errors
    /// I/O errors.
    pub fn save(&mut self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let keys_bytes = serde_json::to_vec(&KeysFile {
            format: KEYS_FORMAT.into(),
            rows: self.keys.clone(),
        })
        .map_err(|e| EmbedError::Corrupt(e.to_string()))?;
        let body = self.body_bytes();
        let mut bin = Vec::with_capacity(HEADER_BYTES + body.len());
        bin.extend_from_slice(&MAGIC);
        bin.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bin.extend_from_slice(&self.id.model_sha256);
        let mut quant = [0u8; QUANT_BYTES];
        quant[..self.id.quant.len()].copy_from_slice(self.id.quant.as_bytes());
        bin.extend_from_slice(&quant);
        bin.extend_from_slice(&u32_of(self.id.dims)?.to_le_bytes());
        bin.extend_from_slice(&self.id.prompt_version.to_le_bytes());
        bin.extend_from_slice(&u32_of(self.keys.len())?.to_le_bytes());
        bin.extend_from_slice(&Sha256::digest(&keys_bytes));
        bin.extend_from_slice(&Sha256::digest(&body));
        bin.extend_from_slice(&body);

        let bin_path = self.bin_path();
        let keys_path = self.dir.join(KEYS_FILE);
        let bin_tmp = tmp_path(&bin_path);
        let keys_tmp = tmp_path(&keys_path);
        write_synced(&bin_tmp, &bin)?;
        write_synced(&keys_tmp, &keys_bytes)?;
        fs::rename(&bin_tmp, &bin_path)?;
        fs::rename(&keys_tmp, &keys_path)?;
        self.dirty = false;
        Ok(())
    }

    fn body_bytes(&self) -> Vec<u8> {
        let dims = self.id.dims;
        let mut body = Vec::with_capacity(self.keys.len() * (4 + dims));
        for (row, scale) in self.data.chunks_exact(dims).zip(&self.scales) {
            body.extend_from_slice(&scale.to_le_bytes());
            body.extend(row.iter().map(|&q| q.to_le_bytes()[0]));
        }
        body
    }

    fn load(&self) -> Result<Self> {
        let bin = fs::read(self.bin_path())?;
        let header = Header::parse(&bin)?;
        self.check_header(&header)?;
        let dims = self.id.dims;
        let body = &bin[HEADER_BYTES..];
        if body.len() != header.count * (4 + dims) {
            return Err(EmbedError::Corrupt(format!(
                "{} row bytes for {} rows of {dims}",
                body.len(),
                header.count
            )));
        }
        if Sha256::digest(body).as_slice() != header.body_sha256 {
            return Err(EmbedError::Corrupt("row digest mismatch".into()));
        }
        let keys = self.read_keys(&header.keys_sha256)?;
        if keys.len() != header.count {
            return Err(EmbedError::Corrupt(format!(
                "{} keys for {} rows",
                keys.len(),
                header.count
            )));
        }
        let mut scales = Vec::with_capacity(header.count);
        let mut data = Vec::with_capacity(header.count * dims);
        for row in body.chunks_exact(4 + dims) {
            scales.push(f32::from_le_bytes([row[0], row[1], row[2], row[3]]));
            data.extend(row[4..].iter().map(|&b| i8::from_le_bytes([b])));
        }
        let mut index = HashMap::with_capacity(keys.len());
        for (i, k) in keys.iter().enumerate() {
            if index.insert(k.key.clone(), i).is_some() {
                return Err(EmbedError::Corrupt(format!("duplicate key {:?}", k.key)));
            }
        }
        Ok(Self {
            dir: self.dir.clone(),
            id: self.id.clone(),
            keys,
            scales,
            data,
            index,
            dirty: false,
        })
    }

    fn check_header(&self, h: &Header) -> Result<()> {
        if h.model_sha256 != self.id.model_sha256 {
            return Err(EmbedError::Corrupt("model SHA-256 differs".into()));
        }
        if h.dims != self.id.dims {
            return Err(EmbedError::Corrupt(format!(
                "dims {} != {}",
                h.dims, self.id.dims
            )));
        }
        if h.quant != self.id.quant {
            return Err(EmbedError::Corrupt(format!(
                "quant {:?} != {:?}",
                h.quant, self.id.quant
            )));
        }
        if h.prompt_version != self.id.prompt_version {
            return Err(EmbedError::Corrupt(format!(
                "prompt version {} != {}",
                h.prompt_version, self.id.prompt_version
            )));
        }
        Ok(())
    }

    /// `keys.json` whose digest matches the header, falling back to a leftover `keys.json.tmp` (crash between
    /// the two renames of [`Self::save`]).
    fn read_keys(&self, digest: &[u8; 32]) -> Result<Vec<KeyRow>> {
        let path = self.dir.join(KEYS_FILE);
        for candidate in [path.clone(), tmp_path(&path)] {
            let Ok(bytes) = fs::read(&candidate) else {
                continue;
            };
            if Sha256::digest(&bytes).as_slice() != digest {
                continue;
            }
            let file: KeysFile = serde_json::from_slice(&bytes)
                .map_err(|e| EmbedError::Corrupt(format!("keys.json: {e}")))?;
            if file.format != KEYS_FORMAT {
                return Err(EmbedError::Corrupt(format!(
                    "keys format {:?}",
                    file.format
                )));
            }
            return Ok(file.rows);
        }
        Err(EmbedError::Corrupt(
            "keys.json missing or does not match".into(),
        ))
    }

    fn wiped(self) -> Result<Self> {
        remove_if_exists(&self.bin_path())?;
        remove_if_exists(&self.dir.join(KEYS_FILE))?;
        remove_if_exists(&tmp_path(&self.dir.join(KEYS_FILE)))?;
        Ok(self)
    }
}

struct Header {
    model_sha256: [u8; 32],
    quant: String,
    dims: usize,
    prompt_version: u32,
    count: usize,
    keys_sha256: [u8; 32],
    body_sha256: [u8; 32],
}

impl Header {
    fn parse(bin: &[u8]) -> Result<Self> {
        if bin.len() < HEADER_BYTES {
            return Err(EmbedError::Corrupt(format!(
                "{} bytes, header needs {HEADER_BYTES}",
                bin.len()
            )));
        }
        if bin[..8] != MAGIC {
            return Err(EmbedError::Corrupt("bad magic".into()));
        }
        let mut at = 8;
        let mut take = |n: usize| {
            let s = &bin[at..at + n];
            at += n;
            s
        };
        let u32_at = |s: &[u8]| u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
        let version = u32_at(take(4));
        if version != FORMAT_VERSION {
            return Err(EmbedError::Corrupt(format!("format version {version}")));
        }
        let model_sha256 = array32(take(32));
        let quant_raw = take(QUANT_BYTES);
        let end = quant_raw
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(QUANT_BYTES);
        let quant = std::str::from_utf8(&quant_raw[..end])
            .map_err(|_| EmbedError::Corrupt("quant label is not UTF-8".into()))?
            .to_string();
        let dims = usize_of(u32_at(take(4)));
        let prompt_version = u32_at(take(4));
        let count = usize_of(u32_at(take(4)));
        let keys_sha256 = array32(take(32));
        let body_sha256 = array32(take(32));
        Ok(Self {
            model_sha256,
            quant,
            dims,
            prompt_version,
            count,
            keys_sha256,
            body_sha256,
        })
    }
}

/// Scale + int8 components of a vector.
fn quantise(v: &[f32]) -> (f32, Vec<i8>) {
    let max = v.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
    if max == 0.0 || !max.is_finite() {
        return (0.0, vec![0; v.len()]);
    }
    let scale = max / I8_MAX;
    #[allow(clippy::cast_possible_truncation)] // clamped to the i8 range
    let q = v
        .iter()
        .map(|x| (x / scale).round().clamp(-I8_MAX, I8_MAX) as i8)
        .collect();
    (scale, q)
}

fn array32(s: &[u8]) -> [u8; 32] {
    let mut a = [0u8; 32];
    a.copy_from_slice(s);
    a
}

fn usize_of(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

fn u32_of(v: usize) -> Result<u32> {
    u32::try_from(v).map_err(|_| EmbedError::Corrupt(format!("{v} does not fit the header")))
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(TMP_SUFFIX);
    PathBuf::from(name)
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut f = fs::File::create(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Delete `vectors-*.bin` (and their `.tmp`) except `keep`.
fn remove_other_vector_files(dir: &Path, keep: &str) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let is_vectors = name.starts_with("vectors-")
            && (name.ends_with(".bin") || name.ends_with(&format!(".bin{TMP_SUFFIX}")));
        if is_vectors && name != keep {
            remove_if_exists(&entry.path())?;
        }
    }
    Ok(())
}

fn decode_hex32(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        let s = std::str::from_utf8(pair).ok()?;
        out[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(out)
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let sha = "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135";
        let raw = decode_hex32(sha).expect("hex");
        assert_eq!(encode_hex(&raw), sha);
        assert!(decode_hex32("zz").is_none());
        assert!(decode_hex32(&"g".repeat(64)).is_none());
    }

    #[test]
    fn file_name_uses_sha8_and_dims() {
        let id = StoreId::new(
            "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135",
            "Q8_0",
            256,
            1,
        )
        .expect("id");
        assert_eq!(id.file_name(), "vectors-2188ac1d-256.bin");
        assert!(StoreId::new("2188", "Q8_0", 256, 1).is_err());
        assert!(StoreId::new(&"a".repeat(64), "a-very-long-quant-label", 256, 1).is_err());
    }

    #[test]
    fn quantisation_error_is_small() {
        let v: Vec<f32> = (0..256u16)
            .map(|i| (f32::from(i) - 128.0) / 1000.0)
            .collect();
        let (scale, q) = quantise(&v);
        for (x, qi) in v.iter().zip(&q) {
            assert!((x - f32::from(*qi) * scale).abs() <= scale / 2.0 + 1e-7);
        }
        assert_eq!(quantise(&[0.0; 4]), (0.0, vec![0; 4]));
    }
}
