//! Matryoshka truncation, L2 normalisation and the degenerate-vector checks.
//!
//! EmbeddingGemma 2 is trained so a prefix of its 768-dim output is itself an embedding. The model card says to
//! **L2-normalise after truncation** (normalising first and then truncating leaves vectors shorter than 1 and
//! breaks dot-product scores). Spike S0.2 compared vectors exactly this way.

use std::fmt;

/// Output sizes the model supports (spec sec. 7): 768 native, Matryoshka prefixes 512 / 256 / 128.
pub const MATRYOSHKA_DIMS: [usize; 4] = [128, 256, 512, 768];

/// Norms at or below this are treated as zero (a real embedding has norm of order 1 before normalising).
pub const MIN_NORM: f32 = 1e-6;

/// Why a vector was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Degeneracy {
    /// A NaN or infinite component (the f16 failure mode of the model card).
    NonFinite,
    /// The truncated vector has (near) zero length, so it cannot be normalised.
    ZeroNorm,
    /// The backend vector is shorter than the requested dimensions.
    TooShort { got: usize, want: usize },
}

impl fmt::Display for Degeneracy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite => f.write_str("NaN or infinite component"),
            Self::ZeroNorm => f.write_str("zero-length vector"),
            Self::TooShort { got, want } => write!(f, "vector has {got} components, need {want}"),
        }
    }
}

/// Keep the first `dims` components of `raw`, then scale to unit L2 length.
///
/// # Errors
/// [`Degeneracy::TooShort`] when `raw` has fewer than `dims` components, [`Degeneracy::NonFinite`] when any
/// component of `raw` (not only of the kept prefix: a NaN anywhere means the forward pass broke) is NaN or
/// infinite, [`Degeneracy::ZeroNorm`] when the prefix has (near) zero length.
pub fn truncate_normalise(raw: &[f32], dims: usize) -> Result<Vec<f32>, Degeneracy> {
    if raw.len() < dims {
        return Err(Degeneracy::TooShort {
            got: raw.len(),
            want: dims,
        });
    }
    if raw.iter().any(|x| !x.is_finite()) {
        return Err(Degeneracy::NonFinite);
    }
    let prefix = &raw[..dims];
    let norm = prefix
        .iter()
        .map(|&x| f64::from(x) * f64::from(x))
        .sum::<f64>()
        .sqrt();
    if !norm.is_finite() {
        return Err(Degeneracy::NonFinite);
    }
    if norm <= f64::from(MIN_NORM) {
        return Err(Degeneracy::ZeroNorm);
    }
    #[allow(clippy::cast_possible_truncation)] // unit-length components fit f32
    Ok(prefix
        .iter()
        .map(|&x| (f64::from(x) / norm) as f32)
        .collect())
}

/// Dot product (= cosine for unit vectors).
#[must_use]
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(v: &[f32]) -> f32 {
        dot(v, v).sqrt()
    }

    #[test]
    fn truncates_then_normalises_to_unit_length() {
        let raw = [3.0, 4.0, 100.0, -50.0];
        let v = truncate_normalise(&raw, 2).expect("ok");
        assert_eq!(v.len(), 2);
        assert!((v[0] - 0.6).abs() < 1e-6, "{v:?}");
        assert!((v[1] - 0.8).abs() < 1e-6, "{v:?}");
        assert!((norm(&v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn normalising_before_truncating_would_differ() {
        // The order matters: normalise(raw)[..2] has length < 1, truncate_normalise has length 1.
        let raw = [3.0_f32, 4.0, 12.0];
        let full = norm(&raw);
        let wrong: Vec<f32> = raw[..2].iter().map(|x| x / full).collect();
        let right = truncate_normalise(&raw, 2).expect("ok");
        assert!(norm(&wrong) < 0.5);
        assert!((norm(&right) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn every_matryoshka_size_gives_a_unit_vector() {
        let raw: Vec<f32> = (0..768u16).map(|i| f32::from(i % 17) - 8.0).collect();
        for dims in MATRYOSHKA_DIMS {
            let v = truncate_normalise(&raw, dims).expect("ok");
            assert_eq!(v.len(), dims);
            assert!((norm(&v) - 1.0).abs() < 1e-5, "{dims}");
        }
    }

    #[test]
    fn nan_anywhere_is_rejected() {
        let mut raw = vec![1.0_f32; 768];
        raw[700] = f32::NAN;
        assert_eq!(truncate_normalise(&raw, 128), Err(Degeneracy::NonFinite));
        raw[700] = f32::INFINITY;
        assert_eq!(truncate_normalise(&raw, 768), Err(Degeneracy::NonFinite));
    }

    #[test]
    fn zero_prefix_is_rejected() {
        let mut raw = vec![0.0_f32; 768];
        raw[300] = 1.0; // outside the 256 prefix
        assert_eq!(truncate_normalise(&raw, 256), Err(Degeneracy::ZeroNorm));
        assert!(truncate_normalise(&raw, 512).is_ok());
    }

    #[test]
    fn short_vectors_are_rejected() {
        assert_eq!(
            truncate_normalise(&[1.0; 10], 128),
            Err(Degeneracy::TooShort { got: 10, want: 128 })
        );
    }
}
