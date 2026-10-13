//! Underlane's platform-independent core: cloudflared supervision, config handling, the
//! inspecting proxy and storage. Nothing in here depends on Tauri.

pub mod cloudflared;
pub mod cli;
pub mod config;
pub mod db;
pub mod decode;
pub mod derived;
pub mod import;
pub mod ingress;
pub mod model;
pub mod proxy;
pub mod platform;
pub mod secret;
pub mod shim;
pub mod supervisor;
