//! Offline real-binary contracts. Run explicitly with:
//! `cargo test -p underlane-core --test cli_contract -- --ignored --nocapture`
//! Optional binary override: `CLOUDFLARED_CONTRACT_BIN=/absolute/path/cloudflared`.

use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::Command;

#[path = "common/fake_cloudflared.rs"]
#[allow(dead_code)]
mod fake_cloudflared;

fn binary() -> PathBuf {
    if let Some(path) = std::env::var_os("CLOUDFLARED_CONTRACT_BIN") {
        let path = PathBuf::from(path);
        assert!(path.is_absolute(), "CLOUDFLARED_CONTRACT_BIN must be absolute");
        return path;
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .chain([PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin")])
        .map(|dir| dir.join("cloudflared"))
        .find(|path| path.is_file())
        .expect("cloudflared unavailable; set CLOUDFLARED_CONTRACT_BIN")
        .canonicalize()
        .unwrap()
}

async fn run(bin: &Path, home: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn().unwrap();
    #[cfg(unix)]
    let pid = child.id().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let mut stdout_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await.unwrap();
        bytes
    });
    let mut stderr_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).await.unwrap();
        bytes
    });
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let status = child.wait().await.unwrap();
        let stdout = (&mut stdout_task).await.unwrap();
        let stderr = (&mut stderr_task).await.unwrap();
        Output { status, stdout, stderr }
    })
    .await;
    let output = match result {
        Ok(output) => output,
        Err(_) => {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL)
            };
            let _ = child.kill().await;
            let _ = child.wait().await;
            stdout_task.abort();
            stderr_task.abort();
            panic!("offline contract timed out: binary={bin:?}, argv={args:?}");
        }
    };
    eprintln!(
        "binary={bin:?}\nargv={args:?}\nexit={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

fn combined(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

#[tokio::test]
async fn shared_fake_dispatches_commands_and_rc_zero_refusals() {
    use fake_cloudflared::fake;
    let f = fake("ok");
    let out = run(&f.bin, &f.root, &["--version"]).await;
    assert!(out.status.success());
    assert!(combined(&out).contains("2026.9.1"));
    for args in [
        vec!["tunnel", "--config", "neutral.yml", "ingress", "validate"],
        vec!["tunnel", "--config", "neutral.yml", "list", "-o", "json"],
        vec!["tunnel", "--config", "neutral.yml", "info", "-o", "json", "synthetic"],
        vec!["tunnel", "--config", "neutral.yml", "create", "-o", "json", "synthetic"],
        vec!["tunnel", "--config", "neutral.yml", "route", "dns", "synthetic", "synthetic.example.com"],
        vec!["tunnel", "--config", "neutral.yml", "login"],
    ] {
        let out = run(&f.bin, &f.root, &args).await;
        assert!(out.status.success(), "{args:?}: {}", combined(&out));
        if args.contains(&"json") {
            serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap();
        }
    }
    for (mode, anchor) in [("unknown-flag", "flag provided but not defined"), ("login-refused", "existing certificate")] {
        let f = fake(mode);
        let out = run(&f.bin, &f.root, &["tunnel", "login"]).await;
        assert!(out.status.success(), "fixture must preserve real rc 0 behavior");
        assert!(combined(&out).contains(anchor));
    }
    let out = Command::new(&f.bin)
        .arg("--version")
        .env("FAKE_CLOUDFLARED_STDOUT", "synthetic output")
        .env("FAKE_CLOUDFLARED_STDERR", "synthetic error")
        .env("FAKE_CLOUDFLARED_EXIT_CODE", "7")
        .output()
        .await
        .unwrap();
    assert_eq!(out.status.code(), Some(7));
    assert_eq!(out.stdout, b"synthetic output");
    assert_eq!(out.stderr, b"synthetic error");
}

#[tokio::test]
#[ignore = "requires an installed real cloudflared; offline, isolated, read-only"]
async fn real_cloudflared_offline_contracts() {
    let bin = binary();
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir(home.path().join(".cloudflared")).unwrap();
    // A conflicting default proves every tunnel probe uses its explicit fixture config.
    std::fs::write(home.path().join(".cloudflared/config.yml"), "ingress:\n  - hostname: default.invalid\n    service: http_status:404\n")
        .unwrap();
    let valid = home.path().join("valid.yml");
    let invalid = home.path().join("invalid.yml");
    let neutral = home.path().join("neutral.yml");
    std::fs::write(
        &valid,
        "ingress:\n  - hostname: contract.example.invalid\n    service: http://localhost:43210\n  - service: http_status:404\n",
    )
    .unwrap();
    std::fs::write(&invalid, "ingress:\n  - hostname: contract.example.invalid\n    service: http://localhost:43210\n").unwrap();
    std::fs::write(&neutral, "{}\n").unwrap();
    let (valid, invalid, neutral) = (valid.to_str().unwrap(), invalid.to_str().unwrap(), neutral.to_str().unwrap());

    let out = run(&bin, home.path(), &["--version"]).await;
    assert!(out.status.success());
    let version = underlane_core::cloudflared::parse_version(&combined(&out)).expect("version contract");
    eprintln!("contract binary version={version}");

    let out = run(&bin, home.path(), &["tunnel", "--config", valid, "ingress", "validate"]).await;
    assert!(out.status.success(), "{}", combined(&out));
    assert!(combined(&out).lines().any(|line| line == "OK"));
    let out = run(&bin, home.path(), &["tunnel", "--config", invalid, "ingress", "validate"]).await;
    assert!(!out.status.success());
    assert!(combined(&out).contains("last ingress rule must match all URLs"));

    let json = r#"{"ingress":[{"hostname":"contract.example.invalid","service":"http://localhost:43210"},{"service":"http_status:404"}]}"#;
    let out = run(&bin, home.path(), &["tunnel", "--config", neutral, "ingress", "validate", "--json", json]).await;
    assert!(out.status.success(), "{}", combined(&out));
    assert!(combined(&out).lines().any(|line| line == "OK"));
    let out = run(&bin, home.path(), &["tunnel", "--config", neutral, "ingress", "validate", "--json", "{invalid-json}"]).await;
    assert!(!out.status.success());

    let out = run(&bin, home.path(), &["tunnel", "--config", valid, "ingress", "rule", "https://contract.example.invalid/path"]).await;
    assert!(out.status.success(), "{}", combined(&out));
    assert!(combined(&out).contains("Matched rule #0"));
    let out = run(&bin, home.path(), &["tunnel", "--config", valid, "ingress", "rule", "https://unmatched.example.invalid/path"]).await;
    assert!(out.status.success(), "{}", combined(&out));
    assert!(combined(&out).contains("Matched rule #1"));

    // The operation itself uses the captured private snapshot, keeping original relative cwd.
    for (url, index, service) in [
        ("https://contract.example.invalid/path", 0, "http://localhost:43210"),
        ("https://unmatched.example.invalid/path", 1, "http_status:404"),
    ] {
        let result = underlane_core::ingress::test_rule(&bin, Path::new(valid), url, underlane_core::cli::Cancellation::new()).await.unwrap();
        eprintln!("production ingress: binary={bin:?} source={valid:?} argv=[tunnel, --config, <private snapshot>/config.yml, ingress, rule, {url:?}] cwd={:?} index={} service={} raw={}", home.path(), result.rule_index, result.rules[result.rule_index].service, result.raw);
        assert_eq!(result.rule_index, index);
        assert_eq!(result.rules[index].service, service);
        assert_eq!(result.source_path, valid);
        assert!(!result.raw.contains("underlane-ingress-"));
    }
    let out = run(&bin, home.path(), &["access", "ssh-config", "--hostname", "ssh.example.invalid"]).await;
    assert!(out.status.success(), "{}", combined(&out));
    assert!(combined(&out).contains("access ssh --hostname %h"));

    // Ingress validation never starts a connector. Invalid synthetic rules must fail locally
    // after parsing both hidden flags; no cert, credentials, token or account is involved.
    let out =
        run(&bin, home.path(), &["tunnel", "--config", invalid, "--protocol", "http2", "--ha-connections", "1", "ingress", "validate"])
            .await;
    assert!(!out.status.success());
    assert!(combined(&out).contains("last ingress rule must match all URLs"), "hidden flags UNVERIFIED: {}", combined(&out));
    assert!(!combined(&out).contains("flag provided but not defined"));

    // Exercise the production runner as well as the raw CLI contracts above.
    use underlane_core::cli::{self, ConfigPolicy, Invocation};
    use underlane_core::secret::Secret;
    for (target, should_pass) in [(valid, true), (invalid, false)] {
        let mut inv = Invocation::new(["ingress", "validate"], ConfigPolicy::Target(target.into()));
        inv.clear_env = true;
        inv.env.push(("HOME".into(), Secret::new(home.path().to_str().unwrap().into())));
        inv.env.push(("PATH".into(), Secret::new("/usr/bin:/bin".into())));
        inv.timeout = Duration::from_secs(5);
        let result = cli::exec(&bin, inv).await;
        eprintln!("production runner: binary={bin:?} argv=[tunnel, --config, {target:?}, ingress, validate] success={}", result.is_ok());
        assert_eq!(result.is_ok(), should_pass, "production target policy with conflicting default");
    }
}
