//! Persistent app model shared by storage, the supervisor and the UI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub sort: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TunnelKind {
    /// Named tunnel run from a local config file.
    Config,
    /// Remotely managed tunnel run with a token.
    Token,
    /// Account-less `trycloudflare.com` tunnel for a local URL.
    Quick,
}

impl TunnelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TunnelKind::Config => "config",
            TunnelKind::Token => "token",
            TunnelKind::Quick => "quick",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "config" => Some(TunnelKind::Config),
            "token" => Some(TunnelKind::Token),
            "quick" => Some(TunnelKind::Quick),
            _ => None,
        }
    }

    pub fn supports_inspection(self) -> bool {
        !matches!(self, TunnelKind::Token)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tunnel {
    pub id: String,
    pub group_id: Option<String>,
    pub name: String,
    pub kind: TunnelKind,
    /// Config file (kind = config).
    pub config_path: Option<String>,
    /// Local origin URL (kind = quick).
    pub origin_url: Option<String>,
    /// Inspect requests through Underlane's proxy (captures bodies). On by default for new projects.
    pub inspect: bool,
    /// Start when Underlane launches.
    pub autostart: bool,
    /// cloudflared --loglevel.
    pub loglevel: String,
    pub created_at: i64,
    pub sort: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Explicit cloudflared binary; None = auto-detect.
    pub cloudflared_path: Option<String>,
    pub log_ttl_days: u32,
    pub max_db_mb: u32,
    pub max_body_kb: u32,
    pub redact_headers: Vec<String>,
    /// Extra query parameter names to redact in captured URLs and logs.
    pub redact_query: Vec<String>,
    /// "system" | "light" | "dark"
    pub theme: String,
    pub default_loglevel: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            cloudflared_path: None,
            log_ttl_days: 7,
            max_db_mb: 256,
            max_body_kb: 1024,
            redact_headers: Vec::new(),
            redact_query: Vec::new(),
            theme: "system".into(),
            default_loglevel: "info".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogLine {
    pub id: i64,
    pub tunnel_id: String,
    pub ts: i64,
    pub level: String,
    pub message: String,
    /// Extra structured fields as a JSON object string ("{}" when none).
    pub fields: String,
}
