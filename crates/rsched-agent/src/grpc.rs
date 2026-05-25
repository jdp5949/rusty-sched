//! gRPC remote-agent client — STUB.
//!
//! Implements the [`Executor`] trait against a remote agent process speaking
//! the `rsched-proto` bidi gRPC `Stream` RPC. In this v0.5.2 skeleton the
//! client only *constructs* — `dispatch()` logs a warning and returns
//! [`AgentError::Io`]. The full handshake, log demux, and outcome plumbing
//! land in a follow-up.
//!
//! Cert pinning, client auth (mTLS), and reconnection back-off are out of
//! scope for the skeleton; the [`GrpcExecutor::tls`] field is a placeholder
//! [`tonic::transport::ClientTlsConfig`] today.

use crate::exec::{Executor, RunHandle};
use crate::AgentError;
use async_trait::async_trait;
use rsched_core::{Job, RunId};
use tonic::transport::{ClientTlsConfig, Endpoint};
use tracing::warn;

/// Remote-agent executor — connects to one rusty-sched agent process over
/// gRPC + TLS.
///
/// Construction parses the endpoint and TLS config eagerly so misconfig
/// surfaces early. Dispatching is a stub today.
#[derive(Clone)]
pub struct GrpcExecutor {
    /// Validated tonic endpoint (scheme + host + port).
    endpoint: Endpoint,
    /// Reserved for mTLS — pinned roots + client identity. Skeleton only.
    #[allow(dead_code)]
    tls: Option<ClientTlsConfig>,
}

impl GrpcExecutor {
    /// Build a new client. `endpoint_url` must be a tonic-parseable URI
    /// like `https://agent.example.com:7443`. `tls` is currently unused
    /// beyond storage — a future patch will wire it into `Endpoint::tls_config`.
    pub fn new(
        endpoint_url: impl Into<String>,
        tls: Option<ClientTlsConfig>,
    ) -> Result<Self, AgentError> {
        let url: String = endpoint_url.into();
        let endpoint = Endpoint::from_shared(url.clone()).map_err(|e| {
            AgentError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid endpoint {url}: {e}"),
            ))
        })?;
        Ok(Self { endpoint, tls })
    }

    /// Endpoint URI as a string (for logs / tests).
    pub fn endpoint_uri(&self) -> String {
        self.endpoint.uri().to_string()
    }
}

#[async_trait]
impl Executor for GrpcExecutor {
    async fn dispatch(&self, run_id: RunId, _job: Job) -> Result<RunHandle, AgentError> {
        warn!(
            %run_id,
            endpoint = %self.endpoint.uri(),
            "GrpcExecutor::dispatch is a v0.5.2 stub — no agent connected, returning Io error"
        );
        Err(AgentError::Io(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "grpc remote agent not implemented",
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsched_core::{JobBuilder, RunId, Trigger};

    fn job() -> Job {
        JobBuilder::new(
            "grpc-test",
            "echo hi",
            Trigger::Cron {
                expr: "* * * * *".into(),
                timezone: None,
            },
        )
        .build()
        .unwrap()
    }

    #[test]
    fn constructs_with_valid_https_endpoint() {
        let exe = GrpcExecutor::new("https://agent.example.com:7443", None).unwrap();
        assert!(exe.endpoint_uri().contains("agent.example.com"));
    }

    #[test]
    fn rejects_invalid_endpoint() {
        match GrpcExecutor::new("not a url", None) {
            Err(AgentError::Io(_)) => {}
            Err(e) => panic!("expected Io error, got {e:?}"),
            Ok(_) => panic!("expected error for invalid endpoint"),
        }
    }

    #[tokio::test]
    async fn dispatch_returns_io_error_when_unwired() {
        let exe = GrpcExecutor::new("https://127.0.0.1:1", None).unwrap();
        match exe.dispatch(RunId::new(), job()).await {
            Err(AgentError::Io(_)) => {}
            Err(e) => panic!("expected Io error, got {e:?}"),
            Ok(_) => panic!("expected error, got ok"),
        }
    }
}
