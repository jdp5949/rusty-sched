//! rusty-sched — single-binary entrypoint. Modes: server | agent | cli.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rusty-sched", version, about = "Autosys-class job scheduler")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the scheduler server (HA-capable).
    Server,
    /// Run the execution agent on this host.
    Agent,
    /// Local + remote CLI operations.
    #[command(subcommand)]
    Job(JobCmd),
    /// Print version + build info.
    Version,
}

#[derive(Subcommand)]
enum JobCmd {
    /// List jobs.
    List,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Server => anyhow::bail!("server mode not yet implemented (see plan M10)"),
        Cmd::Agent => anyhow::bail!("agent mode not yet implemented (see plan M4)"),
        Cmd::Job(_) => anyhow::bail!("cli not yet implemented (see plan M9)"),
        Cmd::Version => {
            println!("rusty-sched {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
