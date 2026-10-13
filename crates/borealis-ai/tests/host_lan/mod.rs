#![allow(dead_code)]
//! Shared helpers for the LAN host tests: one TLS identity, loopback config, pinned clients.

use std::fmt;
use std::net::SocketAddr;
use std::sync::OnceLock;

use borealis_ai::host::{HostConfig, HostHandle, HostIdentity};
use borealis_ai::protocol::CertFingerprint;
use borealis_ai::remote::{pinned_tls_config, RemoteClient};

/// One identity for the whole test binary (also exercises restart with the same cert).
pub fn identity() -> &'static HostIdentity {
    static ID: OnceLock<HostIdentity> = OnceLock::new();
    ID.get_or_init(|| HostIdentity::generate().unwrap())
}

/// Default config bound to loopback with an OS-assigned port.
pub fn loopback_config() -> HostConfig {
    HostConfig {
        bind: Some("127.0.0.1:0".parse().unwrap()),
        ..Default::default()
    }
}

/// A running host's address and pin. `Display` gives the `https://ip:port` base URL.
#[derive(Debug, Clone, Copy)]
pub struct Target {
    pub addr: SocketAddr,
    pub fp: CertFingerprint,
}

impl Target {
    pub fn of(host: &HostHandle) -> Self {
        Self {
            addr: host.local_addr(),
            fp: host.fingerprint(),
        }
    }

    /// Pinned protocol client.
    pub fn client(&self) -> RemoteClient {
        RemoteClient::new(self.addr, self.fp).unwrap()
    }

    /// Raw reqwest client pinned to the host certificate.
    pub fn raw(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .tls_backend_preconfigured(pinned_tls_config(self.fp).unwrap())
            .build()
            .unwrap()
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "https://{}", self.addr)
    }
}

/// Runner that answers every request with a valid rules-QA payload.
pub struct OkRunner;

impl borealis_ai::task::AiRunner for OkRunner {
    fn run<'a>(
        &'a self,
        _req: &'a borealis_ai::AiRequest,
        _sink: Option<&'a tokio::sync::mpsc::UnboundedSender<borealis_ai::AiEvent>>,
    ) -> borealis_ai::task::BoxFuture<'a, Result<borealis_ai::AiOutcome, borealis_ai::AiError>>
    {
        Box::pin(async {
            Ok(borealis_ai::AiOutcome {
                text: r#"{"answer":"ok","confidence":"high"}"#.into(),
                ..Default::default()
            })
        })
    }
}

/// A minimal rules-QA task.
pub fn qa() -> borealis_ai::task::request::TaskRequest {
    borealis_ai::task::request::TaskRequest::RulesQa(borealis_ai::task::qa::QaInput {
        question: "q".into(),
        ruleset: borealis_ai::task::Ruleset::Dnd2014,
        context: vec![],
        character: None,
    })
}
