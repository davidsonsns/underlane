//! Underlane's platform-independent core: cloudflared supervision, config handling, the
//! inspecting proxy and storage. Nothing in here depends on Tauri.

pub mod cloudflared;
pub mod cli;
pub mod config;
pub mod connection;
pub mod db;
pub mod decode;
pub mod derived;
pub mod dns;
pub mod import;
pub mod hostnames;
pub mod model;
pub mod proxy;
pub mod platform;
pub mod projects;
pub mod route_control;
pub mod secret;
pub mod shim;
pub mod supervisor;
pub mod tunnel_setup;
pub mod separate_tunnel;
