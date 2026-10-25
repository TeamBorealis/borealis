//! When background indexing may run (spec sec. 7) and the checkpointed, resumable indexing job (sec. 9).
//!
//! Pause rules, checked before every batch, in this order:
//! 1. tier T0 ([`IndexPolicy::Never`]) or semantic search off in Settings, or the NaN guard tripped;
//! 2. app in the background, Low Power Mode / Battery Saver, thermal state serious or worse (mobile signals; the
//!    desktop provider never reports them);
//! 3. the tier's power rule: T1 charging only; T2 charging, or battery >= 50% when the user allows indexing on
//!    battery (`settings.search.indexOnBattery`); T3 always.
//!
//! Query-time embedding (one short string) is not scheduled: it runs at any time.
//!
//! **Checkpoints.** The [`VectorStore`] is the checkpoint: [`IndexJob::run`] saves it on every pause, cancel and
//! completion and at least every [`RunOptions::checkpoint_every`] while running. After a restart, planning again
//! queues only chunks whose `(doc_key, content_hash)` is not stored yet, so indexing resumes where it stopped.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::device::{DeviceState, DeviceStateProvider, Power, Thermal};
use crate::embedder::{DisabledReport, DocText, GuardedEmbedder, SemanticGuard};
use crate::error::{EmbedError, Result};
use crate::store::VectorStore;

/// The tier's power rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexPolicy {
    /// T0: no semantic search.
    Never,
    /// T1.
    ChargingOnly,
    /// T2: charging, or on battery at `min_percent` or more when the user allows it.
    ChargingOrBattery { min_percent: u8 },
    /// T3 (desktop, low-priority thread).
    Always,
}

/// Why indexing is paused (shown as the indexing status in Settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    /// Tier T0.
    Unsupported,
    /// `settings.search.semantic` is off.
    SemanticOff,
    /// The NaN / zero-norm guard turned semantic mode off.
    GuardTripped,
    Backgrounded,
    LowPowerMode,
    /// Thermal state serious or critical.
    Thermal,
    /// Needs external power.
    NotCharging,
    /// On battery below the tier's minimum (or level unknown).
    BatteryLow,
}

/// Verdict for the next batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Run,
    Pause(PauseReason),
}

/// The settings the rules read (`settings.search`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerSettings {
    pub semantic: bool,
    pub index_on_battery: bool,
}

/// The pause rules as a pure function.
#[must_use]
pub fn decide(policy: IndexPolicy, settings: SchedulerSettings, state: &DeviceState) -> Decision {
    use PauseReason as R;
    if policy == IndexPolicy::Never {
        return Decision::Pause(R::Unsupported);
    }
    if !settings.semantic {
        return Decision::Pause(R::SemanticOff);
    }
    if state.backgrounded {
        return Decision::Pause(R::Backgrounded);
    }
    if state.low_power_mode {
        return Decision::Pause(R::LowPowerMode);
    }
    if state.thermal >= Thermal::Serious {
        return Decision::Pause(R::Thermal);
    }
    let charging = state.power == Power::External;
    match policy {
        IndexPolicy::Always => Decision::Run,
        _ if charging => Decision::Run,
        IndexPolicy::ChargingOrBattery { min_percent } if settings.index_on_battery => {
            match state.power {
                Power::Battery { percent: Some(p) } if p >= min_percent => Decision::Run,
                _ => Decision::Pause(R::BatteryLow),
            }
        }
        _ => Decision::Pause(R::NotCharging),
    }
}

/// Pause rules bound to a device: what [`IndexJob::run`] asks before each batch.
#[derive(Clone)]
pub struct Scheduler {
    pub policy: IndexPolicy,
    pub settings: SchedulerSettings,
    provider: Arc<dyn DeviceStateProvider>,
    guard: SemanticGuard,
}

impl Scheduler {
    #[must_use]
    pub fn new(
        policy: IndexPolicy,
        settings: SchedulerSettings,
        provider: Arc<dyn DeviceStateProvider>,
        guard: SemanticGuard,
    ) -> Self {
        Self {
            policy,
            settings,
            provider,
            guard,
        }
    }

    /// The verdict for right now.
    #[must_use]
    pub fn decide(&self) -> Decision {
        if !self.guard.is_enabled() {
            return Decision::Pause(PauseReason::GuardTripped);
        }
        decide(self.policy, self.settings, &self.provider.current())
    }
}

impl std::fmt::Debug for Scheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scheduler")
            .field("policy", &self.policy)
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

/// One chunk that should have a vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexItem {
    /// Pack id (or campaign id for Sessions).
    pub pack: String,
    /// Search `doc_key`.
    pub doc_key: String,
    /// Content hash of the chunk text (a changed hash re-embeds).
    pub content_hash: String,
    pub title: Option<String>,
    pub text: String,
}

/// Batch size and checkpoint interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunOptions {
    /// Chunks per embedding call (between two pause checks).
    pub batch: usize,
    /// Longest time between two checkpoints while running.
    pub checkpoint_every: Duration,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            batch: 8,
            checkpoint_every: Duration::from_secs(30),
        }
    }
}

/// How a run ended. Every outcome leaves a saved checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    /// Nothing left to embed.
    Completed { embedded: usize },
    /// A pause rule fired; run again once it clears.
    Paused {
        reason: PauseReason,
        embedded: usize,
        remaining: usize,
    },
    /// The cancel flag was set.
    Cancelled { embedded: usize, remaining: usize },
    /// A degenerate vector turned semantic mode off.
    Disabled { report: DisabledReport },
}

/// Chunks still to embed for one corpus.
#[derive(Debug, Default)]
pub struct IndexJob {
    queue: VecDeque<IndexItem>,
    removed: usize,
}

impl IndexJob {
    /// Full sync of a corpus: `desired` is every chunk that should have a vector. Rows of packs or keys not in
    /// `desired`, and rows whose content hash changed, are dropped now; missing ones are queued.
    #[must_use]
    pub fn plan(store: &mut VectorStore, desired: Vec<IndexItem>) -> Self {
        let wanted: HashMap<(&str, &str), &str> = desired
            .iter()
            .map(|i| {
                (
                    (i.pack.as_str(), i.doc_key.as_str()),
                    i.content_hash.as_str(),
                )
            })
            .collect();
        let packs: Vec<String> = store.packs().into_iter().map(str::to_string).collect();
        let mut removed = 0;
        for pack in &packs {
            removed += store.remove_where(pack, |key, hash| {
                wanted.get(&(pack.as_str(), key)) != Some(&hash)
            });
        }
        Self::queue_missing(store, desired, removed)
    }

    /// Incremental sync of one pack (spec sec. 9: embed only chunks whose content hash is new, drop rows whose
    /// key vanished). Other packs are untouched.
    #[must_use]
    pub fn plan_pack(store: &mut VectorStore, pack: &str, items: Vec<IndexItem>) -> Self {
        let items: Vec<IndexItem> = items.into_iter().filter(|i| i.pack == pack).collect();
        let wanted: HashMap<&str, &str> = items
            .iter()
            .map(|i| (i.doc_key.as_str(), i.content_hash.as_str()))
            .collect();
        let removed = store.remove_where(pack, |key, hash| wanted.get(key) != Some(&hash));
        Self::queue_missing(store, items, removed)
    }

    fn queue_missing(store: &VectorStore, items: Vec<IndexItem>, removed: usize) -> Self {
        let queue = items
            .into_iter()
            .filter(|i| !store.contains(&i.doc_key, &i.content_hash))
            .collect();
        Self { queue, removed }
    }

    /// Chunks still to embed.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.queue.len()
    }

    /// Rows dropped while planning.
    #[must_use]
    pub fn removed(&self) -> usize {
        self.removed
    }

    /// Embed queued chunks batch by batch until done, paused, cancelled or disabled, checkpointing `store`. When
    /// the queue empties the store is marked complete ([`VectorStore::mark_complete`]) before the last save.
    ///
    /// # Errors
    /// Store I/O errors and backend errors other than degenerate vectors (the checkpoint is saved first; the
    /// failed batch stays queued).
    pub fn run(
        &mut self,
        store: &mut VectorStore,
        embedder: &GuardedEmbedder,
        scheduler: &Scheduler,
        options: RunOptions,
        cancel: &AtomicBool,
    ) -> Result<RunOutcome> {
        self.run_observed(store, embedder, scheduler, options, cancel, &mut |_| {})
    }

    /// [`Self::run`], calling `checkpoint` with the store after every save (the app publishes a read snapshot
    /// there).
    ///
    /// # Errors
    /// As [`Self::run`].
    pub fn run_observed(
        &mut self,
        store: &mut VectorStore,
        embedder: &GuardedEmbedder,
        scheduler: &Scheduler,
        options: RunOptions,
        cancel: &AtomicBool,
        checkpoint: &mut dyn FnMut(&VectorStore),
    ) -> Result<RunOutcome> {
        let mut embedded = 0;
        let mut last_checkpoint = Instant::now();
        let batch = options.batch.max(1);
        let mut save = |store: &mut VectorStore| -> Result<()> {
            store.save()?;
            checkpoint(store);
            Ok(())
        };
        loop {
            if self.queue.is_empty() {
                store.mark_complete();
                save(store)?;
                return Ok(RunOutcome::Completed { embedded });
            }
            if cancel.load(Ordering::SeqCst) {
                save(store)?;
                return Ok(RunOutcome::Cancelled {
                    embedded,
                    remaining: self.remaining(),
                });
            }
            if let Decision::Pause(reason) = scheduler.decide() {
                save(store)?;
                if reason == PauseReason::GuardTripped {
                    if let Some(report) = embedder.guard().report() {
                        return Ok(RunOutcome::Disabled { report });
                    }
                }
                return Ok(RunOutcome::Paused {
                    reason,
                    embedded,
                    remaining: self.remaining(),
                });
            }
            let n = batch.min(self.queue.len());
            match self.embed_front(store, embedder, n) {
                Ok(()) => embedded += n,
                Err(EmbedError::Degenerate(_) | EmbedError::Disabled(_)) => {
                    save(store)?;
                    let report = embedder
                        .guard()
                        .report()
                        .ok_or_else(|| EmbedError::Backend("guard not tripped".into()))?;
                    return Ok(RunOutcome::Disabled { report });
                }
                Err(e) => {
                    save(store)?;
                    return Err(e);
                }
            }
            if last_checkpoint.elapsed() >= options.checkpoint_every {
                save(store)?;
                last_checkpoint = Instant::now();
            }
        }
    }

    /// Like [`Self::run`], but waits out pauses (polling every `poll`) until completed, cancelled or disabled.
    /// Meant for the app's low-priority indexing thread.
    ///
    /// # Errors
    /// As [`Self::run`].
    pub fn run_until_done(
        &mut self,
        store: &mut VectorStore,
        embedder: &GuardedEmbedder,
        scheduler: &Scheduler,
        options: RunOptions,
        poll: Duration,
        cancel: &AtomicBool,
    ) -> Result<RunOutcome> {
        let mut embedded = 0;
        loop {
            match self.run(store, embedder, scheduler, options, cancel)? {
                RunOutcome::Paused {
                    reason: reason @ (PauseReason::Unsupported | PauseReason::SemanticOff),
                    embedded: e,
                    remaining,
                } => {
                    // Settings-level pauses do not clear by themselves: hand back to the app.
                    return Ok(RunOutcome::Paused {
                        reason,
                        embedded: embedded + e,
                        remaining,
                    });
                }
                RunOutcome::Paused { embedded: e, .. } => {
                    embedded += e;
                    std::thread::sleep(poll);
                }
                RunOutcome::Completed { embedded: e } => {
                    return Ok(RunOutcome::Completed {
                        embedded: embedded + e,
                    })
                }
                RunOutcome::Cancelled {
                    embedded: e,
                    remaining,
                } => {
                    return Ok(RunOutcome::Cancelled {
                        embedded: embedded + e,
                        remaining,
                    })
                }
                disabled @ RunOutcome::Disabled { .. } => return Ok(disabled),
            }
        }
    }

    fn embed_front(
        &mut self,
        store: &mut VectorStore,
        embedder: &GuardedEmbedder,
        n: usize,
    ) -> Result<()> {
        let docs: Vec<DocText<'_>> = self
            .queue
            .iter()
            .take(n)
            .map(|i| DocText {
                title: i.title.as_deref(),
                text: &i.text,
            })
            .collect();
        let vectors = embedder.embed_docs(&docs)?;
        for v in vectors {
            let item = self.queue.pop_front().expect("one queued item per vector");
            store.upsert(&item.pack, &item.doc_key, &item.content_hash, &v)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: SchedulerSettings = SchedulerSettings {
        semantic: true,
        index_on_battery: false,
    };
    const ON_BATTERY_OK: SchedulerSettings = SchedulerSettings {
        semantic: true,
        index_on_battery: true,
    };

    fn battery(percent: u8) -> DeviceState {
        DeviceState {
            power: Power::Battery {
                percent: Some(percent),
            },
            ..DeviceState::DESKTOP
        }
    }

    const T2: IndexPolicy = IndexPolicy::ChargingOrBattery { min_percent: 50 };

    #[test]
    fn t0_and_settings_off_never_run() {
        let s = DeviceState::DESKTOP;
        assert_eq!(
            decide(IndexPolicy::Never, ON, &s),
            Decision::Pause(PauseReason::Unsupported)
        );
        let off = SchedulerSettings {
            semantic: false,
            index_on_battery: true,
        };
        assert_eq!(
            decide(IndexPolicy::Always, off, &s),
            Decision::Pause(PauseReason::SemanticOff)
        );
    }

    #[test]
    fn t1_indexes_only_while_charging() {
        let p = IndexPolicy::ChargingOnly;
        assert_eq!(decide(p, ON, &DeviceState::DESKTOP), Decision::Run);
        assert_eq!(
            decide(p, ON_BATTERY_OK, &battery(100)),
            Decision::Pause(PauseReason::NotCharging)
        );
        assert_eq!(
            decide(p, ON, &DeviceState::UNKNOWN_MOBILE),
            Decision::Pause(PauseReason::NotCharging)
        );
    }

    #[test]
    fn t2_battery_rule_needs_the_user_and_50_percent() {
        assert_eq!(decide(T2, ON, &DeviceState::DESKTOP), Decision::Run);
        assert_eq!(
            decide(T2, ON, &battery(90)),
            Decision::Pause(PauseReason::NotCharging)
        );
        assert_eq!(decide(T2, ON_BATTERY_OK, &battery(50)), Decision::Run);
        assert_eq!(
            decide(T2, ON_BATTERY_OK, &battery(49)),
            Decision::Pause(PauseReason::BatteryLow)
        );
        let unknown_level = DeviceState {
            power: Power::Battery { percent: None },
            ..DeviceState::DESKTOP
        };
        assert_eq!(
            decide(T2, ON_BATTERY_OK, &unknown_level),
            Decision::Pause(PauseReason::BatteryLow)
        );
    }

    #[test]
    fn t3_always_runs_on_desktop() {
        assert_eq!(
            decide(IndexPolicy::Always, ON, &DeviceState::DESKTOP),
            Decision::Run
        );
        assert_eq!(decide(IndexPolicy::Always, ON, &battery(5)), Decision::Run);
    }

    #[test]
    fn mobile_signals_pause_every_tier() {
        let charging = DeviceState::DESKTOP;
        for policy in [IndexPolicy::ChargingOnly, T2, IndexPolicy::Always] {
            let bg = DeviceState {
                backgrounded: true,
                ..charging
            };
            assert_eq!(
                decide(policy, ON, &bg),
                Decision::Pause(PauseReason::Backgrounded)
            );
            let low = DeviceState {
                low_power_mode: true,
                ..charging
            };
            assert_eq!(
                decide(policy, ON, &low),
                Decision::Pause(PauseReason::LowPowerMode)
            );
            for (thermal, paused) in [
                (Thermal::Nominal, false),
                (Thermal::Fair, false),
                (Thermal::Serious, true),
                (Thermal::Critical, true),
            ] {
                let hot = DeviceState {
                    thermal,
                    ..charging
                };
                assert_eq!(
                    decide(policy, ON, &hot) == Decision::Pause(PauseReason::Thermal),
                    paused,
                    "{policy:?} {thermal:?}"
                );
            }
        }
    }
}
