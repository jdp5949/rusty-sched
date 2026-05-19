//! clap subcommand definitions + dispatcher.

use crate::ApiClient;
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};

/// Top-level CLI.
#[derive(Debug, Parser)]
#[command(
    name = "rusty-sched-cli",
    version,
    about = "rusty-sched command-line client"
)]
pub struct Cli {
    /// REST API base URL.
    #[arg(long, env = "RSCHED_URL", default_value = "http://localhost:8080")]
    pub url: String,

    /// Subcommand.
    #[command(subcommand)]
    pub cmd: Cmd,
}

/// Subcommands.
#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// List all jobs (id + name + paused flag).
    List,
    /// Create a job from a YAML or JSON spec file.
    Apply {
        /// Spec file path.
        #[arg(short, long)]
        file: String,
    },
    /// Replace a job's mutable fields from a YAML/JSON file.
    Update {
        /// Job NAME or ULID.
        target: String,
        /// New spec file.
        #[arg(short, long)]
        file: String,
    },
    /// Show a job's full definition (JSON).
    Show {
        /// Job NAME or ULID.
        target: String,
    },
    /// Delete a job by NAME or ULID.
    Delete {
        /// Job NAME or ULID.
        target: String,
    },
    /// Trigger a job manually (Autosys-equivalent: sendevent STARTJOB).
    Trigger {
        /// Job NAME or ULID.
        target: String,
    },
    /// Pause a job (Autosys-equivalent: ON_HOLD).
    Pause {
        /// Job NAME or ULID.
        target: String,
    },
    /// Resume a paused job (Autosys-equivalent: OFF_HOLD).
    Resume {
        /// Job NAME or ULID.
        target: String,
    },
    /// List recent runs for a job (Autosys-equivalent: autorep -j NAME).
    Runs {
        /// Job NAME or ULID.
        target: String,
    },
    /// Autosys-style verb: sendevent NAME EVENT (STARTJOB|KILLJOB|ON_HOLD|OFF_HOLD).
    Sendevent {
        /// Job NAME or ULID.
        target: String,
        /// Event name: STARTJOB / KILLJOB / ON_HOLD / OFF_HOLD.
        event: String,
    },
}

/// Execute the CLI: returns Ok(()) on success.
pub async fn run_cli(cli: Cli) -> Result<()> {
    let client = ApiClient::new(cli.url);
    match cli.cmd {
        Cmd::List => {
            let jobs = client.list_jobs().await?;
            println!("{:<28} {:<40} STATE", "ID", "NAME");
            for j in jobs {
                println!(
                    "{:<28} {:<40} {}",
                    j.id,
                    j.name,
                    if j.paused { "paused" } else { "active" }
                );
            }
        }
        Cmd::Apply { file } => {
            let spec = load_spec(&file)?;
            let resp = client.create_job(&spec).await?;
            let id = resp["job"]["id"]
                .as_str()
                .ok_or_else(|| anyhow!("server response missing job.id"))?;
            println!("{id}");
        }
        Cmd::Update { target, file } => {
            let id = client.resolve(&target).await?;
            let spec = load_spec(&file)?;
            let resp = client.update_job(&id, &spec).await?;
            let name = resp["job"]["name"].as_str().unwrap_or("(updated)");
            println!("{id}\t{name}\tupdated");
        }
        Cmd::Show { target } => {
            let id = client.resolve(&target).await?;
            let job = client.get_job(&id).await?;
            println!("{}", serde_json::to_string_pretty(&job)?);
        }
        Cmd::Delete { target } => {
            let id = client.resolve(&target).await?;
            client.delete_job(&id).await?;
            println!("{id}\tdeleted");
        }
        Cmd::Trigger { target } => {
            let id = client.resolve(&target).await?;
            let run = client.trigger(&id).await?;
            println!(
                "{}",
                run["id"].as_str().unwrap_or("(no run id in response)")
            );
        }
        Cmd::Pause { target } => {
            let id = client.resolve(&target).await?;
            client.pause(&id).await?;
            println!("{id}\tpaused");
        }
        Cmd::Resume { target } => {
            let id = client.resolve(&target).await?;
            client.resume(&id).await?;
            println!("{id}\tresumed");
        }
        Cmd::Runs { target } => {
            let id = client.resolve(&target).await?;
            let runs = client.runs_for(&id).await?;
            println!(
                "{:<28} {:<10} {:<8} {:<26} {:<26} EXIT",
                "RUN_ID", "STATE", "ATTEMPT", "STARTED", "FINISHED"
            );
            for r in runs {
                println!(
                    "{:<28} {:<10?} {:<8} {:<26} {:<26} {}",
                    r.id,
                    r.state,
                    r.attempt,
                    r.started_at
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_else(|| "-".into()),
                    r.finished_at
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_else(|| "-".into()),
                    r.exit_code
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "-".into()),
                );
            }
        }
        Cmd::Sendevent { target, event } => {
            let id = client.resolve(&target).await?;
            match event.to_ascii_uppercase().as_str() {
                "STARTJOB" | "START" | "FORCE_STARTJOB" => {
                    let _ = client.trigger(&id).await?;
                    println!("{id}\tSTARTJOB");
                }
                "KILLJOB" | "KILL" => {
                    // Kill semantics requires per-run kill API (deferred to M4-full).
                    // For now: pause to stop scheduling; explicit run kill is TBD.
                    client.pause(&id).await?;
                    println!("{id}\tKILLJOB (paused; live-run kill API is M4-full)");
                }
                "ON_HOLD" | "HOLD" | "OFF_ICE" => {
                    client.pause(&id).await?;
                    println!("{id}\tON_HOLD");
                }
                "OFF_HOLD" | "UNHOLD" | "ON_ICE" => {
                    client.resume(&id).await?;
                    println!("{id}\tOFF_HOLD");
                }
                other => {
                    return Err(anyhow!(
                        "unknown event {other:?}; supported: STARTJOB, KILLJOB, ON_HOLD, OFF_HOLD"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn load_spec(path: &str) -> Result<serde_json::Value> {
    let contents = std::fs::read_to_string(path)?;
    let v: serde_json::Value = if path.ends_with(".json") {
        serde_json::from_str(&contents)?
    } else {
        serde_yaml::from_str(&contents)?
    };
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_list() {
        let cli = Cli::try_parse_from(["rusty-sched-cli", "list"]).unwrap();
        assert!(matches!(cli.cmd, Cmd::List));
    }

    #[test]
    fn parses_apply() {
        let cli = Cli::try_parse_from(["rusty-sched-cli", "apply", "-f", "job.yaml"]).unwrap();
        match cli.cmd {
            Cmd::Apply { file } => assert_eq!(file, "job.yaml"),
            _ => panic!("expected apply"),
        }
    }

    #[test]
    fn parses_show_by_name() {
        let cli = Cli::try_parse_from(["rusty-sched-cli", "show", "nightly-etl"]).unwrap();
        match cli.cmd {
            Cmd::Show { target } => assert_eq!(target, "nightly-etl"),
            _ => panic!("expected show"),
        }
    }

    #[test]
    fn parses_sendevent() {
        let cli =
            Cli::try_parse_from(["rusty-sched-cli", "sendevent", "myjob", "STARTJOB"]).unwrap();
        match cli.cmd {
            Cmd::Sendevent { target, event } => {
                assert_eq!(target, "myjob");
                assert_eq!(event, "STARTJOB");
            }
            _ => panic!("expected sendevent"),
        }
    }

    #[test]
    fn url_env_default() {
        let cli = Cli::try_parse_from(["rusty-sched-cli", "list"]).unwrap();
        assert_eq!(cli.url, "http://localhost:8080");
    }

    #[test]
    fn url_override() {
        let cli = Cli::try_parse_from(["rusty-sched-cli", "--url", "http://x:1", "list"]).unwrap();
        assert_eq!(cli.url, "http://x:1");
    }
}
