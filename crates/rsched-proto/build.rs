//! tonic-build driver — compiles `proto/agent.proto` into Rust code under
//! `OUT_DIR/agent.rs`. Re-exported by `lib.rs` via `tonic::include_proto!`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&["proto/agent.proto"], &["proto"])?;
    println!("cargo:rerun-if-changed=proto/agent.proto");
    Ok(())
}
