//! One-shot contracts use only invented values and the shared Python fixture.

use serde::Deserialize;
use std::time::Duration;
use underlane_core::cli::{self, CfError, ConfigPolicy, ErrorKind, Invocation, Runner, Stdout, StdoutMode, Warning};
use underlane_core::secret::Secret;

#[path = "common/fake_cloudflared.rs"]
mod fake_cloudflared;
use fake_cloudflared::{argv, fake, Fake};

fn inv(f: &Fake, args: &[&str]) -> Invocation {
    Invocation::new(args.iter().copied(), ConfigPolicy::Neutral(f.data.clone()))
}

fn env(inv: &mut Invocation, key: &str, value: &str) {
    inv.env.push((key.into(), Secret::new(value.into())));
}

#[test]
fn classification_handles_rc_zero_failures_and_warning_success() {
    for (code, text, expected) in [
        (0, "Incorrect Usage. flag provided but not defined", ErrorKind::Usage),
        (0, "You have an existing certificate at /synthetic/cert.pem which login would overwrite", ErrorKind::AlreadyLoggedIn),
        (1, "Cannot read config: no such file", ErrorKind::Config),
        (1, "Tunnel credentials file doesn't exist or is not a file", ErrorKind::Credentials),
        (1, "Cannot find a valid certificate at the path", ErrorKind::NotLoggedIn),
        (1, "Multiple-origin routing is incompatible with the --url flag", ErrorKind::IncompatibleOrigin),
        (1, "Error opening metrics listener: address already in use", ErrorKind::MetricsBind),
        (1, "Expected to find a single tunnel but found 0 tunnels", ErrorKind::TunnelNotFound),
        (7, "", ErrorKind::Unknown),
    ] {
        assert_eq!(CfError::classify(Some(code), "", text).unwrap_err().kind, expected, "{text}");
    }
    let warnings = CfError::classify(
        Some(0),
        "already configured to route to your tunnel",
        r#"{"level":"warn","message":"Your version 2026.9.1 is outdated"}"#,
    )
    .unwrap();
    assert!(matches!(&warnings[..], [Warning::OutdatedVersion(_)]));
    assert!(CfError::classify(Some(1), "already configured to route to your tunnel", "").is_err());
    assert!(CfError::classify(None, "", "").is_err());
    assert!(CfError::classify(Some(0), "Validating rules from /synthetic/429.yml\nOK", "").is_ok());
    assert!(CfError::classify(Some(0), "Validating rules from /synthetic/Incorrect Usage.yml\nOK", "").is_ok());
    assert!(CfError::classify(Some(0), "Validating rules from /synthetic/existing certificate overwrite.yml\nOK", "").is_ok());
    assert!(CfError::classify(Some(0), r#"[{"name":"Incorrect Usage. existing certificate overwrite","id":"429"}]"#, "").is_ok());
    let error = CfError::classify(Some(1), "Validation failed: --token #invented-secret will match every hostname", "").unwrap_err();
    assert!(!format!("{error:?}").contains("invented-secret"));
}

#[test]
fn successful_ingress_rule_payload_is_not_a_diagnostic_but_stderr_and_failures_are() {
    let matched = "Using rules from /synthetic/Incorrect Usage.yml\nMatched rule #0\n\thostname: existing-certificate-overwrite.example.invalid\n\tpath: Incorrect Usage\n\tservice: http://existing-certificate-overwrite.invalid\n";
    assert!(CfError::classify(Some(0), matched, "").is_ok());
    let usage = "Incorrect Usage: flag provided but not defined: fake";
    assert_eq!(CfError::classify(Some(0), matched, usage).unwrap_err().kind, ErrorKind::Usage);
    assert_eq!(CfError::classify(Some(0), usage, "").unwrap_err().kind, ErrorKind::Usage);
    assert!(CfError::classify(Some(1), matched, "").is_err(), "nonzero is never suppressed");
    for malformed in [
        "Using rules from /synthetic.yml\nMatched rule #bogus\n\tpath: Incorrect Usage",
        "prefix\nUsing rules from /synthetic.yml\nMatched rule #0\n\tpath: Incorrect Usage",
        "Using rules from /synthetic.yml\nMatched rule #0\nIncorrect Usage",
    ] { assert!(CfError::classify(Some(0), malformed, "").is_err(), "{malformed}"); }
}

#[tokio::test]
async fn config_policy_and_cwd_are_explicit_and_version_is_config_free() {
    let f = fake("ok");
    let target = f.root.join("project/config.yml");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "credentials-file: relative.json\ningress:\n - service: http_status:404\n").unwrap();
    let invocation = Invocation::new(["ingress", "validate"], ConfigPolicy::Target(target.clone()));
    cli::exec(&f.bin, invocation).await.unwrap();
    let (args, cwd) = argv(&f);
    assert_eq!(args, ["tunnel", "--config", target.to_str().unwrap(), "ingress", "validate"]);
    assert_eq!(std::path::Path::new(&cwd).canonicalize().unwrap(), target.parent().unwrap().canonicalize().unwrap());
    cli::exec(&f.bin, inv(&f, &["list", "-o", "json"])).await.unwrap();
    let (args, _) = argv(&f);
    assert_eq!(&args[..3], ["tunnel", "--config", f.data.join("neutral.yml").to_str().unwrap()]);
    assert!(std::fs::read_to_string(f.data.join("neutral.yml")).unwrap().contains("no-autoupdate: true"));
    assert_eq!(underlane_core::cloudflared::version(&f.bin).await.as_deref(), Some("2026.9.1"));
    assert_eq!(argv(&f).0, ["--version"]);
    let bad = Invocation::new(["tunnel", "list"], ConfigPolicy::None);
    assert_eq!(cli::exec(&f.bin, bad).await.unwrap_err().kind, ErrorKind::Usage);
    let bad = Invocation::new(["--no-autoupdate", "tunnel", "ingress", "validate"], ConfigPolicy::None);
    assert_eq!(cli::exec(&f.bin, bad).await.unwrap_err().kind, ErrorKind::Usage);
}

#[tokio::test]
async fn json_aliases_secret_payloads_and_stderr_logs_are_preserved_safely() {
    #[derive(Debug, Deserialize)]
    struct Tunnel {
        id: String,
        #[serde(default, alias = "created_at", alias = "createdAt")]
        created: String,
        #[serde(default)]
        optional: Vec<String>,
    }
    let f = fake("outdated");
    let mut invocation = inv(&f, &["list", "-o", "json"]);
    invocation.stdout = StdoutMode::Json;
    let output = cli::exec(&f.bin, invocation).await.unwrap();
    let list: Vec<Tunnel> = output.stdout.deserialize().unwrap();
    assert_eq!(list.len(), 1);
    assert!(!list[0].id.is_empty() && !list[0].created.is_empty() && list[0].optional.is_empty());
    assert!(matches!(&output.warnings[..], [Warning::OutdatedVersion(_)]));
    assert_eq!(output.logs[0].level, "warn");
    let mut invocation = inv(&f, &["info", "-o", "json", &list[0].id]);
    invocation.stdout = StdoutMode::Json;
    let info: Tunnel = cli::exec(&f.bin, invocation).await.unwrap().stdout.deserialize().unwrap();
    assert_eq!(info.created, list[0].created);
    let mut invocation = inv(&f, &["create", "-o", "json", "synthetic"]);
    invocation.stdout = StdoutMode::Secret;
    let output = cli::exec(&f.bin, invocation).await.unwrap();
    assert!(!format!("{output:?}").contains("synthetic-create-secret"));
    match output.stdout {
        Stdout::Secret(bytes) => assert!(String::from_utf8_lossy(bytes.expose()).contains("synthetic-create-secret")),
        _ => panic!("secret stdout was exposed"),
    }
}

#[tokio::test]
async fn error_paths_and_debug_redact_synthetic_values() {
    let f = fake("ok");
    for code in ["0", "1"] {
        let mut invocation = inv(&f, &["login"]);
        invocation.stdout = StdoutMode::Secret;
        env(&mut invocation, "FAKE_CLOUDFLARED_STDOUT", "Incorrect Usage. invented-secret-output");
        env(&mut invocation, "FAKE_CLOUDFLARED_STDERR", "failure --token invented-stderr-token");
        env(&mut invocation, "FAKE_CLOUDFLARED_EXIT_CODE", code);
        assert!(!format!("{invocation:?}").contains("invented-"));
        let error = cli::exec(&f.bin, invocation).await.unwrap_err();
        assert!(!format!("{error:?} {error}").contains("invented-"));
    }
    let mut invocation = inv(&f, &["info"]);
    invocation.stdout = StdoutMode::Json;
    env(&mut invocation, "FAKE_CLOUDFLARED_STDOUT", r#"{"token": "invented-malformed", bad"#);
    assert_eq!(cli::exec(&f.bin, invocation).await.unwrap_err().kind, ErrorKind::InvalidJson);
    let error = CfError::classify(Some(1), "", r#"unknown: {"apiToken":"invented-unknown",bad"#).unwrap_err();
    assert!(!format!("{error:?} {error}").contains("invented-"));
    let mut invocation = inv(&f, &["list"]);
    env(&mut invocation, "TUNNEL_TOKEN", "invented-bare-env-token");
    env(&mut invocation, "FAKE_CLOUDFLARED_STDERR", "failure echoed invented-bare-env-token");
    env(&mut invocation, "FAKE_CLOUDFLARED_EXIT_CODE", "1");
    let error = cli::exec(&f.bin, invocation).await.unwrap_err();
    assert!(!format!("{error:?} {error}").contains("invented-"));
}

#[tokio::test]
async fn all_fake_error_fixtures_are_classified_and_permits_released() {
    let runner = Runner::new(1);
    for (mode, kind) in [
        ("unknown-flag", ErrorKind::Usage),
        ("login-refused", ErrorKind::AlreadyLoggedIn),
        ("missing-config", ErrorKind::Config),
        ("missing-credentials", ErrorKind::Credentials),
        ("missing-cert", ErrorKind::NotLoggedIn),
        ("incompatible-url", ErrorKind::IncompatibleOrigin),
        ("metrics-bind", ErrorKind::MetricsBind),
        ("tunnel-not-found", ErrorKind::TunnelNotFound),
        ("nonzero", ErrorKind::Unknown),
    ] {
        let f = fake(mode);
        assert_eq!(runner.exec(&f.bin, inv(&f, &["list"])).await.unwrap_err().kind, kind, "{mode}");
        let good = fake("ok");
        runner.exec(&good.bin, Invocation::new(["--version"], ConfigPolicy::None)).await.unwrap();
    }
    let f = fake("already-routed");
    runner.exec(&f.bin, inv(&f, &["route", "dns", "synthetic", "synthetic.example.com"])).await.unwrap();
    let f = fake("ok");
    assert_eq!(runner.exec(&f.root.join("missing"), inv(&f, &["list"])).await.unwrap_err().kind, ErrorKind::BinaryMissing);
    runner.exec(&f.bin, inv(&f, &["list"])).await.unwrap();
}

#[cfg(unix)]
async fn pids(f: &Fake) -> Vec<u32> {
    for _ in 0..150 {
        if let Ok(text) = std::fs::read_to_string(f.root.join("bin/pids.json")) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                return ["parent", "descendant"].into_iter().filter_map(|k| value[k].as_u64().map(|p| p as u32)).collect();
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("fake did not spawn");
}

#[cfg(unix)]
async fn assert_dead(pids: &[u32]) {
    for _ in 0..150 {
        if pids.iter().all(|pid| underlane_core::platform::signal(*pid, 0).is_err()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("unreaped processes: {pids:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn timeout_cancellation_and_future_drop_reap_group_and_release_permit() {
    for action in ["timeout", "cancel", "drop"] {
        let runner = Runner::new(1);
        let f = fake("descendant");
        let mut invocation = inv(&f, &["list"]);
        invocation.timeout = if action == "timeout" { Duration::from_secs(2) } else { Duration::from_secs(10) };
        let cancel = invocation.cancel.clone();
        let job = {
            let runner = runner.clone();
            let bin = f.bin.clone();
            tokio::spawn(async move { runner.exec(&bin, invocation).await })
        };
        let group = pids(&f).await;
        assert_eq!(group.len(), 2);
        match action {
            "cancel" => {
                cancel.cancel();
                assert_eq!(job.await.unwrap().unwrap_err().kind, ErrorKind::Cancelled);
            }
            "drop" => {
                job.abort();
                assert!(job.await.unwrap_err().is_cancelled());
            }
            _ => {
                assert_eq!(job.await.unwrap().unwrap_err().kind, ErrorKind::Timeout);
            }
        }
        assert_dead(&group).await;
        let good = fake("ok");
        runner.exec(&good.bin, inv(&good, &["list"])).await.unwrap();
    }
}

#[cfg(unix)]
#[tokio::test]
async fn queued_cancellation_or_drop_never_spawns() {
    let runner = Runner::new(1);
    let busy = fake("ignore-term");
    let mut invocation = inv(&busy, &["list"]);
    invocation.timeout = Duration::from_secs(10);
    let cancel_busy = invocation.cancel.clone();
    let job = {
        let runner = runner.clone();
        let bin = busy.bin.clone();
        tokio::spawn(async move { runner.exec(&bin, invocation).await })
    };
    let group = pids(&busy).await;
    let queued = fake("ok");
    let invocation = inv(&queued, &["list"]);
    let cancel = invocation.cancel.clone();
    let waiting = {
        let runner = runner.clone();
        let bin = queued.bin.clone();
        tokio::spawn(async move { runner.exec(&bin, invocation).await })
    };
    tokio::time::sleep(Duration::from_millis(30)).await;
    cancel.cancel();
    assert_eq!(waiting.await.unwrap().unwrap_err().kind, ErrorKind::Cancelled);
    let invocation = inv(&queued, &["list"]);
    let waiting = {
        let runner = runner.clone();
        let bin = queued.bin.clone();
        tokio::spawn(async move { runner.exec(&bin, invocation).await })
    };
    tokio::time::sleep(Duration::from_millis(30)).await;
    waiting.abort();
    assert!(waiting.await.unwrap_err().is_cancelled());
    assert!(!queued.root.join("bin/argv.json").exists());
    cancel_busy.cancel();
    assert_eq!(job.await.unwrap().unwrap_err().kind, ErrorKind::Cancelled);
    assert_dead(&group).await;
    assert!(!queued.root.join("bin/argv.json").exists());
    runner.exec(&queued.bin, inv(&queued, &["list"])).await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn inherited_run_secrets_are_removed_but_explicit_secrets_work() {
    use std::os::unix::fs::PermissionsExt;
    if std::env::var_os("UNDERLANE_ENV_CHILD").is_none() {
        let output = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "inherited_run_secrets_are_removed_but_explicit_secrets_work", "--nocapture"])
                .env("UNDERLANE_ENV_CHILD", "1")
                .env("TUNNEL_TOKEN", "invented-inherited-token")
                .env("TUNNEL_CRED_CONTENTS", "invented-inherited-credentials")
                .env("TUNNEL_CREDENTIALS_CONTENTS", "invented-inherited-alias")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        return;
    }
    assert_eq!(std::env::var("TUNNEL_TOKEN").unwrap(), "invented-inherited-token");
    let f = fake("ok");
    // This test runs in its own environment, without racing other test-process threads.
    let wrapper = f.root.join("wrapper");
    std::fs::write(&wrapper, "#!/bin/sh\n[ -z \"$TUNNEL_TOKEN\" ] && [ -z \"$TUNNEL_CRED_CONTENTS\" ] && [ -z \"$TUNNEL_CREDENTIALS_CONTENTS\" ] || exit 17\nprintf OK\n").unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    cli::exec(&wrapper, Invocation::new(["--version"], ConfigPolicy::None)).await.unwrap();
    let mut invocation = Invocation::new(["--version"], ConfigPolicy::None);
    env(&mut invocation, "TUNNEL_TOKEN", "invented-explicit-token");
    assert_eq!(cli::exec(&wrapper, invocation).await.unwrap_err().kind, ErrorKind::Unknown);
}

#[test]
fn structured_runtime_diagnostics_classify_only_message_and_error_context() {
    for (raw, kind) in [
        (r#"{"level":"error","message":"Request failed","error":"Unable to reach the origin service: status 403","name":"Incorrect Usage","id":"429"}"#, ErrorKind::OriginConnection),
        (r#"{"level":"error","message":"Failed to serve tunnel connection","error":"authentication failed: status 403"}"#, ErrorKind::Auth),
        (r#"{"level":"error","message":"Failed to dial to edge","error":"timeout"}"#, ErrorKind::EdgeConnection),
        (r#"{"level":"error","message":"Request failed","error":"opaque failure","name":"authentication failed","id":"status 429"}"#, ErrorKind::Unknown),
        (r#"{"level":"error","message":"opaque","error":"unauthorized"}"#, ErrorKind::Auth),
        ("unauthorized", ErrorKind::Auth),
        ("Tunnel credentials file '/tmp/foo Incorrect Usage.json' doesn't exist or is not a file", ErrorKind::Credentials),
        ("Tunnel credentials file '/tmp/foo authentication failed.json' doesn't exist or is not a file", ErrorKind::Credentials),
        ("error loading config '/tmp/foo Incorrect Usage.yaml': permission denied", ErrorKind::Config),
        ("error loading config /tmp/unauthorized.yml: permission denied", ErrorKind::Config),
        (r#"{"level":"error","message":"opaque","error":"stat /tmp/unauthorized.yml: no such file"}"#, ErrorKind::Unknown),
        (r#"{"level":"error","message":"opaque","error":"stat /tmp/status-429.yml: no such file"}"#, ErrorKind::Unknown),
        (r#"{"level":"error","message":"Unable to reach the origin service","error":"rate limit status 429"}"#, ErrorKind::OriginConnection),
        (r#"{"level":"error","message":"unknown","name":"Incorrect Usage","token":"invented-token",bad"#, ErrorKind::Unknown),
    ] {
        assert_eq!(CfError::classify(Some(1), "", raw).unwrap_err().kind, kind, "{raw}");
    }
    assert_eq!(CfError::classify(Some(0), r#"{"level":"error","message":"Incorrect Usage: flag provided but not defined"}"#, "").unwrap_err().kind, ErrorKind::Usage);
    for raw in [
        r#"{"level":"warn","message":"Incorrect Usage authentication failed status 403"}"#,
        r#"{"level":"info","message":"Incorrect Usage existing certificate overwrite"}"#,
        r#"{"name":"Incorrect Usage authentication failed","id":"status 429"}"#,
        "2026-10-10T06:00:00Z WRN Incorrect Usage authentication failed status 403",
        "Warning: Incorrect Usage authentication failed status 403",
    ] { assert!(CfError::classify(Some(0), "", raw).is_ok(), "{raw}"); }
}
