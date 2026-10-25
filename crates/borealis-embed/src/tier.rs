//! Device tiers T0-T3 (spec sec. 7) and the startup probe.
//!
//! | Tier | Devices | File | Dims | Threads | Index while |
//! |---|---|---|---|---|---|
//! | T0 | < 3 GB RAM, or a 32-bit build (armv7/i686 Android) | none (semantic hidden) | - | - | - |
//! | T1 | phones 3-4 GB | Q6_K-te-Q4_K (211 MB) | 256 | 2 | charging only |
//! | T2 | phones/tablets > 4 GB, desktops < 8 GB | Q8_0 (310 MB) | 256 | min(4, cores/2) | charging, or battery >= 50% if allowed |
//! | T3 | desktops >= 8 GB | Q8_0 | 256 | min(8, cores/2) | always (low priority) |
//!
//! **Dims from the V4 eval (S3.3, `specs/search-eval-1.md`):** the smallest setting within 0.01 nDCG@10 of
//! Q8_0 / 768 is 256 for both files; 128 loses more than 0.01 and fails the category clause (prefix), so T1 was
//! raised from 128 and T3 lowered from 512. Every semantic tier also carries the hybrid latency budget
//! ([`QUERY_BUDGET_MS`]), checked on the device after the model loads.
//!
//! **RAM is compared in nominal GiB**: the OS reports less than the RAM on the box (Android's
//! `ActivityManager.MemoryInfo.totalMem` leaves out the kernel and carve-outs, so a 4 GB phone reports about
//! 3.6 GiB and a 3 GB phone about 2.7 GiB). [`nominal_half_gib`] rounds the reported total **up** to the next
//! half GiB before comparing. RAM that cannot be read falls back conservatively: T0 on mobile, T2 on desktop.
//! The user may override the tier **down** only ([`effective_tier`]).

use serde::{Deserialize, Serialize};

use crate::models::{ModelFile, Q6_K_TE_Q4_K, Q8_0};
use crate::scheduler::IndexPolicy;

const HALF_GIB: u64 = 512 * 1024 * 1024;
/// Below 3 GiB: T0.
const T0_BELOW_HALVES: u64 = 6;
/// Mobile up to 4 GiB: T1; above: T2.
const T1_UP_TO_HALVES: u64 = 8;
/// Desktop from 8 GiB: T3.
const T3_FROM_HALVES: u64 = 16;
/// Document chunk cap in tokens on T2 / T3 (spec sec. 3).
pub const MAX_TOKENS: usize = 512;
/// Document chunk cap in tokens on T1 (spec sec. 3).
pub const T1_MAX_TOKENS: usize = 256;
/// The hybrid may add at most 40 ms to a keyword query (spec sec. 6). The query embedding is nearly all of it
/// (S3.3: vector search and fusion take 1-4 ms), so the app times it on the device after loading the model and
/// stays keyword-only when the median is above this.
pub const QUERY_BUDGET_MS: u32 = 40;

/// llama.cpp in this build uses only the x86_64 baseline SIMD (SSE2), so no x86_64 CPU can hit an illegal
/// instruction: `.cargo/config.toml` turns ggml's x86 options (SSE4.2, AVX, AVX2, BMI2, FMA, F16C) off, which ggml
/// would otherwise turn on for every native build with `GGML_NATIVE=OFF`; llama-cpp-sys-2 turns one back on only
/// when Rust's target features include it (`build/check-x86-baseline.sh` checks the libraries). The quantised dot
/// products then run without AVX, several times slower than NEON on arm64: such builds get the T2 row at most, and
/// the measured query-speed check ([`TierConfig::query_budget_ms`]) decides per machine. Runtime CPU dispatch
/// (`GGML_CPU_ALL_VARIANTS`) would need ggml's backend DLLs / shared objects shipped with the installers, which is
/// not done yet (S3.3 handoff).
pub const PORTABLE_X86: bool = cfg!(all(target_arch = "x86_64", not(target_feature = "avx2")));

/// Battery level T2 may index on, when the user allows it.
pub const T2_MIN_BATTERY_PERCENT: u8 = 50;

/// Device tier. Ordered: `T0 < T1 < T2 < T3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    T0,
    T1,
    T2,
    T3,
}

/// `settings.search.tierOverride` (S1.5 handoff): `auto` or a tier, applied downward only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TierOverride {
    #[default]
    Auto,
    T1,
    T2,
    T3,
}

/// Phone/tablet or desktop (sets the RAM thresholds and which pause rules can apply).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormFactor {
    Desktop,
    Mobile,
}

impl FormFactor {
    /// The form factor of the build target.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(any(target_os = "android", target_os = "ios")) {
            Self::Mobile
        } else {
            Self::Desktop
        }
    }
}

/// What the tier is decided from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceProfile {
    pub form: FormFactor,
    /// Total physical RAM as the OS reports it; `None` when it could not be read.
    pub total_ram_bytes: Option<u64>,
    /// Logical cores (`available_parallelism`).
    pub cores: usize,
    /// Pointer width of the build (32 on armv7/i686 Android).
    pub pointer_bits: u32,
}

impl DeviceProfile {
    /// Probe this device: RAM via `sysinfo` (desktop), `ProcessInfo.physicalMemory` (iOS) or `/proc/meminfo`
    /// (Android fallback; the app replaces it with `ActivityManager.MemoryInfo.totalMem` from its Kotlin plugin
    /// through [`Self::with_ram`]).
    #[must_use]
    pub fn probe() -> Self {
        Self {
            form: FormFactor::current(),
            total_ram_bytes: crate::device::total_ram_bytes(),
            cores: std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
            pointer_bits: usize::BITS,
        }
    }

    /// The same profile with a better RAM figure (from a platform shim).
    #[must_use]
    pub fn with_ram(self, total_ram_bytes: Option<u64>) -> Self {
        Self {
            total_ram_bytes: total_ram_bytes.or(self.total_ram_bytes),
            ..self
        }
    }

    /// The probed tier.
    #[must_use]
    pub fn tier(&self) -> Tier {
        classify(self)
    }
}

/// Reported bytes rounded up to whole half-GiB units.
#[must_use]
pub fn nominal_half_gib(bytes: u64) -> u64 {
    bytes.div_ceil(HALF_GIB)
}

/// The tier table.
#[must_use]
pub fn classify(profile: &DeviceProfile) -> Tier {
    if profile.pointer_bits < 64 {
        return Tier::T0;
    }
    let Some(bytes) = profile.total_ram_bytes else {
        return match profile.form {
            FormFactor::Mobile => Tier::T0,
            FormFactor::Desktop => Tier::T2,
        };
    };
    let halves = nominal_half_gib(bytes);
    if halves < T0_BELOW_HALVES {
        return Tier::T0;
    }
    match profile.form {
        FormFactor::Mobile if halves <= T1_UP_TO_HALVES => Tier::T1,
        FormFactor::Mobile => Tier::T2,
        FormFactor::Desktop if halves >= T3_FROM_HALVES => Tier::T3,
        FormFactor::Desktop => Tier::T2,
    }
}

/// Apply the user's override, which can only lower the tier.
#[must_use]
pub fn effective_tier(probed: Tier, user: TierOverride) -> Tier {
    let wanted = match user {
        TierOverride::Auto => return probed,
        TierOverride::T1 => Tier::T1,
        TierOverride::T2 => Tier::T2,
        TierOverride::T3 => Tier::T3,
    };
    probed.min(wanted)
}

/// Everything a tier fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TierConfig {
    pub tier: Tier,
    /// Weights file; `None` on T0 (semantic option hidden).
    pub model: Option<&'static ModelFile>,
    /// Matryoshka output dimensions; `None` on T0.
    pub dims: Option<usize>,
    /// Embedding threads; 0 on T0.
    pub threads: usize,
    /// Chunk cap in tokens for document embeddings (spec sec. 3: 512, 256 on T1); 0 on T0. Also the backend's
    /// context size, which bounds its compute buffers (RSS).
    pub max_tokens: usize,
    /// When background indexing may run.
    pub policy: IndexPolicy,
    /// When set, the app times the backend's query embedding right after loading it and keeps search
    /// keyword-only if the median is above this many ms ([`QUERY_BUDGET_MS`] on every semantic tier).
    pub query_budget_ms: Option<u32>,
}

impl Tier {
    /// The tier's row of the table, with threads from `cores`.
    #[must_use]
    pub fn config(self, cores: usize) -> TierConfig {
        let half = (cores / 2).max(1);
        match self {
            Self::T0 => TierConfig {
                tier: self,
                model: None,
                dims: None,
                threads: 0,
                max_tokens: 0,
                policy: IndexPolicy::Never,
                query_budget_ms: None,
            },
            Self::T1 => TierConfig {
                tier: self,
                model: Some(&Q6_K_TE_Q4_K),
                dims: Some(256),
                threads: 2,
                max_tokens: T1_MAX_TOKENS,
                policy: IndexPolicy::ChargingOnly,
                query_budget_ms: Some(QUERY_BUDGET_MS),
            },
            Self::T2 => TierConfig {
                tier: self,
                model: Some(&Q8_0),
                dims: Some(256),
                threads: half.min(4),
                max_tokens: MAX_TOKENS,
                policy: IndexPolicy::ChargingOrBattery {
                    min_percent: T2_MIN_BATTERY_PERCENT,
                },
                query_budget_ms: Some(QUERY_BUDGET_MS),
            },
            Self::T3 => TierConfig {
                tier: self,
                model: Some(&Q8_0),
                dims: Some(256),
                threads: half.min(8),
                max_tokens: MAX_TOKENS,
                policy: IndexPolicy::Always,
                query_budget_ms: Some(QUERY_BUDGET_MS),
            },
        }
    }

    /// The tier's row for this device (threads from its cores). On a [`PORTABLE_X86`] build T3 becomes T2.
    #[must_use]
    pub fn config_for(self, profile: &DeviceProfile) -> TierConfig {
        portable_x86_row(self.config(profile.cores), PORTABLE_X86)
    }

    /// Whether Settings offers semantic search at all.
    #[must_use]
    pub fn semantic_available(self) -> bool {
        self != Self::T0
    }

    /// Settings explanation when semantic search is hidden (T0).
    #[must_use]
    pub fn unavailable_reason(profile: &DeviceProfile) -> Option<&'static str> {
        match classify(profile) {
            Tier::T0 if profile.pointer_bits < 64 => {
                Some("Semantic search needs a 64-bit device. Keyword search works fully.")
            }
            Tier::T0 => {
                Some("Semantic search needs at least 3 GB of memory. Keyword search works fully.")
            }
            _ => None,
        }
    }
}

/// The [`PORTABLE_X86`] adjustment (a parameter so tests run it on every host): a T3 row keeps its threads and
/// power rule but takes T2's file, dims and chunk cap.
fn portable_x86_row(row: TierConfig, portable_x86: bool) -> TierConfig {
    if !portable_x86 || row.tier != Tier::T3 {
        return row;
    }
    let t2 = Tier::T2.config(1);
    TierConfig {
        tier: Tier::T2,
        model: t2.model,
        dims: t2.dims,
        max_tokens: t2.max_tokens,
        ..row
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn mobile(ram: f64) -> DeviceProfile {
        profile(FormFactor::Mobile, ram)
    }

    fn desktop(ram: f64) -> DeviceProfile {
        profile(FormFactor::Desktop, ram)
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn profile(form: FormFactor, ram_gib: f64) -> DeviceProfile {
        DeviceProfile {
            form,
            total_ram_bytes: Some((ram_gib * GIB as f64) as u64),
            cores: 8,
            pointer_bits: 64,
        }
    }

    #[test]
    fn phones_by_reported_ram() {
        // (reported GiB, tier): a "2 GB" phone reports ~1.8, "3 GB" ~2.7, "4 GB" ~3.6, "6 GB" ~5.5.
        for (ram, want) in [
            (1.8, Tier::T0),
            (2.4, Tier::T0),
            (2.7, Tier::T1),
            (3.0, Tier::T1),
            (3.6, Tier::T1),
            (4.0, Tier::T1),
            (4.2, Tier::T2),
            (5.5, Tier::T2),
            (12.0, Tier::T2),
        ] {
            assert_eq!(mobile(ram).tier(), want, "{ram} GiB phone");
        }
    }

    #[test]
    fn desktops_by_ram() {
        for (ram, want) in [
            (2.0, Tier::T0),
            (4.0, Tier::T2),
            (7.4, Tier::T2),
            (7.8, Tier::T3), // an "8 GB" machine reporting a little less
            (8.0, Tier::T3),
            (64.0, Tier::T3),
        ] {
            assert_eq!(desktop(ram).tier(), want, "{ram} GiB desktop");
        }
    }

    #[test]
    fn thirty_two_bit_is_t0_whatever_the_ram() {
        let p = DeviceProfile {
            pointer_bits: 32,
            ..mobile(8.0)
        };
        assert_eq!(p.tier(), Tier::T0);
        assert!(Tier::unavailable_reason(&p)
            .expect("reason")
            .contains("64-bit"));
        assert!(Tier::unavailable_reason(&mobile(2.0))
            .expect("reason")
            .contains("3 GB"));
        assert!(Tier::unavailable_reason(&mobile(6.0)).is_none());
    }

    #[test]
    fn unknown_ram_is_conservative() {
        let m = DeviceProfile {
            total_ram_bytes: None,
            ..mobile(8.0)
        };
        let d = DeviceProfile {
            total_ram_bytes: None,
            ..desktop(32.0)
        };
        assert_eq!(m.tier(), Tier::T0);
        assert_eq!(d.tier(), Tier::T2);
        assert_eq!(m.with_ram(Some(6 * GIB)).tier(), Tier::T2);
        assert_eq!(mobile(6.0).with_ram(None).tier(), Tier::T2);
    }

    #[test]
    fn override_lowers_but_never_raises() {
        assert_eq!(effective_tier(Tier::T3, TierOverride::Auto), Tier::T3);
        assert_eq!(effective_tier(Tier::T3, TierOverride::T1), Tier::T1);
        assert_eq!(effective_tier(Tier::T2, TierOverride::T3), Tier::T2);
        assert_eq!(effective_tier(Tier::T1, TierOverride::T2), Tier::T1);
        assert_eq!(effective_tier(Tier::T0, TierOverride::T3), Tier::T0);
        assert_eq!(effective_tier(Tier::T0, TierOverride::T1), Tier::T0);
    }

    #[test]
    fn override_parses_the_settings_strings() {
        let t: TierOverride = serde_json::from_str("\"t2\"").expect("t2");
        assert_eq!(t, TierOverride::T2);
        let a: TierOverride = serde_json::from_str("\"auto\"").expect("auto");
        assert_eq!(a, TierOverride::Auto);
        assert!(serde_json::from_str::<TierOverride>("\"t0\"").is_err());
    }

    #[test]
    fn tier_rows() {
        let t0 = Tier::T0.config(8);
        assert_eq!((t0.model, t0.dims, t0.threads), (None, None, 0));
        assert_eq!(t0.policy, IndexPolicy::Never);
        assert!(!Tier::T0.semantic_available());

        let t1 = Tier::T1.config(8);
        assert_eq!(t1.model.expect("file").quant, "Q6_K-te-Q4_K");
        assert_eq!(t1.model.expect("file").bytes, 211_161_312);
        assert_eq!((t1.dims, t1.threads), (Some(256), 2));
        assert_eq!(t1.policy, IndexPolicy::ChargingOnly);

        let t2 = Tier::T2.config(12);
        assert_eq!(t2.model.expect("file").quant, "Q8_0");
        assert_eq!((t2.dims, t2.threads), (Some(256), 4));
        assert_eq!(
            t2.policy,
            IndexPolicy::ChargingOrBattery { min_percent: 50 }
        );
        assert_eq!(Tier::T2.config(6).threads, 3);
        assert_eq!(Tier::T2.config(1).threads, 1);

        let t3 = Tier::T3.config(32);
        assert_eq!(t3.model.expect("file").bytes, 309_855_456);
        assert_eq!((t3.dims, t3.threads), (Some(256), 8));
        assert_eq!(t3.policy, IndexPolicy::Always);
        assert_eq!(Tier::T3.config(10).threads, 5);
    }

    #[test]
    fn probe_reads_this_machine() {
        let p = DeviceProfile::probe();
        assert_eq!(p.form, FormFactor::Desktop);
        assert!(p.cores >= 1);
        assert_eq!(p.pointer_bits, 64);
        assert!(p.total_ram_bytes.is_some_and(|b| b > GIB), "{p:?}");
    }

    #[test]
    fn portable_x86_builds_cap_t3_and_every_semantic_tier_has_the_query_budget() {
        let t3 = Tier::T3.config(16);
        let row = portable_x86_row(t3, true);
        assert_eq!(row.tier, Tier::T2);
        assert_eq!(row.dims, Tier::T2.config(16).dims);
        assert_eq!(row.threads, t3.threads, "desktop threads kept");
        assert_eq!(row.policy, IndexPolicy::Always);
        assert_eq!(
            portable_x86_row(Tier::T1.config(8), true),
            Tier::T1.config(8)
        );
        assert_eq!(
            portable_x86_row(Tier::T0.config(8), true),
            Tier::T0.config(8)
        );
        assert_eq!(portable_x86_row(t3, false), t3, "other builds unchanged");
        for tier in [Tier::T1, Tier::T2, Tier::T3] {
            assert_eq!(tier.config(8).query_budget_ms, Some(QUERY_BUDGET_MS));
        }
        assert_eq!(Tier::T0.config(8).query_budget_ms, None);
    }
}
