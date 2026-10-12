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

pub const SHIM_FLAG: &str = "--cloudflared-shim";

/// Run the shim. `args[0]` is the cloudflared binary. Returns the exit code to use.
pub fn run(args: &[String]) -> i32 {
    let Some((bin, rest)) = args.split_first() else {
        eprintln!("underlane shim: missing cloudflared path");
        return 2;
    };

    #[cfg(unix)]
    unsafe {
        // A no-op handler (not SIG_IGN, which would be inherited across exec) keeps the shim
        // alive while cloudflared shuts down gracefully after a group-wide SIGTERM/SIGINT.
        extern "C" fn noop(_: libc::c_int) {}
        let handler = noop as extern "C" fn(libc::c_int) as libc::sighandler_t;
        libc::signal(libc::SIGTERM, handler);
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGHUP, handler);
    }

    let mut child = match Command::new(bin).args(rest).stdin(Stdio::null()).spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{{\"level\":\"fatal\",\"message\":\"could not start {bin}: {e}\"}}");
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
        #[cfg(unix)]
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            #[cfg(unix)]
            unsafe {
                if libc::kill(pid as i32, 0) != 0 {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        #[cfg(unix)]
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    });

    loop {
        match child.wait() {
            Ok(status) => {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    if let Some(sig) = status.signal() {
                        return 128 + sig;
                    }
                }
                return status.code().unwrap_or(1);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return 1,
        }
    }
}
