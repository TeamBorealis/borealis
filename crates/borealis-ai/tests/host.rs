#![cfg(all(feature = "host", feature = "remote"))]
//! LAN host + remote client tests over real loopback sockets with a stub runner.

mod host_lan;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use borealis_ai::host::{AiHost, HostConfig, HostHandle};
use borealis_ai::protocol::{ErrorBody, PATH_INFO, PATH_PAIR, PATH_TASKS};
use borealis_ai::remote::{RemoteClient, RemoteError};
use borealis_ai::task::qa::QaInput;
use borealis_ai::task::request::TaskRequest;
use borealis_ai::task::{AiRunner, BoxFuture, Ruleset};
use borealis_ai::{AiError, AiEvent, AiOutcome, AiRequest};
use tokio::sync::mpsc::UnboundedSender;

struct DropFlag(Arc<AtomicBool>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct Stub {
    reply: String,
    delay: Duration,
    dropped: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
    fail: bool,
}

impl Stub {
    fn ok() -> Self {
        Self {
            reply: r#"{"answer":"stubbed","confidence":"high"}"#.into(),
            delay: Duration::ZERO,
            dropped: Arc::default(),
            calls: Arc::default(),
            fail: false,
        }
    }
}

impl AiRunner for Stub {
    fn run<'a>(
        &'a self,
        _req: &'a AiRequest,
        sink: Option<&'a UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        Box::pin(async move {
            let _guard = DropFlag(self.dropped.clone());
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(s) = sink {
                let _ = s.send(AiEvent::TextDelta {
                    text: "thinking".into(),
                });
            }
            tokio::time::sleep(self.delay).await;
            if self.fail {
                return Err(AiError::Cli {
                    message: "boom at /Users/bob/x".into(),
                    stderr_tail: String::new(),
                    exit_code: Some(1),
                });
            }
            Ok(AiOutcome {
                text: self.reply.clone(),
                ..Default::default()
            })
        })
    }
}

use host_lan::{identity, loopback_config, Target};

async fn start(stub: Stub, tweak: impl FnOnce(&mut HostConfig)) -> (HostHandle, Target) {
    let mut cfg = loopback_config();
    tweak(&mut cfg);
    let host = AiHost::start(cfg, identity(), Arc::new(stub), None, vec![])
        .await
        .unwrap();
    let url = Target::of(&host);
    (host, url)
}

async fn paired(host: &HostHandle, url: &Target) -> RemoteClient {
    let code = host.begin_pairing().unwrap().code;
    let mut c = url.client();
    c.pair(&code, "Test phone").await.unwrap();
    c
}

fn qa() -> TaskRequest {
    TaskRequest::RulesQa(QaInput {
        question: "q".into(),
        ruleset: Ruleset::Dnd2014,
        context: vec![],
        character: None,
    })
}

#[tokio::test]
async fn info_is_public_and_hides_pairing_state() {
    let (host, url) = start(Stub::ok(), |_| {}).await;
    let c = url.client();
    assert_eq!(c.info().await.unwrap().protocol, 2);
    host.begin_pairing().unwrap();
    let raw: serde_json::Value = url
        .raw()
        .get(format!("{url}{PATH_INFO}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let keys: Vec<_> = raw.as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys, ["name", "protocol"], "no pairing_open leak: {raw}");
}

#[tokio::test]
async fn pairing_flow_and_single_use_code() {
    let (host, url) = start(Stub::ok(), |_| {}).await;
    let code = host.begin_pairing().unwrap().code;
    let mut a = url.client();
    let r = a.pair(&format!(" {code} "), "Phone A").await.unwrap();
    assert_eq!(r.token.len(), 64);
    assert_eq!(host.devices().len(), 1);
    assert_eq!(host.devices()[0].name, "Phone A");
    assert!(
        !host.devices()[0].token_hash.contains(&r.token),
        "only a hash is stored"
    );
    let mut b = url.client();
    let err = b.pair(&code, "Phone B").await.unwrap_err();
    assert!(err.to_string().contains("pairing_failed"), "{err}");
}

#[tokio::test]
async fn wrong_code_attempts_lock_out_the_sender() {
    let (host, url) = start(Stub::ok(), |c| c.max_pairing_attempts = 3).await;
    let code = host.begin_pairing().unwrap().code;
    let wrong = if code == "00000000" {
        "00000001"
    } else {
        "00000000"
    };
    let mut c = url.client();
    for _ in 0..3 {
        assert!(c.pair(wrong, "x").await.is_err());
    }
    let err = c.pair(&code, "x").await.unwrap_err();
    assert!(
        err.to_string().contains("pairing_failed"),
        "sender must be locked out after 3 misses: {err}"
    );
    assert!(host.devices().is_empty());
}

#[tokio::test]
async fn pairing_without_open_window_or_after_expiry_fails() {
    let (host, url) = start(Stub::ok(), |c| c.pairing_ttl = Duration::from_millis(50)).await;
    let mut c = url.client();
    assert!(c.pair("12345678", "x").await.is_err());
    let code = host.begin_pairing().unwrap().code;
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert!(c.pair(&code, "x").await.is_err());
}

#[tokio::test]
async fn tasks_require_a_valid_token() {
    let (host, url) = start(Stub::ok(), |_| {}).await;
    let _ = host;
    let none = url.client();
    assert!(matches!(
        none.run_task(&qa(), None).await,
        Err(RemoteError::Ai(AiError::Invalid(_)))
    ));
    let bogus = url.client().with_token("deadbeef");
    let err = bogus.run_task(&qa(), None).await.unwrap_err();
    assert!(err.to_string().contains("unauthorized"), "{err}");
}

#[tokio::test]
async fn full_task_run_streams_events_and_returns_typed_result() {
    let (host, url) = start(Stub::ok(), |_| {}).await;
    let c = paired(&host, &url).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let resp = c.run_task(&qa(), Some(&tx)).await.unwrap();
    assert_eq!(resp.task, "rules_qa");
    assert_eq!(resp.output["answer"], "stubbed");
    assert_eq!(resp.attempts, 1);
    let ev = rx.try_recv().expect("progress event forwarded over SSE");
    assert_eq!(
        ev,
        AiEvent::TextDelta {
            text: "thinking".into()
        }
    );
}

#[tokio::test]
async fn task_failure_is_reported_and_redacted() {
    let mut stub = Stub::ok();
    stub.fail = true;
    let (host, url) = start(stub, |_| {}).await;
    let c = paired(&host, &url).await;
    let err = c.run_task(&qa(), None).await.unwrap_err().to_string();
    assert!(err.contains("task_failed") && err.contains("boom"), "{err}");
    assert!(
        !err.contains("bob"),
        "paths redacted before leaving the host: {err}"
    );
}

#[tokio::test]
async fn second_concurrent_task_gets_busy() {
    let mut stub = Stub::ok();
    stub.delay = Duration::from_millis(600);
    let (host, url) = start(stub, |c| c.max_concurrent = 1).await;
    let c = paired(&host, &url).await;
    let c2 = c.clone();
    let first = tokio::spawn(async move { c2.run_task(&qa(), None).await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    let err = c.run_task(&qa(), None).await.unwrap_err().to_string();
    assert!(err.contains("busy"), "{err}");
    assert!(first.await.unwrap().is_ok());
    assert!(
        c.run_task(&qa(), None).await.is_ok(),
        "permit released afterwards"
    );
}

#[tokio::test]
async fn concurrency_is_clamped_to_two() {
    let mut stub = Stub::ok();
    stub.delay = Duration::from_millis(500);
    let (host, url) = start(stub, |c| c.max_concurrent = 50).await;
    let c = paired(&host, &url).await;
    let mut jobs = Vec::new();
    for _ in 0..2 {
        let c = c.clone();
        jobs.push(tokio::spawn(async move { c.run_task(&qa(), None).await }));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let err = c.run_task(&qa(), None).await.unwrap_err().to_string();
    assert!(err.contains("busy"), "third must be rejected: {err}");
    for j in jobs {
        assert!(j.await.unwrap().is_ok());
    }
}

#[tokio::test]
async fn rate_limit_applies_per_device() {
    let (host, url) = start(Stub::ok(), |c| c.rate_limit_per_minute = 2).await;
    let c = paired(&host, &url).await;
    assert!(c.run_task(&qa(), None).await.is_ok());
    assert!(c.run_task(&qa(), None).await.is_ok());
    let err = c.run_task(&qa(), None).await.unwrap_err().to_string();
    assert!(err.contains("rate_limited"), "{err}");
}

#[tokio::test]
async fn malformed_and_oversized_bodies_are_rejected() {
    let (host, url) = start(Stub::ok(), |c| c.max_body_bytes = 1024).await;
    let c = paired(&host, &url).await;
    let token = c.token().unwrap().to_string();
    let http = url.raw();
    let r = http
        .post(format!("{url}{PATH_TASKS}"))
        .bearer_auth(&token)
        .body("{not json")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
    let body: ErrorBody = r.json().await.unwrap();
    assert_eq!(body.error, "bad_request");
    let r = http
        .post(format!("{url}{PATH_TASKS}"))
        .bearer_auth(&token)
        .body(vec![b'a'; 4096])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 413);
    let r = http
        .post(format!("{url}{PATH_TASKS}"))
        .bearer_auth(&token)
        .body(r#"{"task":"rm_rf","input":{}}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400, "unknown tasks are not executable");
}

#[tokio::test]
async fn sse_response_has_event_stream_content_type() {
    let (host, url) = start(Stub::ok(), |_| {}).await;
    let c = paired(&host, &url).await;
    let r = url
        .raw()
        .post(format!("{url}{PATH_TASKS}"))
        .bearer_auth(c.token().unwrap())
        .json(&qa())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert!(r.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
    let text = r.text().await.unwrap();
    assert!(
        text.contains("event: ai") && text.contains("event: result"),
        "{text}"
    );
}

#[tokio::test]
async fn unpair_and_revoke_invalidate_tokens() {
    let (host, url) = start(Stub::ok(), |_| {}).await;
    let mut c = paired(&host, &url).await;
    let old = c.token().unwrap().to_string();
    c.unpair().await.unwrap();
    assert!(host.devices().is_empty());
    let stale = url.client().with_token(old);
    assert!(stale.run_task(&qa(), None).await.is_err());

    let c2 = paired(&host, &url).await;
    let id = host.devices()[0].id.clone();
    assert!(host.revoke(&id));
    assert!(!host.revoke(&id));
    assert!(c2.run_task(&qa(), None).await.is_err());
}

#[tokio::test]
async fn device_limit_is_enforced() {
    let (host, url) = start(Stub::ok(), |c| c.max_devices = 1).await;
    let _first = paired(&host, &url).await;
    let code = host.begin_pairing().unwrap().code;
    let mut second = url.client();
    assert!(second.pair(&code, "two").await.is_err());
}

#[tokio::test]
async fn known_devices_survive_a_restart() {
    let (host, url) = start(Stub::ok(), |_| {}).await;
    let c = paired(&host, &url).await;
    let records = host.devices();
    host.shutdown().await;
    let host2 = AiHost::start(
        loopback_config(),
        identity(),
        Arc::new(Stub::ok()),
        None,
        records,
    )
    .await
    .unwrap();
    let again = Target::of(&host2).client().with_token(c.token().unwrap());
    assert!(again.run_task(&qa(), None).await.is_ok());
}

#[tokio::test]
async fn client_disconnect_cancels_the_running_task() {
    let mut stub = Stub::ok();
    stub.delay = Duration::from_secs(30);
    let dropped = stub.dropped.clone();
    let (host, url) = start(stub, |_| {}).await;
    let c = paired(&host, &url).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let job = tokio::spawn({
        let c = c.clone();
        async move { c.run_task(&qa(), Some(&tx)).await }
    });
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    job.abort();
    for _ in 0..50 {
        if dropped.load(Ordering::SeqCst) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("host kept running the task after the client disconnected");
}

#[tokio::test]
async fn pair_attempts_are_rate_limited_per_ip() {
    let (host, url) = start(Stub::ok(), |c| c.max_pairing_attempts = 1000).await;
    let _ = host.begin_pairing().unwrap();
    let http = url.raw();
    let mut limited = false;
    for _ in 0..15 {
        let r = http
            .post(format!("{url}{PATH_PAIR}"))
            .json(&serde_json::json!({"code":"9999999x","device_name":"x"}))
            .send()
            .await
            .unwrap();
        if r.status() == 429 {
            limited = true;
            break;
        }
    }
    assert!(limited, "brute-forcing the code must hit the rate limiter");
}

#[tokio::test]
async fn shutdown_stops_listening() {
    let (host, url) = start(Stub::ok(), |_| {}).await;
    let c = url.client();
    assert!(c.info().await.is_ok());
    host.shutdown().await;
    assert!(c.info().await.is_err());
}

#[test]
fn lan_ip_detection_does_not_panic() {
    let _ = borealis_ai::host::detect_lan_ip();
}
