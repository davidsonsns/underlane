//! Offline source rule matching; each fake and file are disposable and independent.

use std::time::Duration;
use underlane_core::cli::Cancellation;
use underlane_core::ingress::test_rule;

#[path = "common/fake_cloudflared.rs"]
#[allow(dead_code)]
mod fake_cloudflared;
use fake_cloudflared::{argv, fake, Fake};

const RULES: &str = "credentials-file: missing-relative.json\ningress:\n - hostname: test.example.invalid\n   path: ^/api\n   service: http://localhost:43210\n - hostname: '*.example.invalid'\n   service: http://localhost:43211\n - service: http_status:404\n";

fn config(f: &Fake, text: &str) -> std::path::PathBuf {
    let path = f.root.join("project/config.yml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

async fn started(f: &Fake) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !f.root.join("bin/argv.json").exists() { tokio::time::sleep(Duration::from_millis(10)).await; }
    }).await.unwrap();
}

#[tokio::test]
async fn ordered_hostname_wildcard_path_precedence_and_catchall_use_target_source() {
    let f = fake("ok");
    let path = config(&f, RULES);
    for (url, index, service) in [
        ("https://test.example.invalid/api/one", 0, "http://localhost:43210"),
        ("https://test.example.invalid/other", 1, "http://localhost:43211"),
        ("https://sub.example.invalid/api", 1, "http://localhost:43211"),
        ("https://unmatched.invalid/a", 2, "http_status:404"),
    ] {
        let result = test_rule(&f.bin, &path, url, Cancellation::new()).await.unwrap();
        assert_eq!(result.rule_index, index);
        assert_eq!(result.rules[index].service, service);
        assert_eq!(result.rules.len(), 3);
        assert!(result.rules.iter().all(|r| !r.inspected));
        assert_eq!(result.source_path, path.to_str().unwrap());
        let (args, cwd) = argv(&f);
        assert_eq!(&args[..2], ["tunnel", "--config"]);
        assert!(args[2].contains("underlane-ingress-"));
        assert!(!std::path::Path::new(&args[2]).exists(), "private snapshot is removed");
        assert_eq!(&args[3..], ["ingress", "rule", url]);
        assert!(!result.raw.contains("underlane-ingress-"));
        assert_eq!(std::path::Path::new(&cwd).canonicalize().unwrap(), path.parent().unwrap().canonicalize().unwrap());
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), RULES);
}

#[tokio::test]
async fn invalid_url_config_missing_ingress_and_bad_output_are_explanatory() {
    let f = fake("ok");
    let path = config(&f, RULES);
    assert!(test_rule(&f.bin, &path, "example.invalid", Cancellation::new()).await.unwrap_err().contains("complete"));
    assert!(!f.root.join("bin/argv.json").exists(), "invalid input must not spawn");
    for (text, expected) in [("url: http://localhost:1\n", "no local ingress"), ("ingress: []\n", "no local ingress"), ("token: invented-secret\ningress: [broken", "not valid YAML")] {
        std::fs::write(&path, text).unwrap();
        let error = test_rule(&f.bin, &path, "https://test.example.invalid", Cancellation::new()).await.unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("invented-secret"));
    }
    for mode in ["ingress-malformed", "ingress-out-of-range", "ingress-invalid"] {
        let f = fake(mode);
        let path = config(&f, RULES);
        let error = test_rule(&f.bin, &path, "https://test.example.invalid", Cancellation::new()).await.unwrap_err();
        assert!(error.contains("invalid rule match") || error.contains("Validation failed"), "{error}");
        assert!(!std::path::Path::new(&argv(&f).0[2]).exists(), "error removes private snapshot");
    }
}

#[tokio::test]
async fn exact_byte_replacement_and_deletion_during_test_require_retry() {
    for delete in [false, true] {
        let f = fake("ingress-slow");
        let path = config(&f, RULES);
        let future = test_rule(&f.bin, &path, "https://test.example.invalid/api", Cancellation::new());
        let edit = async {
            started(&f).await;
            if delete { std::fs::remove_file(&path).unwrap(); }
            else {
                let replacement = path.with_extension("new.yml");
                std::fs::write(&replacement, format!("{RULES}# comment edit\n")).unwrap();
                std::fs::rename(replacement, &path).unwrap();
            }
        };
        let (result, ()) = tokio::join!(future, edit);
        assert!(result.unwrap_err().contains("Config changed during"));
    }
}

#[tokio::test]
async fn cancels_pending_and_pre_cancelled_tests() {
    let f = fake("ingress-slow");
    let path = config(&f, RULES);
    let cancel = Cancellation::new();
    let future = test_rule(&f.bin, &path, "https://test.example.invalid/api", cancel.clone());
    let abort = async { started(&f).await; cancel.cancel(); };
    let (result, ()) = tokio::join!(future, abort);
    assert!(result.unwrap_err().to_ascii_lowercase().contains("cancelled"));
    let f = fake("ok");
    let path = config(&f, RULES);
    let cancel = Cancellation::new(); cancel.cancel();
    assert!(test_rule(&f.bin, &path, "https://test.example.invalid/api", cancel).await.is_err());
    assert!(!f.root.join("bin/argv.json").exists());
}

#[tokio::test]
async fn secrets_never_reach_argv_or_formatted_match_diagnostics() {
    let f = fake("ingress-echo");
    let text = "token: invented-token\ningress:\n - hostname: test.example.invalid\n   service: https://user:invented-service-password@localhost:43210/a?key=invented-service-query\n   originRequest:\n     headers:\n       X-Custom: invented-custom-header\n - service: http_status:404\n";
    let path = config(&f, text);
    let result = test_rule(&f.bin, &path, "https://user:invented-input-password@test.example.invalid/api?custom=invented-input-query#invented-fragment", Cancellation::new()).await.unwrap();
    assert_eq!(argv(&f).0.last().unwrap(), "https://test.example.invalid/api");
    for formatted in [format!("{result:?}"), serde_json::to_string(&result).unwrap(), format!("{:?}", argv(&f))] {
        assert!(!formatted.contains("invented-"), "{formatted}");
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), text);
}

#[tokio::test]
async fn immutable_snapshot_closes_aba_race_and_preserves_source_identity() {
    let f = fake("ingress-aba");
    let original = format!("tunnel: a\noriginRequest:\n originServerName: a\n{RULES}");
    let path = config(&f, &original);
    let wait_for = |name: &'static str| {
        let bin = f.root.join("bin");
        async move {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !bin.join(name).exists() { tokio::time::sleep(Duration::from_millis(10)).await; }
        }).await.unwrap();
        }
    };
    let future = test_rule(&f.bin, &path, "https://test.example.invalid/api", Cancellation::new());
    let edit = async {
        wait_for("ready").await;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let args = argv(&f).0;
            let snapshot = std::path::Path::new(&args[2]);
            assert_eq!(snapshot.metadata().unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(snapshot.parent().unwrap().metadata().unwrap().permissions().mode() & 0o777, 0o700);
        }
        std::fs::write(&path, original.replace("test.example.invalid", "different.invalid")).unwrap();
        std::fs::write(f.root.join("bin/proceed"), "").unwrap();
        wait_for("read").await;
        std::fs::write(&path, &original).unwrap();
        std::fs::write(f.root.join("bin/restored"), "").unwrap();
    };
    let (result, ()) = tokio::join!(future, edit);
    let result = result.unwrap();
    assert_eq!(result.rule_index, 0, "matched captured A, even when source briefly became B");
    assert_eq!(result.rules[0].hostname.as_deref(), Some("test.example.invalid"));
    assert_eq!(result.rules[0].path.as_deref(), Some("^/api"));
    assert_eq!(result.rules[0].service, "http://localhost:43210");
    assert!(result.raw.contains("test.example.invalid"));
    assert!(!result.raw.contains("[REDACTED]"), "tunnel: a is identity, not a secret");
}

#[tokio::test]
async fn matching_rule_regex_can_contain_error_words_without_false_failure() {
    let f = fake("ok");
    let text = RULES.replace("^/api", "^/api|Incorrect Usage existing certificate overwrite");
    let path = config(&f, &text);
    let result = test_rule(&f.bin, &path, "https://test.example.invalid/api", Cancellation::new()).await.unwrap();
    assert_eq!(result.rule_index, 0);
    assert!(result.rules[0].path.as_ref().unwrap().contains("Incorrect Usage"));
    assert!(result.raw.contains("Incorrect Usage"));
}
