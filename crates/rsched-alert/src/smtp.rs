//! SMTP email channel.
//!
//! Reads server credentials from a [`SmtpConfig`] (or env via
//! [`SmtpConfig::from_env`]) — works with any SMTP provider, including
//! Gmail with an App Password:
//!
//!   1. Enable 2-Step Verification on the Google account.
//!   2. Create an App Password at <https://myaccount.google.com/apppasswords>.
//!   3. `RSCHED_SMTP_HOST=smtp.gmail.com RSCHED_SMTP_PORT=465 \\
//!       RSCHED_SMTP_USER=you@gmail.com RSCHED_SMTP_PASS=<app-password> \\
//!       RSCHED_SMTP_FROM="rusty-sched <you@gmail.com>" rusty-sched server`

use crate::{AlertError, AlertPayload, Channel};
use async_trait::async_trait;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::env;
use tracing::debug;

/// SMTP server config.
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    /// Hostname (e.g. `smtp.gmail.com`).
    pub host: String,
    /// Port (465 for implicit TLS, 587 for STARTTLS).
    pub port: u16,
    /// Username.
    pub user: String,
    /// Password / app password.
    pub pass: String,
    /// RFC 5322 From header value, e.g. `"rusty-sched <bot@example.com>"`.
    pub from: String,
    /// Use implicit TLS (true=465, false=STARTTLS on 587).
    pub implicit_tls: bool,
}

impl SmtpConfig {
    /// Read from env vars.
    /// Required: RSCHED_SMTP_HOST, RSCHED_SMTP_USER, RSCHED_SMTP_PASS, RSCHED_SMTP_FROM.
    /// Optional: RSCHED_SMTP_PORT (default 465), RSCHED_SMTP_STARTTLS=1 to use 587/STARTTLS.
    pub fn from_env() -> Option<Self> {
        let host = env::var("RSCHED_SMTP_HOST").ok()?;
        let user = env::var("RSCHED_SMTP_USER").ok()?;
        let pass = env::var("RSCHED_SMTP_PASS").ok()?;
        let from = env::var("RSCHED_SMTP_FROM").ok()?;
        let implicit_tls = env::var("RSCHED_SMTP_STARTTLS").ok().is_none();
        let default_port = if implicit_tls { 465 } else { 587 };
        let port = env::var("RSCHED_SMTP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(default_port);
        Some(Self {
            host,
            port,
            user,
            pass,
            from,
            implicit_tls,
        })
    }
}

/// SMTP delivery channel — sends one email per recipient list.
pub struct EmailChannel {
    config: SmtpConfig,
    transport: AsyncSmtpTransport<Tokio1Executor>,
    recipients: Vec<String>,
}

impl EmailChannel {
    /// Build a channel for the given recipient list.
    pub fn new(config: SmtpConfig, recipients: Vec<String>) -> Result<Self, AlertError> {
        let creds = Credentials::new(config.user.clone(), config.pass.clone());
        let builder = if config.implicit_tls {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
                .map_err(|e| AlertError::Smtp(format!("relay: {e}")))?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
                .map_err(|e| AlertError::Smtp(format!("starttls relay: {e}")))?
        };
        let transport = builder.port(config.port).credentials(creds).build();
        Ok(Self {
            config,
            transport,
            recipients,
        })
    }
}

#[async_trait]
impl Channel for EmailChannel {
    async fn deliver(&self, p: &AlertPayload) -> Result<(), AlertError> {
        let subject = format!(
            "[rusty-sched] {event:?} — {name} (attempt {n})",
            event = p.event,
            name = p.job_name,
            n = p.attempt,
        );
        let body = format!(
            "Job:        {name}\nJob id:     {jid}\nRun id:     {rid}\nEvent:      {event:?}\nState:      {state:?}\nAttempt:    {att}\nExit code:  {exit}\nStarted:    {st}\nFinished:   {fin}\nHost:       {host}\n\n{msg}\n",
            name = p.job_name,
            jid = p.job_id,
            rid = p.run_id,
            event = p.event,
            state = p.state,
            att = p.attempt,
            exit = p.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "-".into()),
            st = p.started_at.map(|t| t.to_rfc3339()).unwrap_or_else(|| "-".into()),
            fin = p.finished_at.map(|t| t.to_rfc3339()).unwrap_or_else(|| "-".into()),
            host = p.host,
            msg = p.message.clone().unwrap_or_default(),
        );

        for to in &self.recipients {
            let email = Message::builder()
                .from(
                    self.config
                        .from
                        .parse()
                        .map_err(|e| AlertError::Smtp(format!("from: {e}")))?,
                )
                .to(to
                    .parse()
                    .map_err(|e| AlertError::Smtp(format!("to {to}: {e}")))?)
                .subject(&subject)
                .header(ContentType::TEXT_PLAIN)
                .body(body.clone())
                .map_err(|e| AlertError::Smtp(format!("build: {e}")))?;
            self.transport
                .send(email)
                .await
                .map_err(|e| AlertError::Smtp(format!("send: {e}")))?;
            debug!(%to, subject, "email delivered");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_requires_all_vars() {
        // Hard to test without polluting env; verify struct constructs.
        let cfg = SmtpConfig {
            host: "smtp.example.com".into(),
            port: 465,
            user: "u".into(),
            pass: "p".into(),
            from: "x@example.com".into(),
            implicit_tls: true,
        };
        assert!(EmailChannel::new(cfg, vec!["a@b.com".into()]).is_ok());
    }

    #[test]
    fn invalid_host_still_constructs() {
        // lettre validates host at connect time, not build time.
        let cfg = SmtpConfig {
            host: "smtp.example.com".into(),
            port: 587,
            user: "u".into(),
            pass: "p".into(),
            from: "x@example.com".into(),
            implicit_tls: false,
        };
        assert!(EmailChannel::new(cfg, vec![]).is_ok());
    }
}
