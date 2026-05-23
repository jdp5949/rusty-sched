//! clap subcommand definitions + dispatcher.

use crate::ApiClient;
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use rsched_jil::{parse as jil_parse, JilBlock};

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
    /// Parse an Autosys JIL file and apply each block to the cluster.
    Jil {
        /// Path to the JIL file.
        #[arg(short, long)]
        file: String,
    },
    /// Autosys-compat `autorep`: -J <name> prints a job + recent runs;
    /// -A lists all jobs.
    Autorep {
        /// `-J <name>` to inspect a single job; `-A` to list all jobs.
        #[arg(short = 'J', long, conflicts_with = "all")]
        job: Option<String>,
        /// List all jobs (`autorep -A`).
        #[arg(short = 'A', long)]
        all: bool,
    },
    /// Manage Autosys-style global variables (used by `value(name)` conditions).
    Global {
        /// `list` / `set` / `delete` subcommand.
        #[command(subcommand)]
        cmd: GlobalCmd,
    },
}

/// Subcommands for `global`.
#[derive(Debug, Subcommand)]
pub enum GlobalCmd {
    /// List every global as `name=value`.
    List,
    /// Set a global value (creates or overwrites).
    Set {
        /// Global variable name.
        name: String,
        /// Value to store (string).
        value: String,
    },
    /// Delete a global.
    Delete {
        /// Global variable name.
        name: String,
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
        Cmd::Jil { file } => {
            let input = std::fs::read_to_string(&file)?;
            let blocks = jil_parse(&input).map_err(|e| anyhow!("JIL parse error: {e}"))?;
            for block in blocks {
                match block {
                    JilBlock::Insert(spec) => {
                        let name = spec.name.clone();
                        let out = spec
                            .into_job()
                            .map_err(|e| anyhow!("translate error for {name}: {e}"))?;
                        for w in &out.warnings {
                            eprintln!("warn [{name}]: {w}");
                        }
                        let body = serde_json::to_value(&out.job)?;
                        let resp = client.create_job(&body).await?;
                        let id = resp["job"]["id"].as_str().unwrap_or("(unknown)");
                        println!("inserted: {name} ({id})");
                    }
                    JilBlock::Update(name, partial) => {
                        let id = client.resolve(&name).await?;
                        let patch = serde_json::to_value(&partial)?;
                        client.update_job(&id, &patch).await?;
                        println!("updated: {name} ({id})");
                    }
                    JilBlock::Delete(name) => {
                        let id = client.resolve(&name).await?;
                        client.delete_job(&id).await?;
                        println!("deleted: {name} ({id})");
                    }
                }
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
                    // Find the most recent running run and kill it.
                    let runs = client.runs_for(&id).await?;
                    let running = runs
                        .iter()
                        .find(|r| r.state == rsched_core::RunState::Running);
                    match running {
                        Some(r) => {
                            let run_id = r.id.to_string();
                            client.kill_run(&run_id).await?;
                            println!("killed run {run_id}");
                        }
                        None => {
                            return Err(anyhow!("no running run found for job {}", target));
                        }
                    }
                }
                "ON_HOLD" | "HOLD" | "OFF_ICE" => {
                    client.pause(&id).await?;
                    println!("{id}\tON_HOLD");
                }
                "OFF_HOLD" | "UNHOLD" | "ON_ICE" => {
                    client.resume(&id).await?;
                    println!("{id}\tOFF_HOLD");
                }
                "SET_GLOBAL" => {
                    return Err(anyhow!(
                        "use `rusty-sched cli global set NAME VALUE` instead of `sendevent SET_GLOBAL`"
                    ));
                }
                ev if ev.starts_with("CHANGE_STATUS=") => {
                    let state = &ev["CHANGE_STATUS=".len()..];
                    // Apply to the most recent run.
                    let runs = client.runs_for(&id).await?;
                    let r = runs
                        .first()
                        .ok_or_else(|| anyhow!("no runs found for job {}", target))?;
                    let run_id = r.id.to_string();
                    client.change_run_state(&run_id, state).await?;
                    println!("{run_id}\tCHANGE_STATUS={state}");
                }
                ev if ev == "SEND_SIGNAL" || ev.starts_with("SEND_SIGNAL=") => {
                    return Err(anyhow!(
                        "SEND_SIGNAL is not yet supported; only KILLJOB (SIGKILL) is available"
                    ));
                }
                other => {
                    return Err(anyhow!(
                        "unknown event {other:?}; supported: STARTJOB, KILLJOB, ON_HOLD, OFF_HOLD, CHANGE_STATUS=<state>, SET_GLOBAL"
                    ));
                }
            }
        }
        Cmd::Autorep { job, all } => {
            if all || job.is_none() {
                // `autorep -A`: list every job + most recent run summary.
                let jobs = client.list_jobs().await?;
                println!(
                    "{:<28} {:<10} {:<10} {:<26}",
                    "JOB_NAME", "STATE", "LAST_RUN", "NEXT_FIRE"
                );
                for j in jobs {
                    let runs = client.runs_for(&j.id.to_string()).await.unwrap_or_default();
                    let last = runs
                        .first()
                        .map(|r| format!("{:?}", r.state))
                        .unwrap_or_else(|| "-".into());
                    println!(
                        "{:<28} {:<10} {:<10} {:<26}",
                        j.name,
                        if j.paused { "ON_HOLD" } else { "active" },
                        last,
                        j.next_fire_at
                            .map(|t| t.to_rfc3339())
                            .unwrap_or_else(|| "-".into()),
                    );
                }
            } else if let Some(name) = job {
                let id = client.resolve(&name).await?;
                let j = client.get_job(&id).await?;
                println!("Job: {}", j.name);
                println!("  id:           {}", j.id);
                println!(
                    "  state:        {}",
                    if j.paused { "ON_HOLD" } else { "active" }
                );
                println!("  cmd:          {}", j.cmd);
                println!(
                    "  next_fire_at: {}",
                    j.next_fire_at
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_else(|| "-".into())
                );
                println!("Recent runs:");
                let runs = client.runs_for(&id).await?;
                for r in runs.iter().take(20) {
                    println!(
                        "  {}  {:?}  exit={}  started={}",
                        r.id,
                        r.state,
                        r.exit_code
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "-".into()),
                        r.started_at
                            .map(|t| t.to_rfc3339())
                            .unwrap_or_else(|| "-".into()),
                    );
                }
            }
        }
        Cmd::Global { cmd } => match cmd {
            GlobalCmd::List => {
                let rows = client.list_globals().await?;
                for (n, v, _) in rows {
                    println!("{n}={v}");
                }
            }
            GlobalCmd::Set { name, value } => {
                client.set_global(&name, &value).await?;
                println!("{name}\tset");
            }
            GlobalCmd::Delete { name } => {
                client.delete_global(&name).await?;
                println!("{name}\tdeleted");
            }
        },
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
