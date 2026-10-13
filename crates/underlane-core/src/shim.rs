//! Watchdog shim: `underlane --cloudflared-shim <cloudflared> <args...>`.
//!
//! macOS doesn't kill children when their parent dies, so Underlane starts cloudflared through
//! this shim. The shim's stdin is a pipe from the app. When it reaches EOF the app is gone
//! (crash, force quit), and the shim terminates cloudflared. stdout/stderr are inherited, so
//! log lines go straight to the app. Normal stops signal the whole process group; the shim
//! ignores the signal itself and exits with cloudflared's status once it has shut down.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::platform;

pub const SHIM_FLAG: &str = "--cloudflared-shim";

/// Run the shim. `args[0]` is the cloudflared binary. Returns the exit code to use.
pub fn run(args: &[String]) -> i32 {
    let Some((bin, rest)) = args.split_first() else {
        eprintln!("underlane shim: missing cloudflared path");
        return 2;
    };

    if let Err(error) = platform::ignore_shim_signals() {
        eprintln!("underlane shim: {error}");
        return 2;
    }

    let mut child = match Command::new(bin).args(rest).stdin(Stdio::null()).spawn() {
        Ok(c) => c,
        Err(e) => {
            let message = crate::secret::redact_text(&format!("could not start {bin}: {e}"));
            eprintln!("{}", serde_json::json!({ "level": "fatal", "message": message }));
            return 127;
        }
    };
    let pid = child.id();

    // Parent-death watch: block on stdin until EOF.
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 64];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => continue,
            }
        }
        let _ = platform::signal(pid, platform::SIGTERM);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if platform::signal(pid, 0).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = platform::signal(pid, platform::SIGKILL);
    });

    loop {
        match child.wait() {
            Ok(status) => return platform::exit_code(status),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return 1,
        }
    }
}
