//! Checkpointed indexing under simulated battery / thermal / background states, resume after a restart, and the
//! NaN guard turning semantic mode off mid-index.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use borealis_embed::scheduler::RunOptions;
use borealis_embed::{
    DeviceState, DeviceStateProvider, FakeEmbedder, GuardedEmbedder, IndexItem, IndexJob,
    IndexPolicy, PauseReason, Power, RunOutcome, Scheduler, SchedulerSettings, SemanticGuard,
    SimulatedDevice, StoreId, Thermal, Tier, VectorStore,
};

const DIMS: usize = 128;

fn items(pack: &str, n: usize) -> Vec<IndexItem> {
    (0..n)
        .map(|i| IndexItem {
            pack: pack.into(),
            doc_key: format!("{pack}:{i}"),
            content_hash: format!("hash-{i}"),
            title: Some(format!("Entry {i}")),
            text: format!("text number {i} about topic{}", i % 7),
        })
        .collect()
}

fn on_battery(percent: u8) -> DeviceState {
    DeviceState {
        power: Power::Battery {
            percent: Some(percent),
        },
        ..DeviceState::DESKTOP
    }
}

fn setup(
    dir: &std::path::Path,
    tier: Tier,
    device: Arc<dyn DeviceStateProvider>,
    settings: SchedulerSettings,
) -> (VectorStore, GuardedEmbedder, Scheduler) {
    let fake = Arc::new(FakeEmbedder::new(DIMS).with_poison("POISON"));
    let guard = SemanticGuard::new();
    let embedder = GuardedEmbedder::new(fake, guard.clone());
    let (store, _) =
        VectorStore::open(dir, StoreId::for_embedder(embedder.embedder()).expect("id"))
            .expect("open");
    let scheduler = Scheduler::new(tier.config(8).policy, settings, device, guard);
    (store, embedder, scheduler)
}

const ON: SchedulerSettings = SchedulerSettings {
    semantic: true,
    index_on_battery: false,
};

fn opts(batch: usize) -> RunOptions {
    RunOptions {
        batch,
        checkpoint_every: Duration::from_secs(3600),
    }
}

/// Answers a scripted state per call, then the last one forever.
struct Scripted {
    states: Vec<DeviceState>,
    calls: AtomicUsize,
}

impl DeviceStateProvider for Scripted {
    fn current(&self) -> DeviceState {
        let i = self.calls.fetch_add(1, Ordering::SeqCst);
        self.states[i.min(self.states.len() - 1)]
    }
}

#[test]
fn t1_pauses_when_unplugged_and_resumes_when_charging() {
    let tmp = tempfile::tempdir().expect("tmp");
    let device = Arc::new(SimulatedDevice::new(on_battery(90)));
    let (mut store, embedder, scheduler) = setup(tmp.path(), Tier::T1, device.clone(), ON);
    let mut job = IndexJob::plan(&mut store, items("srd", 20));
    let cancel = AtomicBool::new(false);

    let out = job
        .run(&mut store, &embedder, &scheduler, opts(4), &cancel)
        .expect("run");
    assert_eq!(
        out,
        RunOutcome::Paused {
            reason: PauseReason::NotCharging,
            embedded: 0,
            remaining: 20
        }
    );

    device.set(DeviceState::DESKTOP); // plugged in
    let out = job
        .run(&mut store, &embedder, &scheduler, opts(4), &cancel)
        .expect("run");
    assert_eq!(out, RunOutcome::Completed { embedded: 20 });
    assert_eq!(store.len(), 20);
}

#[test]
fn unplugging_mid_run_pauses_at_the_next_batch_with_a_checkpoint() {
    let tmp = tempfile::tempdir().expect("tmp");
    // Charging for two batches, then unplugged.
    let device = Arc::new(Scripted {
        states: vec![DeviceState::DESKTOP, DeviceState::DESKTOP, on_battery(80)],
        calls: AtomicUsize::new(0),
    });
    let (mut store, embedder, scheduler) = setup(tmp.path(), Tier::T1, device, ON);
    let mut job = IndexJob::plan(&mut store, items("srd", 20));
    let out = job
        .run(
            &mut store,
            &embedder,
            &scheduler,
            opts(5),
            &AtomicBool::new(false),
        )
        .expect("run");
    assert_eq!(
        out,
        RunOutcome::Paused {
            reason: PauseReason::NotCharging,
            embedded: 10,
            remaining: 10
        }
    );

    // "Restart": a fresh store from disk and a fresh plan only queue what the checkpoint lacks.
    drop(store);
    let device = Arc::new(SimulatedDevice::new(DeviceState::DESKTOP));
    let (mut store, embedder, scheduler) = setup(tmp.path(), Tier::T1, device, ON);
    assert_eq!(store.len(), 10, "checkpoint saved on pause");
    let mut job = IndexJob::plan(&mut store, items("srd", 20));
    assert_eq!(job.remaining(), 10);
    let out = job
        .run(
            &mut store,
            &embedder,
            &scheduler,
            opts(5),
            &AtomicBool::new(false),
        )
        .expect("run");
    assert_eq!(out, RunOutcome::Completed { embedded: 10 });
}

#[test]
fn t2_battery_rule_follows_the_user_setting_and_level() {
    let tmp = tempfile::tempdir().expect("tmp");
    let device = Arc::new(SimulatedDevice::new(on_battery(70)));
    let allow = SchedulerSettings {
        semantic: true,
        index_on_battery: true,
    };
    let (mut store, embedder, scheduler) = setup(tmp.path(), Tier::T2, device.clone(), ON);
    let mut job = IndexJob::plan(&mut store, items("lore", 6));
    let cancel = AtomicBool::new(false);
    assert!(matches!(
        job.run(&mut store, &embedder, &scheduler, opts(2), &cancel),
        Ok(RunOutcome::Paused {
            reason: PauseReason::NotCharging,
            ..
        })
    ));

    let scheduler = Scheduler::new(
        scheduler.policy,
        allow,
        device.clone(),
        embedder.guard().clone(),
    );
    device.set(on_battery(49));
    assert!(matches!(
        job.run(&mut store, &embedder, &scheduler, opts(2), &cancel),
        Ok(RunOutcome::Paused {
            reason: PauseReason::BatteryLow,
            ..
        })
    ));
    device.set(on_battery(50));
    assert_eq!(
        job.run(&mut store, &embedder, &scheduler, opts(2), &cancel)
            .expect("run"),
        RunOutcome::Completed { embedded: 6 }
    );
}

#[test]
fn thermal_low_power_and_background_pause_then_resume() {
    let tmp = tempfile::tempdir().expect("tmp");
    let device = Arc::new(SimulatedDevice::new(DeviceState::DESKTOP));
    let (mut store, embedder, scheduler) = setup(tmp.path(), Tier::T2, device.clone(), ON);
    let mut job = IndexJob::plan(&mut store, items("srd", 4));
    let cancel = AtomicBool::new(false);
    for (state, reason) in [
        (
            DeviceState {
                thermal: Thermal::Serious,
                ..DeviceState::DESKTOP
            },
            PauseReason::Thermal,
        ),
        (
            DeviceState {
                low_power_mode: true,
                ..DeviceState::DESKTOP
            },
            PauseReason::LowPowerMode,
        ),
        (
            DeviceState {
                backgrounded: true,
                ..DeviceState::DESKTOP
            },
            PauseReason::Backgrounded,
        ),
    ] {
        device.set(state);
        match job
            .run(&mut store, &embedder, &scheduler, opts(2), &cancel)
            .expect("run")
        {
            RunOutcome::Paused { reason: r, .. } => assert_eq!(r, reason),
            other => panic!("{state:?}: {other:?}"),
        }
    }
    device.set(DeviceState {
        thermal: Thermal::Fair,
        ..DeviceState::DESKTOP
    });
    assert_eq!(
        job.run(&mut store, &embedder, &scheduler, opts(2), &cancel)
            .expect("run"),
        RunOutcome::Completed { embedded: 4 }
    );
}

#[test]
fn run_until_done_waits_out_a_pause() {
    let tmp = tempfile::tempdir().expect("tmp");
    let device = Arc::new(SimulatedDevice::new(on_battery(30)));
    let (mut store, embedder, scheduler) = setup(tmp.path(), Tier::T1, device.clone(), ON);
    let mut job = IndexJob::plan(&mut store, items("srd", 8));
    let plug = {
        let device = device.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            device.set(DeviceState::DESKTOP);
        })
    };
    let out = job
        .run_until_done(
            &mut store,
            &embedder,
            &scheduler,
            opts(3),
            Duration::from_millis(10),
            &AtomicBool::new(false),
        )
        .expect("run");
    plug.join().expect("join");
    assert_eq!(out, RunOutcome::Completed { embedded: 8 });
}

#[test]
fn t0_and_semantic_off_never_index() {
    let tmp = tempfile::tempdir().expect("tmp");
    let device = Arc::new(SimulatedDevice::new(DeviceState::DESKTOP));
    let (mut store, embedder, scheduler) = setup(tmp.path(), Tier::T0, device.clone(), ON);
    let mut job = IndexJob::plan(&mut store, items("srd", 3));
    let out = job
        .run_until_done(
            &mut store,
            &embedder,
            &scheduler,
            opts(3),
            Duration::from_millis(1),
            &AtomicBool::new(false),
        )
        .expect("run");
    assert!(matches!(
        out,
        RunOutcome::Paused {
            reason: PauseReason::Unsupported,
            ..
        }
    ));
    let off = SchedulerSettings {
        semantic: false,
        index_on_battery: false,
    };
    let scheduler = Scheduler::new(IndexPolicy::Always, off, device, embedder.guard().clone());
    assert!(matches!(
        job.run(
            &mut store,
            &embedder,
            &scheduler,
            opts(3),
            &AtomicBool::new(false)
        ),
        Ok(RunOutcome::Paused {
            reason: PauseReason::SemanticOff,
            ..
        })
    ));
    assert!(store.is_empty());
}

#[test]
fn cancel_saves_the_checkpoint() {
    let tmp = tempfile::tempdir().expect("tmp");
    let device = Arc::new(SimulatedDevice::new(DeviceState::DESKTOP));
    let (mut store, embedder, scheduler) = setup(tmp.path(), Tier::T3, device, ON);
    let mut job = IndexJob::plan(&mut store, items("srd", 5));
    let out = job
        .run(
            &mut store,
            &embedder,
            &scheduler,
            opts(2),
            &AtomicBool::new(true),
        )
        .expect("run");
    assert_eq!(
        out,
        RunOutcome::Cancelled {
            embedded: 0,
            remaining: 5
        }
    );
}

#[test]
fn nan_while_indexing_disables_semantic_mode() {
    let tmp = tempfile::tempdir().expect("tmp");
    let device = Arc::new(SimulatedDevice::new(DeviceState::DESKTOP));
    let (mut store, embedder, scheduler) = setup(tmp.path(), Tier::T3, device, ON);
    let mut all = items("srd", 6);
    all[4].text = "this chunk makes the model emit POISON".into();
    let mut job = IndexJob::plan(&mut store, all);
    let out = job
        .run(
            &mut store,
            &embedder,
            &scheduler,
            opts(2),
            &AtomicBool::new(false),
        )
        .expect("run");
    let RunOutcome::Disabled { report } = out else {
        panic!("{out:?}");
    };
    assert_eq!(report.reason, borealis_embed::Degeneracy::NonFinite);
    assert!(!embedder.guard().is_enabled());
    assert_eq!(store.len(), 4, "batches before the bad one are kept");
    assert_eq!(job.remaining(), 2, "the failed batch stays queued");
    // Queries are refused too until the guard is reset.
    assert!(embedder.embed_query("fireball").is_err());
    assert!(matches!(
        job.run(
            &mut store,
            &embedder,
            &scheduler,
            opts(2),
            &AtomicBool::new(false)
        ),
        Ok(RunOutcome::Disabled { .. })
    ));
}

#[test]
fn plan_drops_vanished_and_changed_rows_and_queues_only_new_hashes() {
    let tmp = tempfile::tempdir().expect("tmp");
    let device = Arc::new(SimulatedDevice::new(DeviceState::DESKTOP));
    let (mut store, embedder, scheduler) = setup(tmp.path(), Tier::T3, device, ON);
    let cancel = AtomicBool::new(false);
    let mut desired = items("srd", 5);
    desired.extend(items("homebrew", 3));
    let mut job = IndexJob::plan(&mut store, desired.clone());
    job.run(&mut store, &embedder, &scheduler, opts(4), &cancel)
        .expect("run");
    assert_eq!(store.len(), 8);

    // Pack update: one chunk changed, one vanished, one added. Other packs untouched.
    let mut srd = items("srd", 5);
    srd[1].content_hash = "changed".into();
    srd.remove(3);
    srd.push(IndexItem {
        doc_key: "srd:new".into(),
        ..srd[0].clone()
    });
    let mut job = IndexJob::plan_pack(&mut store, "srd", srd);
    assert_eq!(job.removed(), 2, "changed + vanished");
    assert_eq!(job.remaining(), 2, "changed + new");
    job.run(&mut store, &embedder, &scheduler, opts(4), &cancel)
        .expect("run");
    assert_eq!(store.len(), 8);
    assert!(store.contains("srd:1", "changed"));
    assert!(!store.contains("srd:3", "hash-3"));
    assert!(store.contains("homebrew:2", "hash-2"));

    // Full plan without homebrew drops the pack.
    let job = IndexJob::plan(&mut store, items("srd", 1));
    assert_eq!(job.remaining(), 0);
    assert_eq!(store.len(), 1);
}
