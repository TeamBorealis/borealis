//! Paired randomisation test and the adoption rule (spec section 6).
use super::metrics::VariantRun;

pub const PERMUTATIONS: usize = 10_000;
pub const SEED: u64 = 0x5EED_B0EA_115D_0001;
pub const MIN_GAIN: f64 = 0.01;
pub const MAX_P: f64 = 0.05;
pub const MAX_CATEGORY_LOSS: f64 = 0.02;
pub const DESKTOP_P95_MS: f64 = 30.0;

/// SplitMix64: tiny deterministic generator, no dependency.
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Two-sided paired randomisation (sign-flip) test on the mean difference `b - a`.
/// Returns the p-value `(extreme + 1) / (permutations + 1)`.
pub fn paired_randomisation(a: &[f64], b: &[f64], permutations: usize, seed: u64) -> f64 {
    assert_eq!(a.len(), b.len(), "paired samples need equal length");
    let diffs: Vec<f64> = a.iter().zip(b).map(|(x, y)| y - x).collect();
    if diffs.is_empty() {
        return 1.0;
    }
    let observed = diffs.iter().sum::<f64>().abs();
    let mut rng = SplitMix64(seed);
    let mut extreme = 0usize;
    for _ in 0..permutations {
        let mut sum = 0.0;
        let mut bits = 0u64;
        for (i, d) in diffs.iter().enumerate() {
            if i % 64 == 0 {
                bits = rng.next_u64();
            }
            sum += if (bits >> (i % 64)) & 1 == 1 { *d } else { -*d };
        }
        // The tolerance keeps exact ties (identical samples) counting as extreme.
        if sum.abs() >= observed - 1e-12 {
            extreme += 1;
        }
    }
    (extreme + 1) as f64 / (permutations + 1) as f64
}

#[derive(Debug, Clone)]
pub struct Adoption {
    pub base: String,
    pub candidate: String,
    pub delta: f64,
    pub p_value: f64,
    pub worst_category: String,
    pub worst_category_delta: f64,
    pub candidate_p95_ms: f64,
    pub adopted: bool,
}

/// Adoption rule: overall nDCG@10 gain >= 0.01 with p < 0.05, no category loses more than 0.02, and desktop p95
/// under 30 ms. Ties go to the simpler variant, which the caller expresses by passing it as `base`.
pub fn decide(base: &VariantRun, candidate: &VariantRun) -> Adoption {
    let delta = candidate.ndcg10(None) - base.ndcg10(None);
    let p_value = paired_randomisation(
        &base.ndcg_vector(),
        &candidate.ndcg_vector(),
        PERMUTATIONS,
        SEED,
    );
    let (worst_category, worst_category_delta) = base
        .categories()
        .into_iter()
        .map(|c| {
            let d = candidate.ndcg10(Some(&c)) - base.ndcg10(Some(&c));
            (c, d)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or_default();
    let candidate_p95_ms = candidate.latency(None, 0.95);
    let adopted = delta >= MIN_GAIN
        && p_value < MAX_P
        && worst_category_delta >= -MAX_CATEGORY_LOSS
        && candidate_p95_ms < DESKTOP_P95_MS;
    Adoption {
        base: base.name.clone(),
        candidate: candidate.name.clone(),
        delta,
        p_value,
        worst_category,
        worst_category_delta,
        candidate_p95_ms,
        adopted,
    }
}
