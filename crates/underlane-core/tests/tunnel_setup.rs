//! Setup tests never contact a real Cloudflare account or launch a real browser.

use base64::Engine;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use underlane_core::cli::{Cancellation, CfError, ErrorKind};
use underlane_core::tunnel_setup::{self, SetupIdentity, Stage};

#[path = "common/fake_cloudflared.rs"]
#[allow(dead_code)]
mod fake_cloudflared;
use fake_cloudflared::{fake, Fake};

const PROJECT: &str = "00000000-0000-4000-8000-000000000002";
const TUNNEL: &str = "00000000-0000-4000-8000-000000000001";
const CERT: &str = "-----BEGIN ARGO TUNNEL TOKEN-----\neyJhY2NvdW50SUQiOiJzeW50aGV0aWMtYWNjb3VudCIsInpvbmVJRCI6InN5bnRoZXRpYy16b25lIiwiYXBpVG9rZW4iOiJzeW50aGV0aWMtYXBpLXRva2VuIn0=\n-----END ARGO TUNNEL TOKEN-----\n";

fn fixture(mode: &str, authenticated: bool) -> Fake {
    let f = fake(mode);
    std::fs::write(&f.bin, include_str!("common/setup_cloudflared.py")).unwrap();
    if authenticated {
        std::fs::create_dir_all(f.root.join(".cloudflared")).unwrap();
        std::fs::write(f.root.join(".cloudflared/cert.pem"), CERT).unwrap();
    }
    f
}

#[test]
fn certificate_identity_keeps_the_existing_login_token_secret() {
    let f = fixture("ok", true);
    let cert = tunnel_setup::certificate_identity(&f.root.join(".cloudflared/cert.pem")).unwrap();
    assert_eq!(cert.account.expose(), "synthetic-account");
    assert_eq!(cert.zone_id, "synthetic-zone");
    assert_eq!(cert.api_token.expose(), "synthetic-api-token");
    assert_eq!(
        format!("{:?} {}", cert.api_token, cert.api_token),
        "Secret([REDACTED]) [REDACTED]"
    );
}

fn credentials(f: &Fake) -> PathBuf {
    f.root
        .join(format!(".cloudflared/underlane/{PROJECT}.json"))
}

fn calls(f: &Fake) -> Vec<Vec<String>> {
    std::fs::read_to_string(f.root.join("bin/calls.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn command_count(f: &Fake, command: &str) -> usize {
    calls(f).iter().filter(|args| args[5] == command).count()
}

async fn setup(f: &Fake) -> anyhow::Result<SetupIdentity> {
    tunnel_setup::setup(
        &f.bin,
        &f.data,
        &f.root,
        PROJECT,
        "Test project",
        Cancellation::new(),
        |_| {},
    )
    .await
}

fn save_credentials(f: &Fake, account: &str, secret: &str, id: &str) {
    let path = credentials(f);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        serde_json::to_vec(
            &serde_json::json!({"AccountTag": account, "TunnelSecret": secret, "TunnelID": id}),
        )
        .unwrap(),
    )
    .unwrap();
}

fn save_remote(f: &Fake, id: &str, name: &str, deleted: &str) {
    std::fs::write(
        f.root.join("bin/remote.json"),
        serde_json::to_vec(&serde_json::json!([{"id": id, "name": name, "deleted_at": deleted}]))
            .unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn recovery_accepts_supported_longer_tunnel_secrets() {
    let f = fixture("ok", true);
    let secret = base64::engine::general_purpose::STANDARD.encode([7u8; 64]);
    save_credentials(&f, "synthetic-account", &secret, TUNNEL);
    save_remote(&f, TUNNEL, "Test project", "");
    assert_eq!(setup(&f).await.unwrap().tunnel_id, TUNNEL);
    assert_eq!(command_count(&f, "create"), 0);
}

#[tokio::test]
async fn existing_auth_creates_verified_identity_and_reports_stages() {
    let f = fixture("ok", true);
    let stages = Mutex::new(Vec::new());
    let identity = tunnel_setup::setup(
        &f.bin,
        &f.data,
        &f.root,
        PROJECT,
        " Test  project ",
        Cancellation::new(),
        |stage| stages.lock().unwrap().push(stage),
    )
    .await
    .unwrap();
    assert_eq!(identity.tunnel_id, TUNNEL);
    assert_eq!(identity.credentials_file, credentials(&f));
    assert_eq!(
        *stages.lock().unwrap(),
        [Stage::Checking, Stage::Creating, Stage::Linking]
    );
    assert_eq!(
        serde_json::to_string(&Stage::Creating).unwrap(),
        "\"creating\""
    );
    assert_eq!(command_count(&f, "login"), 0);
    assert_eq!(command_count(&f, "create"), 1);
    assert_eq!(command_count(&f, "list"), 2);
    let intent = f.data.join(format!("project-setup/{PROJECT}.json"));
    let saved = std::fs::read_to_string(&intent).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&saved).unwrap(),
        serde_json::json!({"name": "Test project", "account_id": "synthetic-account"})
    );
    assert!(!saved.contains("synthetic-api-token"));
    assert!(!saved.contains("TunnelSecret"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(credentials(&f))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o400
        );
        assert_eq!(
            std::fs::metadata(credentials(&f).parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(intent).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn flag_like_project_names_are_passed_after_the_option_terminator() {
    for name in [
        "--help",
        "-test",
        "--credentials-file=/synthetic/other.json",
    ] {
        let f = fixture("ok", true);
        let identity = tunnel_setup::setup(
            &f.bin,
            &f.data,
            &f.root,
            PROJECT,
            name,
            Cancellation::new(),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(identity.credentials_file, credentials(&f));
        let calls = calls(&f);
        let create = calls.iter().find(|args| args[5] == "create").unwrap();
        assert_eq!(&create[create.len() - 2..], ["--", name]);
    }
}

#[tokio::test]
async fn missing_certificate_logs_in_then_verifies_account() {
    let f = fixture("ok", false);
    let stages = Mutex::new(Vec::new());
    tunnel_setup::setup(
        &f.bin,
        &f.data,
        &f.root,
        PROJECT,
        "Test project",
        Cancellation::new(),
        |stage| stages.lock().unwrap().push(stage),
    )
    .await
    .unwrap();
    assert_eq!(
        *stages.lock().unwrap(),
        [
            Stage::Checking,
            Stage::Login,
            Stage::Creating,
            Stage::Linking
        ]
    );
    assert_eq!(command_count(&f, "login"), 1);
    assert!(f.root.join(".cloudflared/cert.pem").is_file());
}

#[tokio::test]
async fn zero_exit_login_without_a_valid_certificate_never_creates() {
    for mode in ["login-zero-no-cert", "login-zero-invalid-cert"] {
        let f = fixture(mode, false);
        let error = setup(&f).await.err().unwrap();
        assert!(!format!("{error:?}").contains("invented-"));
        assert_eq!(command_count(&f, "create"), 0);
        assert_eq!(command_count(&f, "login"), 1);
    }
    let f = fixture("auth-fail", false);
    assert!(setup(&f).await.is_err());
    assert_eq!(command_count(&f, "login"), 1);
    assert_eq!(command_count(&f, "list"), 1);
    assert_eq!(command_count(&f, "create"), 0);
}

#[tokio::test]
async fn invalid_existing_certificate_is_preserved_without_login() {
    let f = fixture("ok", true);
    let cert = f.root.join(".cloudflared/cert.pem");
    for invalid in [
        "invented-invalid-api-token".to_owned(),
        format!("{CERT}{CERT}"),
        format!("{CERT}-----BEGIN UNKNOWN-----\nAA==\n-----END UNKNOWN-----\n"),
    ] {
        std::fs::write(&cert, &invalid).unwrap();
        let error = setup(&f).await.err().unwrap();
        assert!(!format!("{error:?}").contains("invented-"));
        assert_eq!(std::fs::read_to_string(&cert).unwrap(), invalid);
        assert!(calls(&f).is_empty());
    }
}

#[tokio::test]
async fn duplicate_remote_name_without_credentials_never_creates() {
    let f = fixture("existing-name", true);
    assert!(setup(&f)
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("already exists"));
    assert_eq!(command_count(&f, "create"), 0);
    assert!(!credentials(&f).exists());
}

#[tokio::test]
async fn recovery_reuses_saved_identity_even_after_project_rename() {
    let f = fixture("ok", true);
    save_credentials(
        &f,
        "synthetic-account",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        TUNNEL,
    );
    save_remote(&f, TUNNEL, "Previous name", "0001-01-01T00:00:00Z");
    let before = std::fs::read(credentials(&f)).unwrap();
    assert_eq!(setup(&f).await.unwrap().tunnel_id, TUNNEL);
    assert_eq!(command_count(&f, "create"), 0);
    assert_eq!(std::fs::read(credentials(&f)).unwrap(), before);
}

#[tokio::test]
async fn recovery_rejects_missing_deleted_or_other_account_tunnels() {
    for mode in ["missing", "deleted", "other-account"] {
        let f = fixture("ok", true);
        save_credentials(
            &f,
            if mode == "other-account" {
                "invented-other-account"
            } else {
                "synthetic-account"
            },
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            TUNNEL,
        );
        if mode != "missing" {
            save_remote(
                &f,
                TUNNEL,
                "Test project",
                if mode == "deleted" {
                    "2026-10-13T00:00:00Z"
                } else {
                    "0001-01-01T00:00:00Z"
                },
            );
        }
        let before = std::fs::read(credentials(&f)).unwrap();
        let error = setup(&f).await.err().unwrap();
        assert!(!format!("{error:?}").contains("invented-"));
        assert_eq!(command_count(&f, "create"), 0);
        assert_eq!(std::fs::read(credentials(&f)).unwrap(), before);
    }
}

#[tokio::test]
async fn invalid_credentials_are_preserved_and_never_leak() {
    let f = fixture("ok", true);
    for (account, secret, id) in [
        ("synthetic-account", "invented-secret", TUNNEL),
        ("synthetic-account", "AA==", TUNNEL),
        ("", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=", TUNNEL),
        (
            "synthetic-account",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "invented-id",
        ),
    ] {
        save_credentials(&f, account, secret, id);
        let before = std::fs::read(credentials(&f)).unwrap();
        let error = setup(&f).await.err().unwrap();
        assert!(!format!("{error:?}").contains("invented-"));
        assert_eq!(std::fs::read(credentials(&f)).unwrap(), before);
    }
    std::fs::write(credentials(&f), "invented-malformed-secret {").unwrap();
    assert!(!format!("{:?}", setup(&f).await.err().unwrap()).contains("invented-"));
    assert!(calls(&f).is_empty());
}

#[tokio::test]
async fn malformed_list_or_new_credentials_never_return_an_identity() {
    for mode in ["malformed-list", "wrong-account", "malformed-credentials"] {
        let f = fixture(mode, true);
        let error = setup(&f).await.err().unwrap();
        assert!(!format!("{error:?}").contains("invented-"));
        assert_eq!(
            command_count(&f, "create"),
            usize::from(mode != "malformed-list")
        );
        if mode != "malformed-list" {
            assert!(credentials(&f).is_file());
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_credentials_or_directory_are_preserved_and_never_reused() {
    use std::os::unix::fs::symlink;
    for directory_link in [false, true] {
        let f = fixture("ok", true);
        let external = f.root.join("external");
        std::fs::create_dir_all(&external).unwrap();
        let target = external.join(format!("{PROJECT}.json"));
        std::fs::write(&target, "invented-external-secret").unwrap();
        let directory = credentials(&f).parent().unwrap().to_path_buf();
        if directory_link {
            symlink(&external, &directory).unwrap();
        } else {
            std::fs::create_dir_all(&directory).unwrap();
            symlink(&target, credentials(&f)).unwrap();
        }
        let error = setup(&f).await.err().unwrap();
        assert!(!format!("{error:?}").contains("invented-"));
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "invented-external-secret"
        );
        assert!(calls(&f).is_empty());
    }
}

#[tokio::test]
async fn legacy_certificate_blocks_are_ignored_alongside_the_account_token() {
    let f = fixture("ok", true);
    std::fs::write(f.root.join(".cloudflared/cert.pem"), format!("-----BEGIN PRIVATE KEY-----\nAA==\n-----END PRIVATE KEY-----\n-----BEGIN CERTIFICATE-----\nAA==\n-----END CERTIFICATE-----\n{CERT}")).unwrap();
    assert_eq!(setup(&f).await.unwrap().tunnel_id, TUNNEL);
}

#[tokio::test]
async fn create_failure_preserves_written_credentials_for_retry() {
    let f = fixture("create-fail-file", true);
    let error = setup(&f).await.err().unwrap();
    assert!(!format!("{error:?}").contains("invented-"));
    assert!(credentials(&f).is_file());
    assert_eq!(setup(&f).await.unwrap().tunnel_id, TUNNEL);
    assert_eq!(command_count(&f, "create"), 1);
}

#[tokio::test]
async fn failed_create_without_credentials_retry_refuses_duplicate() {
    let f = fixture("create-fail-no-file", true);
    assert!(setup(&f).await.is_err());
    assert!(setup(&f)
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("already exists"));
    assert_eq!(command_count(&f, "create"), 1);
}

#[tokio::test]
async fn failed_create_then_rename_still_checks_the_original_remote_name() {
    let f = fixture("create-fail-no-file", true);
    assert!(setup(&f).await.is_err());
    let error = tunnel_setup::setup(
        &f.bin,
        &f.data,
        &f.root,
        PROJECT,
        "Renamed project",
        Cancellation::new(),
        |_| {},
    )
    .await
    .err()
    .unwrap();
    assert!(error
        .to_string()
        .contains("original setup name already exists"));
    assert_eq!(command_count(&f, "create"), 1);
    assert!(!credentials(&f).exists());
}

#[tokio::test]
async fn intent_account_mismatch_blocks_retry_before_any_cli_calls() {
    use base64::Engine;
    let f = fixture("create-fail-no-file", true);
    assert!(setup(&f).await.is_err());
    let before = calls(&f).len();
    let payload = serde_json::to_vec(&serde_json::json!({"accountID": "invented-other-account", "zoneID": "synthetic-zone", "apiToken": "invented-other-api-token"})).unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(payload);
    std::fs::write(
        f.root.join(".cloudflared/cert.pem"),
        format!("-----BEGIN ARGO TUNNEL TOKEN-----\n{encoded}\n-----END ARGO TUNNEL TOKEN-----\n"),
    )
    .unwrap();
    let error = setup(&f).await.err().unwrap();
    assert!(error.to_string().contains("different Cloudflare account"));
    assert!(!format!("{error:?}").contains("invented-"));
    assert_eq!(calls(&f).len(), before);
    assert_eq!(command_count(&f, "create"), 1);
}

#[tokio::test]
async fn malformed_setup_intent_is_preserved_without_creating() {
    let f = fixture("ok", true);
    let intent = f.data.join(format!("project-setup/{PROJECT}.json"));
    std::fs::create_dir_all(intent.parent().unwrap()).unwrap();
    for malformed in [
        "invented-intent-data {",
        "{\"name\":\"  \" ,\"account_id\":\"synthetic-account\"}",
    ] {
        std::fs::write(&intent, malformed).unwrap();
        let error = setup(&f).await.err().unwrap();
        assert!(!format!("{error:?}").contains("invented-"));
        assert_eq!(std::fs::read_to_string(&intent).unwrap(), malformed);
        assert!(calls(&f).is_empty());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_setup_intent_or_directory_is_preserved_without_creating() {
    use std::os::unix::fs::symlink;
    for directory_link in [false, true] {
        let f = fixture("ok", true);
        std::fs::create_dir_all(&f.data).unwrap();
        let external = f.root.join("external-intent");
        std::fs::create_dir_all(&external).unwrap();
        let target = external.join(format!("{PROJECT}.json"));
        std::fs::write(&target, "invented-external-intent").unwrap();
        let directory = f.data.join("project-setup");
        if directory_link {
            symlink(&external, &directory).unwrap();
        } else {
            std::fs::create_dir_all(&directory).unwrap();
            symlink(&target, directory.join(format!("{PROJECT}.json"))).unwrap();
        }
        assert!(setup(&f).await.is_err());
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "invented-external-intent"
        );
        assert!(calls(&f).is_empty());
    }
}

async fn cancel_during_create(f: &Fake) -> anyhow::Error {
    let cancel = Cancellation::new();
    let (bin, data, home) = (f.bin.clone(), f.data.clone(), f.root.clone());
    let child_cancel = cancel.clone();
    let job = tokio::spawn(async move {
        tunnel_setup::setup(
            &bin,
            &data,
            &home,
            PROJECT,
            "Test project",
            child_cancel,
            |_| {},
        )
        .await
    });
    for _ in 0..300 {
        if f.root.join("bin/created").exists() {
            cancel.cancel();
            return tokio::time::timeout(Duration::from_secs(3), job)
                .await
                .unwrap()
                .unwrap()
                .err()
                .unwrap();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    cancel.cancel();
    panic!("fake create did not start");
}

#[tokio::test]
async fn cancellation_preserves_written_credentials_and_never_repeats_create() {
    let f = fixture("create-hang", true);
    let error = cancel_during_create(&f).await;
    assert_eq!(
        error.downcast_ref::<CfError>().unwrap().kind,
        ErrorKind::Cancelled
    );
    assert!(credentials(&f).exists());
    assert_eq!(setup(&f).await.unwrap().tunnel_id, TUNNEL);
    assert_eq!(command_count(&f, "create"), 1);
    let f = fixture("create-hang-no-file", true);
    assert_eq!(
        cancel_during_create(&f)
            .await
            .downcast_ref::<CfError>()
            .unwrap()
            .kind,
        ErrorKind::Cancelled
    );
    assert!(setup(&f)
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("already exists"));
    assert_eq!(command_count(&f, "create"), 1);
}

#[tokio::test]
async fn create_timeout_is_bounded_and_retry_never_duplicates_remote_tunnel() {
    let f = fixture("create-hang-no-file", true);
    let error = tokio::time::timeout(Duration::from_secs(35), setup(&f))
        .await
        .unwrap()
        .err()
        .unwrap();
    assert_eq!(
        error.downcast_ref::<CfError>().unwrap().kind,
        ErrorKind::Timeout
    );
    assert!(setup(&f)
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("already exists"));
    assert_eq!(command_count(&f, "create"), 1);
}

#[tokio::test]
async fn cancelled_before_setup_or_bad_input_never_spawns() {
    let f = fixture("ok", true);
    let cancel = Cancellation::new();
    cancel.cancel();
    let error = tunnel_setup::setup(
        &f.bin,
        &f.data,
        &f.root,
        PROJECT,
        "Test project",
        cancel,
        |_| {},
    )
    .await
    .err()
    .unwrap();
    assert_eq!(
        error.downcast_ref::<CfError>().unwrap().kind,
        ErrorKind::Cancelled
    );
    assert!(tunnel_setup::setup(
        &f.bin,
        &f.data,
        &f.root,
        "../../escape",
        "Test project",
        Cancellation::new(),
        |_| {}
    )
    .await
    .is_err());
    assert!(tunnel_setup::setup(
        &f.bin,
        &f.data,
        &f.root,
        PROJECT,
        &"a".repeat(65),
        Cancellation::new(),
        |_| {}
    )
    .await
    .is_err());
    assert!(calls(&f).is_empty());
}

#[test]
fn linked_config_only_changes_identity_keys_and_retains_nested_options() {
    use underlane_core::config::TunnelConfig;
    let config = TunnelConfig::parse(std::path::Path::new("/synthetic/project.yml"), "protocol: http2\norigincert: /synthetic/cert.pem\noriginRequest:\n  connectTimeout: 9s\n  httpHostHeader: localhost\ningress:\n - hostname: app.example.invalid\n   service: http://localhost:8080\n   originRequest:\n     noTLSVerify: true\n     custom: [one, two]\n - service: http_status:404\n").unwrap();
    let identity = SetupIdentity {
        tunnel_id: TUNNEL.into(),
        credentials_file: "/synthetic/credentials.json".into(),
    };
    let candidate = tunnel_setup::linked_config(&config, &identity).unwrap();
    let linked = TunnelConfig::parse(&config.source, &candidate).unwrap();
    assert_eq!(linked.tunnel.as_deref(), Some(TUNNEL));
    assert_eq!(
        linked.credentials_file.as_ref(),
        Some(&identity.credentials_file)
    );
    let mut original = config.raw.clone();
    original
        .as_mapping_mut()
        .unwrap()
        .insert("tunnel".into(), TUNNEL.into());
    original.as_mapping_mut().unwrap().insert(
        "credentials-file".into(),
        "/synthetic/credentials.json".into(),
    );
    assert_eq!(linked.raw, original);
    assert_eq!(linked.ingress, config.ingress);
    assert!(config.tunnel.is_none());
    assert!(tunnel_setup::linked_config(&linked, &identity).is_err());
}

#[test]
fn linked_config_refuses_existing_tokens_single_origin_and_invalid_identity() {
    use underlane_core::config::TunnelConfig;
    let identity = SetupIdentity {
        tunnel_id: TUNNEL.into(),
        credentials_file: "/synthetic/credentials.json".into(),
    };
    for text in [
        "token: invented-secret\ningress: [{service: http_status:404}]",
        "tunnel: existing\ningress: [{service: http_status:404}]",
        "url: http://localhost:8080",
    ] {
        let config =
            TunnelConfig::parse(std::path::Path::new("/synthetic/project.yml"), text).unwrap();
        let error = tunnel_setup::linked_config(&config, &identity)
            .err()
            .unwrap();
        assert!(!format!("{error:?}").contains("invented-secret"));
    }
    let config = TunnelConfig::parse(
        std::path::Path::new("/synthetic/project.yml"),
        "ingress: [{service: http_status:404}]",
    )
    .unwrap();
    let identity = SetupIdentity {
        tunnel_id: TUNNEL.into(),
        credentials_file: "relative.json".into(),
    };
    assert!(tunnel_setup::linked_config(&config, &identity).is_err());
    let identity = SetupIdentity {
        tunnel_id: "invalid".into(),
        credentials_file: "/synthetic/credentials.json".into(),
    };
    assert!(tunnel_setup::linked_config(&config, &identity).is_err());
}
