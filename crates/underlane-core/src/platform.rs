//! The operating-system boundary for cloudflared children and process discovery.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::{Duration, Instant};

use tokio::process::{Child, Command};

pub const SIGTERM: i32 = 15;
pub const SIGKILL: i32 = 9;

#[derive(Clone)]
pub struct Process {
    pub pid: u32,
    pub command: String,
}

impl fmt::Debug for Process {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Process").field("pid", &self.pid).field("command", &crate::secret::redact_text(&self.command)).finish()
    }
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

pub fn process_rows(output: &str) -> Vec<Process> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let end = line.find(char::is_whitespace)?;
            let pid = line[..end].parse().ok()?;
            let command = line[end..].trim_start();
            (!command.is_empty()).then(|| Process { pid, command: command.to_string() })
        })
        .collect()
}

#[cfg(unix)]
pub fn processes() -> io::Result<Vec<Process>> {
    let output = std::process::Command::new("ps").args(["-axww", "-o", "pid=,command="]).output()?;
    if !output.status.success() {
        return Err(io::Error::other("process listing failed"));
    }
    Ok(process_rows(&String::from_utf8_lossy(&output.stdout)))
}

#[cfg(not(unix))]
pub fn processes() -> io::Result<Vec<Process>> {
    Err(unsupported())
}

pub fn configure_group(command: &mut Command) -> io::Result<()> {
    #[cfg(unix)]
    {
        command.process_group(0);
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = command;
        Err(unsupported())
    }
}

pub fn signal(pid: u32, signal: i32) -> io::Result<()> {
    signal_pid(pid, signal, false)
}

pub fn signal_group(pid: u32, signal: i32) -> io::Result<()> {
    signal_pid(pid, signal, true)
}

fn signal_pid(pid: u32, signal: i32, group: bool) -> io::Result<()> {
    // Never let an invalid pid become kill(0) or kill(-1).
    let pid =
        i32::try_from(pid).ok().filter(|pid| *pid > 1).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid process id"))?;
    #[cfg(unix)]
    {
        let target = if group { -pid } else { pid };
        if unsafe { libc::kill(target, signal) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, signal, group);
        Err(unsupported())
    }
}

/// SIGTERM the process group, wait for a graceful exit, then SIGKILL and reap.
pub async fn kill_tree(child: &mut Child, grace: Duration) {
    if let Some(pid) = child.id() {
        let _ = signal_group(pid, SIGTERM);
        if matches!(tokio::time::timeout(grace, child.wait()).await, Ok(Ok(_))) {
            return;
        }
        let _ = signal_group(pid, SIGKILL);
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

/// Clean up descendants after the direct child (possibly a shim) has exited.
pub async fn sweep_group(pid: Option<u32>, grace: Duration) {
    let Some(pid) = pid else { return };
    let alive = || signal_group(pid, 0).is_ok();
    if !alive() {
        return;
    }
    let _ = signal_group(pid, SIGTERM);
    let deadline = Instant::now() + grace;
    while alive() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if alive() {
        let _ = signal_group(pid, SIGKILL);
    }
}

/// Only stop stale children whose command still references this app's data directory.
pub fn reap_orphans(data_dir: &Path) -> Vec<u32> {
    let mut killed = Vec::new();
    let Ok(entries) = std::fs::read_dir(data_dir.join("run")) else { return killed };
    let marker = data_dir.to_string_lossy();
    let processes = processes().unwrap_or_default();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("pid") {
            continue;
        }
        let pid = std::fs::read_to_string(&path).ok().and_then(|content| content.trim().parse::<u32>().ok());
        if let Some(process) = processes.iter().find(|process| Some(process.pid) == pid) {
            if process.command.contains("cloudflared") && process.command.contains(marker.as_ref()) {
                let _ = signal_group(process.pid, SIGTERM);
                let _ = signal(process.pid, SIGTERM);
                killed.push(process.pid);
            }
        }
        let _ = std::fs::remove_file(path);
    }
    // Pidfiles can name a dead shim while its cloudflared child is still running.
    for process in processes {
        let executable = process.command.split_whitespace().next().unwrap_or("");
        let is_cloudflared = Path::new(executable).file_name().and_then(|name| name.to_str()) == Some("cloudflared");
        if is_cloudflared
            && process.command.contains(marker.as_ref())
            && !killed.contains(&process.pid)
            && process.pid != std::process::id()
        {
            let _ = signal(process.pid, SIGTERM);
            killed.push(process.pid);
        }
    }
    if !killed.is_empty() {
        std::thread::sleep(Duration::from_millis(800));
        for &pid in &killed {
            if signal(pid, 0).is_ok() {
                let _ = signal_group(pid, SIGKILL);
                let _ = signal(pid, SIGKILL);
            }
        }
    }
    killed
}

/// A handler rather than SIG_IGN lets cloudflared receive group shutdown signals after exec.
pub fn ignore_shim_signals() -> io::Result<()> {
    #[cfg(unix)]
    unsafe {
        extern "C" fn noop(_: libc::c_int) {}
        let handler = noop as extern "C" fn(libc::c_int) as libc::sighandler_t;
        for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            if libc::signal(signal, handler) == libc::SIG_ERR {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        Err(unsupported())
    }
}

pub fn exit_code(status: ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    status.code().unwrap_or(1)
}

#[cfg(not(unix))]
fn unsupported() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "cloudflared process management is unsupported on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_rows_preserve_argv_for_matching_but_debug_is_redacted() {
        let rows = process_rows(" 42 /usr/bin/cloudflared tunnel run --token invented-platform-token\n\t73\tcloudflared --credentials-contents {\"TunnelSecret\": \"invented-platform-credential\"}\ninvalid row\n99\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].pid, 42);
        assert!(rows[0].command.contains("invented-platform-token"));
        assert!(!format!("{rows:?}").contains("invented-platform-"));
    }

    #[test]
    fn invalid_pids_never_signal_other_processes() {
        for pid in [0, 1, u32::MAX] {
            assert_eq!(signal(pid, 0).unwrap_err().kind(), io::ErrorKind::InvalidInput);
            assert_eq!(signal_group(pid, 0).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        }
    }
}
