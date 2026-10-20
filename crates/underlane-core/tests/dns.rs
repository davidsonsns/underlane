//! DNS mutation tests use only a local fake CLI, never a real Cloudflare account.

use underlane_core::dns::{self, DnsStatus};

#[path = "common/fake_cloudflared.rs"]
#[allow(dead_code)]
mod fake_cloudflared;
use fake_cloudflared::{argv, fake};

const ID: &str = "00000000-0000-4000-8000-000000000001";

#[test]
fn existing_dns_payloads_default_new_repair_metadata_safely() {
    let status: dns::HostnameDns = serde_json::from_value(serde_json::json!({
        "status":"unknown", "message":"Verify DNS", "can_create":false, "needs_dns_access":true
    }))
    .unwrap();
    assert!(!status.can_repair);
    assert!(status.expected_tunnel_id.is_none());
    assert!(status.observed_tunnel_id.is_none());
    assert!(status.actual_target.is_none());
    assert_eq!(status.source, "public");
    assert_eq!(status.checked_at, 0);
}
const SCRIPT: &str = r#"#!/usr/bin/env python3
import json, os, pathlib, sys
root = pathlib.Path(__file__).parent
(root / 'argv.json').write_text(json.dumps({'argv': sys.argv[1:], 'cwd': str(pathlib.Path.cwd())}))
assert os.environ.get('TUNNEL_FORCE_PROVISIONING_DNS') is None
if (root / 'mode').read_text() == 'false-host':
    print('2026-10-13 INF Added CNAME false.example.com which will route to this tunnel tunnelID=synthetic', file=sys.stderr)
    sys.exit(0)
if (root / 'mode').read_text() == 'conflict':
    print('Failed to add route: A DNS record already exists for this hostname; synthetic-secret')
    sys.exit(1)
if (root / 'mode').read_text() == 'wrong-host':
    print('Added CNAME wrong.example.com which will route to this tunnel')
    sys.exit(0)
if (root / 'mode').read_text() == 'no-summary':
    print('old cloudflared version synthetic-secret')
    sys.exit(0)
if (root / 'mode').read_text() == 'existing':
    print('app.example.com is already configured to route to your tunnel tunnelID=synthetic')
    sys.exit(0)
print('Added CNAME app.example.com which will route to this tunnel')
"#;

#[tokio::test]
async fn route_uses_explicit_certificate_neutral_config_and_never_overwrites() {
    let fixture = fake("ok");
    std::fs::write(&fixture.bin, SCRIPT).unwrap();
    let cert = fixture.root.join("synthetic-cert.pem");
    let result = dns::create(&fixture.bin, &fixture.data, &cert, ID, "APP.example.com.")
        .await
        .unwrap();
    assert_eq!(result.status, DnsStatus::Connected);
    assert!(!result.can_create);
    let (args, _) = argv(&fixture);
    assert_eq!(
        args,
        vec![
            "tunnel",
            "--config",
            fixture.data.join("neutral.yml").to_str().unwrap(),
            "--origincert",
            cert.to_str().unwrap(),
            "route",
            "dns",
            "--overwrite-dns=false",
            "--",
            ID,
            "app.example.com"
        ]
    );
    assert!(args.iter().any(|arg| arg == "--overwrite-dns=false"));
    assert!(!args
        .iter()
        .any(|arg| arg == "--overwrite-dns" || arg == "--overwrite-dns=true"));
    let neutral: serde_norway::Value =
        serde_norway::from_str(&std::fs::read_to_string(fixture.data.join("neutral.yml")).unwrap())
            .unwrap();
    assert_eq!(neutral["no-autoupdate"].as_bool(), Some(true));
    assert!(neutral.get("tunnel").is_none());
}

#[tokio::test]
async fn nonsecret_boolean_words_in_reported_hostnames_are_not_redacted() {
    let fixture = fake("false-host");
    std::fs::write(&fixture.bin, SCRIPT).unwrap();
    let result = dns::create(
        &fixture.bin,
        &fixture.data,
        &fixture.root.join("cert.pem"),
        ID,
        "false.example.com",
    )
    .await
    .unwrap();
    assert_eq!(result.status, DnsStatus::Connected);
}

#[tokio::test]
async fn route_conflicts_preserve_records_and_do_not_expose_cli_output() {
    let fixture = fake("conflict");
    std::fs::write(&fixture.bin, SCRIPT).unwrap();
    let error = dns::create(
        &fixture.bin,
        &fixture.data,
        &fixture.root.join("synthetic-cert.pem"),
        ID,
        "app.example.com",
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("already exists"));
    assert!(error.contains("preserved"));
    assert!(!error.contains("synthetic-secret"));
    assert!(argv(&fixture)
        .0
        .iter()
        .any(|arg| arg == "--overwrite-dns=false"));
}

#[tokio::test]
async fn exit_success_requires_confirmation_for_the_exact_hostname() {
    for mode in ["wrong-host", "no-summary", "existing"] {
        let fixture = fake(mode);
        std::fs::write(&fixture.bin, SCRIPT).unwrap();
        let result = dns::create(
            &fixture.bin,
            &fixture.data,
            &fixture.root.join("synthetic-cert.pem"),
            ID,
            "app.example.com",
        )
        .await;
        if mode == "existing" {
            assert_eq!(result.unwrap().status, DnsStatus::Connected);
        } else {
            let error = result.unwrap_err().to_string();
            assert!(error.contains("exact hostname"));
            assert!(!error.contains("synthetic-secret"));
        }
    }
}

#[tokio::test]
async fn invalid_operands_never_spawn_cli_or_contact_dns() {
    let fixture = fake("ok");
    for (id, hostname) in [(ID, "--overwrite-dns"), ("not-a-tunnel", "app.example.com")] {
        assert!(dns::create(
            &fixture.bin,
            &fixture.data,
            &fixture.root.join("synthetic-cert.pem"),
            id,
            hostname
        )
        .await
        .is_err());
        let result = dns::check(hostname, id).await;
        assert_eq!(result.status, DnsStatus::Unknown);
        assert!(!result.can_create);
    }
    assert!(!fixture.root.join("bin/argv.json").exists());
    assert!(!fixture.data.exists());
}
