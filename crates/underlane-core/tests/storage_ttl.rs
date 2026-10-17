use underlane_core::db::{Db, LogQuery};
use underlane_core::model::{LogLine, Settings, Tunnel, TunnelKind};
use underlane_core::proxy::capture::{BodyCapture, CapturedExchange, ExchangeKind};

const DAY: i64 = 86_400_000;

fn log(tunnel: &str, ts: i64, level: &str, msg: &str) -> LogLine {
    LogLine { id: 0, tunnel_id: tunnel.into(), ts, level: level.into(), message: msg.into(), fields: "{}".into() }
}

fn exchange(id: &str, ts: i64, body: usize) -> CapturedExchange {
    CapturedExchange {
        id: id.into(),
        tunnel_id: "t1".into(),
        rule_index: 0,
        started_at_ms: ts,
        duration_ms: Some(12),
        kind: ExchangeKind::Http,
        method: "POST".into(),
        host: "app.example.com".into(),
        path: format!("/items/{id}"),
        status: Some(201),
        req_headers: vec![("content-type".into(), "application/json".into())],
        resp_headers: vec![],
        req_body: BodyCapture { bytes: vec![b'q'; body], size: body as u64, truncated: false },
        resp_body: BodyCapture { bytes: vec![b'r'; body], size: body as u64, truncated: false },
        error: None,
    }
}

#[test]
fn ttl_sweep_deletes_only_expired_rows() {
    let db = Db::open_in_memory().unwrap();
    let now = 100 * DAY;
    db.insert_logs(&[
        log("t1", now - 8 * DAY, "info", "old"),
        log("t1", now - 6 * DAY, "info", "recent"),
        log("t1", now, "error", "new"),
    ])
    .unwrap();
    db.upsert_request(&exchange("old", now - 7 * DAY - 1, 10)).unwrap();
    db.upsert_request(&exchange("edge", now - 7 * DAY, 10)).unwrap();
    db.upsert_request(&exchange("new", now - 1000, 10)).unwrap();

    let stats = db.sweep(now, 7 * DAY, u64::MAX).unwrap();
    assert_eq!(stats.logs_expired, 1);
    assert_eq!(stats.requests_expired, 1);

    let logs = db.query_logs(&LogQuery { limit: 100, ..Default::default() }).unwrap();
    let msgs: Vec<&str> = logs.iter().map(|l| l.message.as_str()).collect();
    assert_eq!(msgs, vec!["recent", "new"]);
    let ids: Vec<String> = db.list_requests(None, None, 100).unwrap().into_iter().map(|r| r.id).collect();
    assert_eq!(ids, vec!["new".to_string(), "edge".to_string()]);
}

#[test]
fn size_cap_removes_oldest_rows_and_shrinks_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("underlane.db");
    let db = Db::open(&path).unwrap();
    let now = 10 * DAY;
    for i in 0..200 {
        db.upsert_request(&exchange(&format!("r{i:03}"), now - 1000 + i, 50_000)).unwrap();
    }
    let before = db.used_bytes().unwrap();
    assert!(before > 15_000_000, "{before}");

    let cap = 4_000_000;
    let stats = db.sweep(now, 7 * DAY, cap).unwrap();
    assert!(stats.requests_capped > 0);
    assert!(db.used_bytes().unwrap() <= cap);
    let remaining = db.list_requests(None, None, 1000).unwrap();
    // Newest rows survive.
    assert_eq!(remaining.first().unwrap().id, "r199");
    assert!(remaining.iter().all(|r| r.id.as_str() > "r100"));
    // Freed pages are returned (auto_vacuum = INCREMENTAL + WAL checkpoint).
    drop(db);
    let file = std::fs::metadata(&path).unwrap().len();
    assert!(file < before, "file {file} should shrink below {before}");
}

#[test]
fn log_queries_filter_by_tunnel_level_search_and_cursor() {
    let db = Db::open_in_memory().unwrap();
    db.insert_logs(&[
        log("a", 1, "info", "Registered tunnel connection"),
        log("a", 2, "error", "Unable to reach the origin service"),
        log("b", 3, "info", "Registered tunnel connection"),
        log("a", 4, "warn", "Retrying 100% of requests"),
    ])
    .unwrap();
    let q = |q: LogQuery| db.query_logs(&LogQuery { limit: 100, ..q }).unwrap();
    assert_eq!(q(LogQuery { tunnel_id: Some("a".into()), ..Default::default() }).len(), 3);
    let errs = q(LogQuery { levels: vec!["error".into(), "warn".into()], ..Default::default() });
    assert_eq!(errs.iter().map(|l| l.ts).collect::<Vec<_>>(), vec![2, 4]);
    assert_eq!(q(LogQuery { search: Some("origin".into()), ..Default::default() }).len(), 1);
    // LIKE wildcards in the search are literal.
    assert_eq!(q(LogQuery { search: Some("100%".into()), ..Default::default() }).len(), 1);
    assert_eq!(q(LogQuery { search: Some("_".into()), ..Default::default() }).len(), 0);
    let first = q(LogQuery::default());
    let after = q(LogQuery { after_id: Some(first[1].id), ..Default::default() });
    assert_eq!(after.len(), 2);
    let limited = db.query_logs(&LogQuery { limit: 2, ..Default::default() }).unwrap();
    assert_eq!(limited.iter().map(|l| l.ts).collect::<Vec<_>>(), vec![3, 4], "newest page, oldest first");
}

#[test]
fn requests_round_trip_and_upsert() {
    let db = Db::open_in_memory().unwrap();
    let mut x = exchange("x1", 5, 3);
    x.duration_ms = None;
    db.upsert_request(&x).unwrap();
    x.duration_ms = Some(40);
    x.resp_body = BodyCapture { bytes: b"done".to_vec(), size: 4, truncated: true };
    db.upsert_request(&x).unwrap();
    let got = db.get_request("x1").unwrap().unwrap();
    assert_eq!(got, x);
    assert_eq!(db.list_requests(Some("t1"), Some("items"), 10).unwrap().len(), 1);
    assert_eq!(db.list_requests(Some("t1"), Some("201"), 10).unwrap().len(), 1);
    assert_eq!(db.list_requests(Some("nope"), None, 10).unwrap().len(), 0);
}

#[test]
fn request_filters_apply_before_limit_and_combine_categories() {
    let db = Db::open_in_memory().unwrap();
    for (id, ts, status, method, tunnel) in [
        ("get-ok", 1, Some(200), "GET", "t1"),
        ("post-error", 2, Some(502), "POST", "t1"),
        ("get-error", 3, Some(502), "GET", "t1"),
        ("other-tunnel", 4, Some(502), "POST", "t2"),
        ("pending", 5, None, "POST", "t1"),
    ] {
        let mut x = exchange(id, ts, 0);
        x.status = status;
        x.method = method.into();
        x.tunnel_id = tunnel.into();
        db.upsert_request(&x).unwrap();
    }
    // All recent rows are outside the selected status/method; matches must precede LIMIT.
    for i in 0..20 {
        db.upsert_request(&exchange(&format!("recent-{i}"), 100 + i, 0)).unwrap();
    }
    let methods = vec!["GET".into(), "POST".into()];
    let rows = db.list_requests_filtered(Some("t1"), None, &[200, 502], &methods, 2).unwrap();
    assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["get-error", "post-error"]);
    let rows = db.list_requests_filtered(Some("t1"), Some("items"), &[200, 502], &["GET".into()], 10).unwrap();
    assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["get-error", "get-ok"]);
    assert_eq!(db.list_requests_filtered(Some("t1"), None, &[502], &["POST".into()], 1).unwrap()[0].id, "post-error");
    assert!(db.list_requests_filtered(None, None, &[502], &["POST' OR 1=1 --".into()], 10).unwrap().is_empty());
    assert_eq!(db.list_requests_filtered(None, None, &[], &[], 100).unwrap().len(), db.list_requests(None, None, 100).unwrap().len());
    let posts = db.list_requests_filtered(None, None, &[], &["POST".into()], 100).unwrap();
    assert_eq!(posts.len(), 23);
    assert!(posts.iter().any(|r| r.id == "pending"), "no status selection includes pending responses");
}

#[test]
fn settings_groups_and_tunnels_persist() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("b.db");
    {
        let db = Db::open(&path).unwrap();
        assert_eq!(db.settings().unwrap(), Settings::default());
        let mut s = Settings::default();
        s.theme = "dark".into();
        s.log_ttl_days = 3;
        db.save_settings(&s).unwrap();
        let p = db.ensure_group("Shop").unwrap();
        assert_eq!(db.ensure_group("shop").unwrap().id, p.id, "case-insensitive");
        db.upsert_tunnel(&Tunnel {
            id: "t1".into(),
            group_id: Some(p.id.clone()),
            name: "web".into(),
            kind: TunnelKind::Quick,
            config_path: None,
            origin_url: Some("http://localhost:3000".into()),
            inspect: true,
            autostart: true,
            loglevel: "info".into(),
            created_at: 1,
            sort: 0,
        })
        .unwrap();
    }
    let db = Db::open(&path).unwrap();
    assert_eq!(db.settings().unwrap().theme, "dark");
    let t = db.tunnel("t1").unwrap().unwrap();
    assert!(t.inspect && t.autostart);
    db.delete_tunnel("t1").unwrap();
    assert!(db.groups().unwrap().is_empty(), "empty groups are pruned");
}

fn marked_exchange(id: &str, tunnel: &str, marker: &str) -> CapturedExchange {
    let mut x = exchange(id, 1, 0);
    x.tunnel_id = tunnel.into();
    // Exercise overflow pages as well as the row's inline body bytes.
    let bytes = marker.repeat(400).into_bytes();
    x.req_body = BodyCapture { size: bytes.len() as u64, bytes: bytes.clone(), truncated: false };
    x.resp_body = BodyCapture { size: bytes.len() as u64, bytes, truncated: false };
    x
}

fn contains_marker(path: &std::path::Path, marker: &str) -> bool {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(error) => panic!("reading {}: {error}", path.display()),
    };
    bytes.windows(marker.len()).any(|window| window == marker.as_bytes())
}

fn wal_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    wal.into()
}

fn assert_erased(path: &std::path::Path, marker: &str) {
    for file in [path.to_owned(), wal_path(path)] {
        assert!(!contains_marker(&file, marker), "deleted marker remains in {}", file.display());
    }
    assert_eq!(std::fs::metadata(wal_path(path)).map(|m| m.len()).unwrap_or(0), 0);
}

fn checkpoint_without_truncating(path: &std::path::Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    let busy: i64 = conn.query_row("PRAGMA wal_checkpoint(FULL)", [], |row| row.get(0)).unwrap();
    assert_eq!(busy, 0);
}

#[test]
fn clear_requests_scrubs_database_and_wal_while_retaining_other_tunnels() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("requests.db");
    let db = Db::open(&path).unwrap();
    let removed = "synthetic-request-body-to-delete-6d305c";
    let retained = marked_exchange("keep", "t2", "synthetic-retained-request-c8cd66");
    db.upsert_request(&marked_exchange("remove", "t1", removed)).unwrap();
    db.upsert_request(&retained).unwrap();
    checkpoint_without_truncating(&path);
    assert!(contains_marker(&path, removed));
    assert!(contains_marker(&wal_path(&path), removed));

    db.clear_requests(Some("t1")).unwrap();
    assert!(db.get_request("remove").unwrap().is_none());
    assert_eq!(db.get_request("keep").unwrap(), Some(retained.clone()));
    assert_erased(&path, removed);

    db.clear_requests(None).unwrap();
    assert!(db.list_requests(None, None, 100).unwrap().is_empty());
    assert_erased(&path, "synthetic-retained-request-c8cd66");
    db.upsert_request(&retained).unwrap();
    assert_eq!(db.get_request("keep").unwrap(), Some(retained));
}

#[test]
fn clear_logs_scrubs_database_and_wal_while_retaining_other_tunnels() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("logs.db");
    let db = Db::open(&path).unwrap();
    let removed = "synthetic-log-to-delete-971c8e";
    let kept = "synthetic-log-to-retain-e596b9";
    db.insert_logs(&[log("t1", 1, "info", removed), log("t2", 2, "info", kept)]).unwrap();
    checkpoint_without_truncating(&path);
    assert!(contains_marker(&path, removed));
    assert!(contains_marker(&wal_path(&path), removed));

    db.clear_logs(Some("t1")).unwrap();
    let rows = db.query_logs(&LogQuery { limit: 100, ..Default::default() }).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message, kept);
    assert_erased(&path, removed);
    db.clear_logs(None).unwrap();
    assert_erased(&path, kept);
}

#[test]
fn reset_scrubs_active_files_and_legacy_deleted_content_and_stays_usable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reset.db");
    let db = Db::open(&path).unwrap();
    let request = "synthetic-reset-body-9cb2dd";
    let token = "synthetic-reset-token-d7fcb7";
    let message = "synthetic-reset-log-28a7ab";
    let legacy = "synthetic-legacy-deletion-857f0d";
    db.upsert_request(&marked_exchange("remove", "t1", request)).unwrap();
    db.kv_set("token", token).unwrap();
    db.kv_set("legacy", legacy).unwrap();
    db.insert_logs(&[log("t1", 1, "info", message)]).unwrap();
    {
        // Model an older application version that did not scrub deleted page contents.
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("PRAGMA secure_delete = OFF; DELETE FROM settings WHERE key = 'legacy';").unwrap();
    }
    checkpoint_without_truncating(&path);
    for marker in [request, token, message, legacy] {
        assert!(contains_marker(&path, marker), "fixture must contain {marker}");
    }

    db.reset().unwrap();
    for marker in [request, token, message, legacy] {
        assert_erased(&path, marker);
    }
    assert!(db.list_requests(None, None, 100).unwrap().is_empty());
    assert!(db.query_logs(&LogQuery { limit: 100, ..Default::default() }).unwrap().is_empty());
    assert!(db.kv_get("token").unwrap().is_none());
    db.kv_set("fresh", "usable").unwrap();
    assert_eq!(db.kv_get("fresh").unwrap().as_deref(), Some("usable"));
}

#[test]
fn clear_reports_blocked_wal_cleanup_and_retry_finishes_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("busy.db");
    let db = Db::open(&path).unwrap();
    let marker = "synthetic-busy-request-094161";
    db.upsert_request(&marked_exchange("remove", "t1", marker)).unwrap();
    let reader = rusqlite::Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let count: i64 = reader.query_row("SELECT count(*) FROM requests", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 1);

    let error = db.clear_requests(None).unwrap_err();
    assert!(error.to_string().contains("Records were deleted, but SQLite cleanup could not finish"), "{error}");
    assert!(contains_marker(&wal_path(&path), marker), "reader keeps the old WAL snapshot alive");
    reader.execute_batch("ROLLBACK").unwrap();
    db.clear_requests(None).unwrap();
    assert_erased(&path, marker);
}

#[test]
fn retention_and_tunnel_deletion_scrub_removed_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("retention.db");
    let db = Db::open(&path).unwrap();
    let expired = "synthetic-expired-request-c6bc53";
    let deleted = "synthetic-deleted-tunnel-906d20";
    let retained = marked_exchange("keep", "t2", "synthetic-retained-tunnel-cc47d4");
    db.upsert_request(&marked_exchange("expired", "t1", expired)).unwrap();
    checkpoint_without_truncating(&path);
    db.sweep(10 * DAY, DAY, u64::MAX).unwrap();
    assert_erased(&path, expired);

    db.upsert_request(&marked_exchange("remove", "t1", deleted)).unwrap();
    db.upsert_request(&retained).unwrap();
    db.insert_logs(&[log("t1", 1, "info", deleted)]).unwrap();
    checkpoint_without_truncating(&path);
    db.delete_tunnel("t1").unwrap();
    assert_erased(&path, deleted);
    assert_eq!(db.get_request("keep").unwrap(), Some(retained));
}
