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
    /// Apply a job spec from a YAML or JSON file.
    Apply {
        /// Spec file path.
        #[arg(short, long)]
        file: String,
    },
    /// Trigger a job manually by id.
    Trigger {
        /// Job id (ULID).
        id: String,
    },
    /// Pause a job by id.
    Pause {
        /// Job id (ULID).
        id: String,
    },
    /// Resume a paused job.
    Resume {
        /// Job id (ULID).
        id: String,
    },
}

/// Execute the CLI: returns Ok(()) on success.
pub async fn run_cli(cli: Cli) -> Result<()> {
    let client = ApiClient::new(cli.url);
    match cli.cmd {
        Cmd::List => {
            let jobs = client.list_jobs().await?;
            for j in jobs {
                println!(
                    "{}\t{}\t{}",
                    j.id,
                    j.name,
                    if j.paused { "paused" } else { "active" }
                );
            }
        }
        Cmd::Apply { file } => {
            let contents = std::fs::read_to_string(&file)?;
            let spec: serde_json::Value = if file.ends_with(".json") {
                serde_json::from_str(&contents)?
            } else {
                serde_yaml::from_str(&contents)?
            };
            let resp = client.create_job(&spec).await?;
            let id = resp["job"]["id"]
                .as_str()
                .ok_or_else(|| anyhow!("server response missing job.id"))?;
            println!("{id}");
        }
        Cmd::Trigger { id } => {
            let run = client.trigger(&id).await?;
            println!(
                "{}",
                run["id"].as_str().unwrap_or("(no run id in response)")
            );
        }
        Cmd::Pause { id } => client.pause(&id).await?,
        Cmd::Resume { id } => client.resume(&id).await?,
    }
    Ok(())
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
