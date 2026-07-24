//! # odr-inbox
//!
//! Reads the user's mailbox over IMAP to find and open broker confirmation
//! links — the last big source of manual work in a removal run.
//!
//! Brokers email a link that expires in 24–48h, and a full run can generate a
//! dozen of them. Clicking each by hand is exactly the chore ODR exists to
//! remove. Given read access to a mailbox, the engine can do it.
//!
//! **Privacy posture.** Credentials and mail never leave the machine; this
//! crate talks only to the user's own IMAP server. Prefer
//! [`InboxConfig::password_command`] over a plaintext password, and prefer a
//! dedicated address — brokers only ever see the address you opt out with, so
//! it need not be your main one.
//!
//! **Safety posture.** ODR only ever opens a link on the broker's *own* domain
//! (see [`links::find_confirmation_link`]), so a spoofed email can't redirect
//! it somewhere else, and it refuses obvious non-actions like unsubscribe
//! links.

pub mod links;

use std::io::Read;
use std::process::Command;

use chrono::{Duration, Utc};
use odr_engine::profile::InboxConfig;

pub use links::{domain_of, find_confirmation_link};

/// Something that went wrong talking to the mailbox.
#[derive(Debug, thiserror::Error)]
pub enum InboxError {
    #[error("no password configured: set `password` or `password_command` under `inbox`")]
    NoPassword,

    #[error("password command `{cmd}` failed: {reason}")]
    PasswordCommand { cmd: String, reason: String },

    #[error("could not reach {host}:{port}: {source}")]
    Connect {
        host: String,
        port: u16,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("IMAP login failed for {user} (for Gmail/Outlook use an app password): {reason}")]
    Login { user: String, reason: String },

    #[error("IMAP error: {0}")]
    Imap(String),
}

/// A confirmation link found in the mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundLink {
    pub broker_id: String,
    pub url: String,
    pub subject: String,
}

/// A live IMAP session over TLS.
pub struct Inbox {
    session: imap::Session<native_tls::TlsStream<std::net::TcpStream>>,
    mailbox: String,
}

impl Inbox {
    /// Connect and authenticate using the profile's inbox settings.
    pub fn connect(config: &InboxConfig) -> Result<Self, InboxError> {
        let password = resolve_password(config)?;

        let tls = native_tls::TlsConnector::builder()
            .build()
            .map_err(|e| InboxError::Connect {
                host: config.imap_host.clone(),
                port: config.imap_port,
                source: Box::new(e),
            })?;

        let client = imap::connect(
            (config.imap_host.as_str(), config.imap_port),
            &config.imap_host,
            &tls,
        )
        .map_err(|e| InboxError::Connect {
            host: config.imap_host.clone(),
            port: config.imap_port,
            source: Box::new(e),
        })?;

        let mut session = client
            .login(&config.username, &password)
            .map_err(|(e, _)| InboxError::Login {
                user: config.username.clone(),
                reason: e.to_string(),
            })?;

        session
            .select(&config.mailbox)
            .map_err(|e| InboxError::Imap(e.to_string()))?;

        Ok(Self {
            session,
            mailbox: config.mailbox.clone(),
        })
    }

    /// Search recent mail for a confirmation link belonging to `broker_id`,
    /// whose site lives at `homepage`.
    ///
    /// Only messages from the last `since_hours` are considered, matching the
    /// window in which broker links are still valid.
    pub fn find_link(
        &mut self,
        broker_id: &str,
        homepage: &str,
        since_hours: i64,
    ) -> Result<Option<FoundLink>, InboxError> {
        let domain = domain_of(homepage);
        let since = (Utc::now() - Duration::hours(since_hours))
            .format("%d-%b-%Y")
            .to_string();

        let ids = self
            .session
            .search(format!("SINCE {since}"))
            .map_err(|e| InboxError::Imap(e.to_string()))?;
        if ids.is_empty() {
            return Ok(None);
        }

        // Newest first — the freshest link is the one still valid.
        let mut ids: Vec<u32> = ids.into_iter().collect();
        ids.sort_unstable_by(|a, b| b.cmp(a));

        for id in ids.into_iter().take(200) {
            let messages = self
                .session
                .fetch(id.to_string(), "RFC822")
                .map_err(|e| InboxError::Imap(e.to_string()))?;

            for message in messages.iter() {
                let Some(body) = message.body() else { continue };
                let Ok(parsed) = mailparse::parse_mail(body) else {
                    continue;
                };

                let subject = parsed
                    .headers
                    .iter()
                    .find(|h| h.get_key().eq_ignore_ascii_case("subject"))
                    .map(|h| h.get_value())
                    .unwrap_or_default();

                let text = collect_text(&parsed);
                if let Some(url) = find_confirmation_link(&text, &domain) {
                    return Ok(Some(FoundLink {
                        broker_id: broker_id.to_string(),
                        url,
                        subject,
                    }));
                }
            }
        }
        Ok(None)
    }

    /// The mailbox being searched.
    pub fn mailbox(&self) -> &str {
        &self.mailbox
    }

    /// Close the session cleanly.
    pub fn logout(&mut self) {
        let _ = self.session.logout();
    }
}

/// Flatten a message into searchable text, including all MIME parts so a link
/// in either the plain-text or HTML alternative is found.
fn collect_text(mail: &mailparse::ParsedMail) -> String {
    let mut out = mail.get_body().unwrap_or_default();
    for part in &mail.subparts {
        out.push('\n');
        out.push_str(&collect_text(part));
    }
    out
}

/// Get the password, preferring a command over a stored secret.
fn resolve_password(config: &InboxConfig) -> Result<String, InboxError> {
    if let Some(cmd) = &config.password_command {
        let output = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .map_err(|e| InboxError::PasswordCommand {
                cmd: cmd.clone(),
                reason: e.to_string(),
            })?;
        if !output.status.success() {
            return Err(InboxError::PasswordCommand {
                cmd: cmd.clone(),
                reason: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        let mut password = String::new();
        output
            .stdout
            .as_slice()
            .read_to_string(&mut password)
            .map_err(|e| InboxError::PasswordCommand {
                cmd: cmd.clone(),
                reason: e.to_string(),
            })?;
        return Ok(password.trim_end_matches(['\n', '\r']).to_string());
    }

    config.password.clone().ok_or(InboxError::NoPassword)
}
