//! rsched-proto — protobuf + tonic-generated types for the agent <-> server
//! bidi gRPC channel.
//!
//! See `proto/agent.proto` for the wire format. The generated module is
//! re-exported as [`agent`].
#![allow(missing_docs)]

/// Generated module from `proto/agent.proto`.
pub mod agent {
    tonic::include_proto!("agent");
}

pub use agent::{
    agent_client, agent_msg, agent_server, server_msg, AgentMsg, Dispatch, Heartbeat, Kill,
    LogChunk, Result as RunResult, ServerMsg, Signal,
};

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    #[test]
    fn dispatch_round_trips() {
        let mut env = std::collections::HashMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        let d = Dispatch {
            run_id: "01HXYZ".into(),
            cmd: "echo".into(),
            args: vec!["hi".into()],
            env,
            cwd: "/tmp".into(),
            timeout_secs: 30,
        };
        let bytes = d.encode_to_vec();
        let d2 = Dispatch::decode(bytes.as_slice()).unwrap();
        assert_eq!(d, d2);
    }

    #[test]
    fn log_chunk_round_trips() {
        let lc = LogChunk {
            run_id: "r1".into(),
            stream: 1,
            ts_unix_ms: 1_700_000_000_123,
            bytes: b"hello\n".to_vec(),
        };
        let bytes = lc.encode_to_vec();
        let lc2 = LogChunk::decode(bytes.as_slice()).unwrap();
        assert_eq!(lc, lc2);
    }

    #[test]
    fn server_msg_oneof_dispatch_round_trips() {
        let msg = ServerMsg {
            kind: Some(server_msg::Kind::Dispatch(Dispatch {
                run_id: "r2".into(),
                cmd: "true".into(),
                args: vec![],
                env: Default::default(),
                cwd: String::new(),
                timeout_secs: 0,
            })),
        };
        let bytes = msg.encode_to_vec();
        let back = ServerMsg::decode(bytes.as_slice()).unwrap();
        match back.kind {
            Some(server_msg::Kind::Dispatch(d)) => assert_eq!(d.cmd, "true"),
            other => panic!("expected Dispatch, got {other:?}"),
        }
    }

    #[test]
    fn agent_msg_oneof_result_round_trips() {
        let msg = AgentMsg {
            kind: Some(agent_msg::Kind::Result(RunResult {
                run_id: "r3".into(),
                exit_code: 7,
                timed_out: false,
                peak_rss_bytes: 1024,
                cpu_user_ms: 50,
                cpu_sys_ms: 10,
            })),
        };
        let bytes = msg.encode_to_vec();
        let back = AgentMsg::decode(bytes.as_slice()).unwrap();
        match back.kind {
            Some(agent_msg::Kind::Result(r)) => {
                assert_eq!(r.exit_code, 7);
                assert_eq!(r.peak_rss_bytes, 1024);
            }
            other => panic!("expected Result, got {other:?}"),
        }
    }
}
