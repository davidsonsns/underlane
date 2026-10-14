//! SQLite storage: app state, cloudflared logs and captured requests, with TTL and size-cap
//! expiry.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

use crate::model::{LogLine, Group, Settings, Tunnel, TunnelKind};
use crate::proxy::capture::{BodyCapture, CapturedExchange, ExchangeKind};

const SCHEMA_VERSION: i64 = 3;

const MIGRATIONS: [&str; SCHEMA_VERSION as usize] = [
    "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE IF NOT EXISTS projects (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, sort INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE IF NOT EXISTS tunnels (
                    id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    config_path TEXT,
                    origin_url TEXT,
                    inspect INTEGER NOT NULL DEFAULT 0,
                    autostart INTEGER NOT NULL DEFAULT 0,
                    loglevel TEXT NOT NULL DEFAULT 'info',
                    created_at INTEGER NOT NULL,
                    sort INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE IF NOT EXISTS logs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    tunnel_id TEXT NOT NULL,
                    ts INTEGER NOT NULL,
                    level TEXT NOT NULL,
                    message TEXT NOT NULL,
                    fields TEXT NOT NULL DEFAULT '{}');
                 CREATE INDEX IF NOT EXISTS logs_tunnel_id ON logs(tunnel_id, id);
                 CREATE INDEX IF NOT EXISTS logs_ts ON logs(ts);
                 CREATE TABLE IF NOT EXISTS requests (
                    id TEXT PRIMARY KEY,
                    tunnel_id TEXT NOT NULL,
                    rule_index INTEGER NOT NULL,
                    ts INTEGER NOT NULL,
                    duration_ms INTEGER,
                    kind TEXT NOT NULL,
                    method TEXT NOT NULL,
                    host TEXT NOT NULL,
                    path TEXT NOT NULL,
                    status INTEGER,
                    req_headers TEXT NOT NULL,
                    resp_headers TEXT NOT NULL,
                    req_body BLOB,
                    req_size INTEGER NOT NULL DEFAULT 0,
                    req_truncated INTEGER NOT NULL DEFAULT 0,
                    resp_body BLOB,
                    resp_size INTEGER NOT NULL DEFAULT 0,
                    resp_truncated INTEGER NOT NULL DEFAULT 0,
                    error TEXT);
                 CREATE INDEX IF NOT EXISTS requests_tunnel_ts ON requests(tunnel_id, ts);
                 CREATE INDEX IF NOT EXISTS requests_ts ON requests(ts);",
    "ALTER TABLE projects RENAME TO groups;
     ALTER TABLE tunnels RENAME COLUMN project_id TO group_id;",
    "CREATE TABLE tunnels_ungrouped (
        id TEXT PRIMARY KEY,
        group_id TEXT REFERENCES groups(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        kind TEXT NOT NULL,
        config_path TEXT,
        origin_url TEXT,
        inspect INTEGER NOT NULL DEFAULT 0,
        autostart INTEGER NOT NULL DEFAULT 0,
        loglevel TEXT NOT NULL DEFAULT 'info',
        created_at INTEGER NOT NULL,
        sort INTEGER NOT NULL DEFAULT 0);
     INSERT INTO tunnels_ungrouped SELECT * FROM tunnels;
     DROP TABLE tunnels;
     ALTER TABLE tunnels_ungrouped RENAME TO tunnels;",
];

pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Debug, Default, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct SweepStats {
    pub logs_expired: usize,
    pub requests_expired: usize,
    pub logs_capped: usize,
    pub requests_capped: usize,
}

/// A row in the request list (no bodies).
#[derive(Debug, Clone, Serialize)]
pub struct RequestSummary {
    pub id: String,
    pub tunnel_id: String,
    pub ts: i64,
    pub duration_ms: Option<i64>,
    pub kind: String,
    pub method: String,
    pub host: String,
    pub path: String,
    pub status: Option<u16>,
    pub resp_size: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct LogQuery {
    pub tunnel_id: Option<String>,
    /// Empty = all levels.
    pub levels: Vec<String>,
    pub search: Option<String>,
    /// Only rows with id < before_id (paging backwards).
    pub before_id: Option<i64>,
    /// Only rows with id > after_id (live tail).
    pub after_id: Option<i64>,
    pub limit: usize,
}

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> anyhow::Result<Self> {
        Self::init_with_migrations(conn, &MIGRATIONS)
    }

    fn init_with_migrations(mut conn: Connection, migrations: &[&str]) -> anyhow::Result<Self> {
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let supported = migrations.len() as i64;
        anyhow::ensure!(
            version <= supported,
            "database schema version {version} is newer than supported version {supported}"
        );
        // Check compatibility before pragmas that can change the database file.
        // auto_vacuum must be set before the first table exists to take effect.
        conn.execute_batch(
            "PRAGMA auto_vacuum = INCREMENTAL;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        for (index, sql) in migrations.iter().enumerate().skip(version as usize) {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (index + 1) as i64)?;
            tx.commit()?;
        }
        Ok(Db { conn: Mutex::new(conn) })
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> anyhow::Result<T> {
        let conn = self.conn.lock().unwrap();
        Ok(f(&conn)?)
    }

    // ---- settings ----

    /// Clear app records atomically while keeping the live SQLite connection valid.
    pub fn reset(&self) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute_batch("DELETE FROM requests; DELETE FROM logs; DELETE FROM tunnels; DELETE FROM groups; DELETE FROM settings; DELETE FROM sqlite_sequence;")?;
        tx.commit()?;
        Ok(())
    }

    pub fn settings(&self) -> anyhow::Result<Settings> {
        let raw: Option<String> = self.with(|c| {
            c.query_row("SELECT value FROM settings WHERE key = 'settings'", [], |r| r.get(0)).optional()
        })?;
        Ok(raw.and_then(|r| serde_json::from_str(&r).ok()).unwrap_or_default())
    }

    pub fn save_settings(&self, s: &Settings) -> anyhow::Result<()> {
        let json = serde_json::to_string(s)?;
        self.with(|c| {
            c.execute(
                "INSERT INTO settings(key, value) VALUES ('settings', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [json],
            )
        })?;
        Ok(())
    }

    pub fn kv_get(&self, key: &str) -> anyhow::Result<Option<String>> {
        self.with(|c| c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional())
    }

    pub fn kv_set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [key, value],
            )
        })?;
        Ok(())
    }

    // ---- groups & tunnels ----

    pub fn groups(&self) -> anyhow::Result<Vec<Group>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id, name, sort FROM groups ORDER BY sort, name")?;
            let rows = st.query_map([], |r| Ok(Group { id: r.get(0)?, name: r.get(1)?, sort: r.get(2)? }))?;
            rows.collect()
        })
    }

    /// Find a group by name (case-insensitive) or create it.
    pub fn ensure_group(&self, name: &str) -> anyhow::Result<Group> {
        let name = if name.trim().is_empty() { "Default" } else { name };
        let name = normalize_name(name)?;
        let conn = self.conn.lock().unwrap();
        if let Some(p) = group_by_name(&conn, &name, None)? {
            return Ok(p);
        }
        let p = Group {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            sort: conn.query_row("SELECT COALESCE(MAX(sort), -1) + 1 FROM groups", [], |r| r.get(0))?,
        };
        conn.execute("INSERT INTO groups(id, name, sort) VALUES (?1, ?2, ?3)", params![p.id, p.name, p.sort])?;
        Ok(p)
    }

    pub fn rename_group(&self, id: &str, name: &str) -> anyhow::Result<()> {
        let name = normalize_name(name)?;
        let conn = self.conn.lock().unwrap();
        anyhow::ensure!(group_by_name(&conn, &name, Some(id))?.is_none(), "A group with that name already exists.");
        let updated = conn.execute("UPDATE groups SET name = ?1 WHERE id = ?2", params![name, id])?;
        anyhow::ensure!(updated != 0, "Group not found.");
        Ok(())
    }

    /// Delete groups that no longer have tunnels.
    pub fn prune_empty_groups(&self) -> anyhow::Result<()> {
        self.with(|c| c.execute("DELETE FROM groups WHERE id NOT IN (SELECT group_id FROM tunnels WHERE group_id IS NOT NULL)", []))?;
        Ok(())
    }

    fn tunnel_from_row(r: &Row) -> rusqlite::Result<Tunnel> {
        let kind: String = r.get(3)?;
        Ok(Tunnel {
            id: r.get(0)?,
            group_id: r.get(1)?,
            name: r.get(2)?,
            kind: TunnelKind::parse(&kind).unwrap_or(TunnelKind::Config),
            config_path: r.get(4)?,
            origin_url: r.get(5)?,
            inspect: r.get(6)?,
            autostart: r.get(7)?,
            loglevel: r.get(8)?,
            created_at: r.get(9)?,
            sort: r.get(10)?,
        })
    }

    const TUNNEL_COLS: &'static str =
        "id, group_id, name, kind, config_path, origin_url, inspect, autostart, loglevel, created_at, sort";

    pub fn tunnels(&self) -> anyhow::Result<Vec<Tunnel>> {
        self.with(|c| {
            let mut st = c.prepare(&format!("SELECT {} FROM tunnels ORDER BY sort, created_at", Self::TUNNEL_COLS))?;
            let rows = st.query_map([], Self::tunnel_from_row)?;
            rows.collect()
        })
    }

    pub fn tunnel(&self, id: &str) -> anyhow::Result<Option<Tunnel>> {
        self.with(|c| {
            c.query_row(&format!("SELECT {} FROM tunnels WHERE id = ?1", Self::TUNNEL_COLS), [id], Self::tunnel_from_row)
                .optional()
        })
    }

    pub fn tunnel_by_config_path(&self, path: &str) -> anyhow::Result<Option<Tunnel>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {} FROM tunnels WHERE config_path = ?1", Self::TUNNEL_COLS),
                [path],
                Self::tunnel_from_row,
            )
            .optional()
        })
    }

    pub fn upsert_tunnel(&self, t: &Tunnel) -> anyhow::Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO tunnels(id, group_id, name, kind, config_path, origin_url, inspect, autostart, loglevel, created_at, sort)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET group_id = excluded.group_id, name = excluded.name,
                    kind = excluded.kind, config_path = excluded.config_path, origin_url = excluded.origin_url,
                    inspect = excluded.inspect, autostart = excluded.autostart, loglevel = excluded.loglevel,
                    sort = excluded.sort",
                params![
                    t.id,
                    t.group_id,
                    t.name,
                    t.kind.as_str(),
                    t.config_path,
                    t.origin_url,
                    t.inspect,
                    t.autostart,
                    t.loglevel,
                    t.created_at,
                    t.sort
                ],
            )
        })?;
        Ok(())
    }

    /// Register a project and its optional group together; legacy path spellings also count.
    pub(crate) fn register_project(&self, mut tunnel: Tunnel, group: Option<&str>) -> anyhow::Result<Tunnel> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let path = std::path::Path::new(tunnel.config_path.as_deref().expect("project config path"));
        let canonical = std::fs::canonicalize(path)?;
        {
            let mut statement = tx.prepare("SELECT config_path FROM tunnels WHERE config_path IS NOT NULL")?;
            for existing in statement.query_map([], |row| row.get::<_, String>(0))? {
                let existing = existing?;
                let existing = crate::config::expand_tilde(&existing);
                anyhow::ensure!(existing != path && std::fs::canonicalize(&existing).ok().as_deref() != Some(canonical.as_path()),
                    "This config file already belongs to a project. Open that project instead.");
            }
        }
        if let Some(name) = group {
            let group = match group_by_name(&tx, name, None)? {
                Some(group) => group,
                None => {
                    let group = Group {
                        id: uuid::Uuid::new_v4().to_string(), name: name.to_string(),
                        sort: tx.query_row("SELECT COALESCE(MAX(sort), -1) + 1 FROM groups", [], |r| r.get(0))?,
                    };
                    tx.execute("INSERT INTO groups(id, name, sort) VALUES (?1, ?2, ?3)", params![group.id, group.name, group.sort])?;
                    group
                }
            };
            tunnel.group_id = Some(group.id);
        }
        tunnel.sort = tx.query_row("SELECT COALESCE(MAX(sort), -1) + 1 FROM tunnels", [], |r| r.get(0))?;
        tx.execute(
            "INSERT INTO tunnels(id, group_id, name, kind, config_path, origin_url, inspect, autostart, loglevel, created_at, sort)
             VALUES (?1, ?2, ?3, 'config', ?4, NULL, 0, 0, ?5, ?6, ?7)",
            params![tunnel.id, tunnel.group_id, tunnel.name, tunnel.config_path, tunnel.loglevel, tunnel.created_at, tunnel.sort],
        )?;
        tx.commit()?;
        Ok(tunnel)
    }

    pub fn delete_tunnel(&self, id: &str) -> anyhow::Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM tunnels WHERE id = ?1", [id])?;
            c.execute("DELETE FROM logs WHERE tunnel_id = ?1", [id])?;
            c.execute("DELETE FROM requests WHERE tunnel_id = ?1", [id])
        })?;
        self.prune_empty_groups()
    }

    // ---- logs ----

    /// Insert a batch in one transaction. Returns the assigned ids in order.
    pub fn insert_logs(&self, lines: &[LogLine]) -> anyhow::Result<Vec<i64>> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut ids = Vec::with_capacity(lines.len());
        {
            let mut st = tx.prepare_cached("INSERT INTO logs(tunnel_id, ts, level, message, fields) VALUES (?1, ?2, ?3, ?4, ?5)")?;
            for l in lines {
                st.execute(params![l.tunnel_id, l.ts, l.level, l.message, l.fields])?;
                ids.push(tx.last_insert_rowid());
            }
        }
        tx.commit()?;
        Ok(ids)
    }

    /// Newest-last page of logs matching the query.
    pub fn query_logs(&self, q: &LogQuery) -> anyhow::Result<Vec<LogLine>> {
        let mut sql = String::from("SELECT id, tunnel_id, ts, level, message, fields FROM logs WHERE 1=1");
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(t) = &q.tunnel_id {
            sql.push_str(" AND tunnel_id = ?");
            args.push(t.clone().into());
        }
        if !q.levels.is_empty() {
            sql.push_str(&format!(" AND level IN ({})", vec!["?"; q.levels.len()].join(",")));
            args.extend(q.levels.iter().map(|l| rusqlite::types::Value::from(l.clone())));
        }
        if let Some(s) = q.search.as_ref().filter(|s| !s.trim().is_empty()) {
            sql.push_str(" AND (message LIKE ? ESCAPE '\\' OR fields LIKE ? ESCAPE '\\')");
            let pat = format!("%{}%", escape_like(s.trim()));
            args.push(pat.clone().into());
            args.push(pat.into());
        }
        if let Some(b) = q.before_id {
            sql.push_str(" AND id < ?");
            args.push(b.into());
        }
        if let Some(a) = q.after_id {
            sql.push_str(" AND id > ?");
            args.push(a.into());
        }
        sql.push_str(" ORDER BY id DESC LIMIT ?");
        args.push((q.limit.clamp(1, 5000) as i64).into());
        let mut rows: Vec<LogLine> = self.with(|c| {
            let mut st = c.prepare(&sql)?;
            let rows = st.query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok(LogLine {
                    id: r.get(0)?,
                    tunnel_id: r.get(1)?,
                    ts: r.get(2)?,
                    level: r.get(3)?,
                    message: r.get(4)?,
                    fields: r.get(5)?,
                })
            })?;
            rows.collect()
        })?;
        rows.reverse();
        Ok(rows)
    }

    pub fn clear_logs(&self, tunnel_id: Option<&str>) -> anyhow::Result<()> {
        self.with(|c| match tunnel_id {
            Some(t) => c.execute("DELETE FROM logs WHERE tunnel_id = ?1", [t]),
            None => c.execute("DELETE FROM logs", []),
        })?;
        Ok(())
    }

    // ---- requests ----

    /// Insert or replace (long-lived exchanges are written when they open and again when they close).
    pub fn upsert_request(&self, x: &CapturedExchange) -> anyhow::Result<()> {
        let req_headers = serde_json::to_string(&x.req_headers)?;
        let resp_headers = serde_json::to_string(&x.resp_headers)?;
        self.with(|c| {
            c.execute(
                "INSERT INTO requests(id, tunnel_id, rule_index, ts, duration_ms, kind, method, host, path, status,
                    req_headers, resp_headers, req_body, req_size, req_truncated, resp_body, resp_size, resp_truncated, error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
                 ON CONFLICT(id) DO UPDATE SET duration_ms = excluded.duration_ms, kind = excluded.kind,
                    status = excluded.status, resp_headers = excluded.resp_headers,
                    req_body = excluded.req_body, req_size = excluded.req_size, req_truncated = excluded.req_truncated,
                    resp_body = excluded.resp_body, resp_size = excluded.resp_size,
                    resp_truncated = excluded.resp_truncated, error = excluded.error",
                params![
                    x.id,
                    x.tunnel_id,
                    x.rule_index as i64,
                    x.started_at_ms,
                    x.duration_ms,
                    x.kind.as_str(),
                    x.method,
                    x.host,
                    x.path,
                    x.status,
                    req_headers,
                    resp_headers,
                    (!x.req_body.bytes.is_empty()).then_some(&x.req_body.bytes),
                    x.req_body.size as i64,
                    x.req_body.truncated,
                    (!x.resp_body.bytes.is_empty()).then_some(&x.resp_body.bytes),
                    x.resp_body.size as i64,
                    x.resp_body.truncated,
                    x.error
                ],
            )
        })?;
        Ok(())
    }

    pub fn list_requests(&self, tunnel_id: Option<&str>, search: Option<&str>, limit: usize) -> anyhow::Result<Vec<RequestSummary>> {
        self.list_requests_filtered(tunnel_id, search, &[], &[], limit)
    }

    pub fn list_requests_filtered(
        &self, tunnel_id: Option<&str>, search: Option<&str>, statuses: &[u16], methods: &[String], limit: usize,
    ) -> anyhow::Result<Vec<RequestSummary>> {
        let mut sql = String::from(
            "SELECT id, tunnel_id, ts, duration_ms, kind, method, host, path, status, resp_size, error FROM requests WHERE 1=1",
        );
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(t) = tunnel_id {
            sql.push_str(" AND tunnel_id = ?");
            args.push(t.to_string().into());
        }
        if let Some(s) = search.filter(|s| !s.trim().is_empty()) {
            sql.push_str(" AND (path LIKE ? ESCAPE '\\' OR method LIKE ? ESCAPE '\\' OR host LIKE ? ESCAPE '\\' OR CAST(status AS TEXT) = ?)");
            let pat = format!("%{}%", escape_like(s.trim()));
            args.push(pat.clone().into());
            args.push(pat.clone().into());
            args.push(pat.into());
            args.push(s.trim().to_string().into());
        }
        if !statuses.is_empty() {
            sql.push_str(&format!(" AND status IN ({})", vec!["?"; statuses.len()].join(",")));
            args.extend(statuses.iter().map(|s| rusqlite::types::Value::from(i64::from(*s))));
        }
        if !methods.is_empty() {
            sql.push_str(&format!(" AND method IN ({})", vec!["?"; methods.len()].join(",")));
            args.extend(methods.iter().map(|m| rusqlite::types::Value::from(m.clone())));
        }
        sql.push_str(" ORDER BY ts DESC LIMIT ?");
        args.push((limit.clamp(1, 5000) as i64).into());
        self.with(|c| {
            let mut st = c.prepare(&sql)?;
            let rows = st.query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok(RequestSummary {
                    id: r.get(0)?,
                    tunnel_id: r.get(1)?,
                    ts: r.get(2)?,
                    duration_ms: r.get(3)?,
                    kind: r.get(4)?,
                    method: r.get(5)?,
                    host: r.get(6)?,
                    path: r.get(7)?,
                    status: r.get(8)?,
                    resp_size: r.get::<_, i64>(9)? as u64,
                    error: r.get(10)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn get_request(&self, id: &str) -> anyhow::Result<Option<CapturedExchange>> {
        self.with(|c| {
            c.query_row(
                "SELECT id, tunnel_id, rule_index, ts, duration_ms, kind, method, host, path, status, req_headers, resp_headers,
                        req_body, req_size, req_truncated, resp_body, resp_size, resp_truncated, error
                 FROM requests WHERE id = ?1",
                [id],
                |r| {
                    let kind: String = r.get(5)?;
                    let req_h: String = r.get(10)?;
                    let resp_h: String = r.get(11)?;
                    Ok(CapturedExchange {
                        id: r.get(0)?,
                        tunnel_id: r.get(1)?,
                        rule_index: r.get::<_, i64>(2)? as usize,
                        started_at_ms: r.get(3)?,
                        duration_ms: r.get(4)?,
                        kind: match kind.as_str() {
                            "websocket" => ExchangeKind::Websocket,
                            "sse" => ExchangeKind::Sse,
                            _ => ExchangeKind::Http,
                        },
                        method: r.get(6)?,
                        host: r.get(7)?,
                        path: r.get(8)?,
                        status: r.get(9)?,
                        req_headers: serde_json::from_str(&req_h).unwrap_or_default(),
                        resp_headers: serde_json::from_str(&resp_h).unwrap_or_default(),
                        req_body: BodyCapture {
                            bytes: r.get::<_, Option<Vec<u8>>>(12)?.unwrap_or_default(),
                            size: r.get::<_, i64>(13)? as u64,
                            truncated: r.get(14)?,
                        },
                        resp_body: BodyCapture {
                            bytes: r.get::<_, Option<Vec<u8>>>(15)?.unwrap_or_default(),
                            size: r.get::<_, i64>(16)? as u64,
                            truncated: r.get(17)?,
                        },
                        error: r.get(18)?,
                    })
                },
            )
            .optional()
        })
    }

    pub fn clear_requests(&self, tunnel_id: Option<&str>) -> anyhow::Result<()> {
        self.with(|c| match tunnel_id {
            Some(t) => c.execute("DELETE FROM requests WHERE tunnel_id = ?1", [t]),
            None => c.execute("DELETE FROM requests", []),
        })?;
        Ok(())
    }

    // ---- expiry ----

    /// Bytes used by live pages (excludes the free list).
    pub fn used_bytes(&self) -> anyhow::Result<u64> {
        self.with(|c| {
            let page_size: i64 = c.query_row("PRAGMA page_size", [], |r| r.get(0))?;
            let pages: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0))?;
            let free: i64 = c.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
            Ok(((pages - free).max(0) * page_size) as u64)
        })
    }

    /// Delete logs/requests older than `ttl_ms` (relative to `now_ms`), then delete the oldest
    /// rows until live data fits in `max_bytes`, then return freed pages to the OS.
    pub fn sweep(&self, now_ms: i64, ttl_ms: i64, max_bytes: u64) -> anyhow::Result<SweepStats> {
        let mut stats = SweepStats::default();
        let cutoff = now_ms - ttl_ms;
        // Delete in small batches so the connection lock is never held for long (the UI and
        // the log writer share it).
        loop {
            let n = self.with(|c| c.execute("DELETE FROM logs WHERE id IN (SELECT id FROM logs WHERE ts < ?1 LIMIT 2000)", [cutoff]))?;
            stats.logs_expired += n;
            if n == 0 {
                break;
            }
        }
        loop {
            let n = self.with(|c| c.execute("DELETE FROM requests WHERE id IN (SELECT id FROM requests WHERE ts < ?1 LIMIT 200)", [cutoff]))?;
            stats.requests_expired += n;
            if n == 0 {
                break;
            }
        }

        let mut guard = 0;
        while self.used_bytes()? > max_bytes && guard < 10_000 {
            guard += 1;
            let (oldest_log, oldest_req): (Option<i64>, Option<i64>) = self.with(|c| {
                Ok((
                    c.query_row("SELECT MIN(ts) FROM logs", [], |r| r.get(0))?,
                    c.query_row("SELECT MIN(ts) FROM requests", [], |r| r.get(0))?,
                ))
            })?;
            let n = match (oldest_log, oldest_req) {
                (None, None) => break,
                (Some(l), Some(r)) if r <= l => self.delete_oldest_requests(&mut stats)?,
                (None, Some(_)) => self.delete_oldest_requests(&mut stats)?,
                _ => self.delete_oldest_logs(&mut stats)?,
            };
            if n == 0 {
                break;
            }
        }
        self.with(|c| c.execute_batch("PRAGMA incremental_vacuum; PRAGMA wal_checkpoint(TRUNCATE);"))?;
        Ok(stats)
    }

    fn delete_oldest_logs(&self, stats: &mut SweepStats) -> anyhow::Result<usize> {
        let n = self.with(|c| c.execute("DELETE FROM logs WHERE id IN (SELECT id FROM logs ORDER BY ts LIMIT 200)", []))?;
        stats.logs_capped += n;
        Ok(n)
    }

    fn delete_oldest_requests(&self, stats: &mut SweepStats) -> anyhow::Result<usize> {
        let n = self.with(|c| c.execute("DELETE FROM requests WHERE id IN (SELECT id FROM requests ORDER BY ts LIMIT 10)", []))?;
        stats.requests_capped += n;
        Ok(n)
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

pub fn normalize_name(name: &str) -> anyhow::Result<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    anyhow::ensure!(!name.is_empty(), "Name cannot be empty.");
    anyhow::ensure!(name.chars().count() <= 64, "Name must be 64 characters or fewer.");
    Ok(name)
}

fn group_by_name(conn: &Connection, name: &str, exclude_id: Option<&str>) -> rusqlite::Result<Option<Group>> {
    let name = name.to_lowercase();
    let mut statement = conn.prepare("SELECT id, name, sort FROM groups WHERE ?1 IS NULL OR id != ?1")?;
    let groups = statement.query_map([exclude_id], |r| Ok(Group { id: r.get(0)?, name: r.get(1)?, sort: r.get(2)? }))?;
    for group in groups {
        let group = group?;
        if group.name.to_lowercase() == name {
            return Ok(Some(group));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod group_tests {
    use super::*;

    #[test]
    fn ensure_group_normalizes_whitespace_and_reuses_case_insensitive_names() {
        let db = Db::open_in_memory().unwrap();
        let group = db.ensure_group(" \tÉquipe\u{a0}  Web\n ").unwrap();
        assert_eq!(group.name, "Équipe Web");
        assert_eq!(db.ensure_group("éQUIPE   web").unwrap().id, group.id);
        assert_eq!(db.groups().unwrap().len(), 1);
        let default = db.ensure_group(" \t\n\u{a0}").unwrap();
        assert_eq!(default.name, "Default");
        assert_eq!(db.ensure_group("").unwrap().id, default.id);
    }

    #[test]
    fn group_name_limits_count_unicode_characters_after_normalization() {
        let db = Db::open_in_memory().unwrap();
        let name = "💻".repeat(64);
        let group = db.ensure_group(&format!("\t{name} \n")).unwrap();
        assert_eq!(group.name, name);
        assert!(db.ensure_group(&"💻".repeat(65)).is_err());
        for invalid in [" \t\n\u{a0}".to_string(), "💻".repeat(65)] {
            assert!(db.rename_group(&group.id, &invalid).is_err());
        }
        assert_eq!(db.groups().unwrap().len(), 1);
        assert_eq!(db.groups().unwrap()[0].name, name);
    }

    #[test]
    fn rename_group_preserves_identity_sort_and_tunnel_membership_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("groups.db");
        let db = Db::open(&path).unwrap();
        let tunnel = crate::import::add_simple(&db, TunnelKind::Quick, "Web", "Old", Some("http://localhost:3000".into()), "info").unwrap();
        let group = db.groups().unwrap().remove(0);
        db.rename_group(&group.id, " \tNew   project\n ").unwrap();
        drop(db);

        let db = Db::open(&path).unwrap();
        let renamed = db.groups().unwrap().remove(0);
        assert_eq!(renamed.id, group.id);
        assert_eq!(renamed.sort, group.sort);
        assert_eq!(renamed.name, "New project");
        assert_eq!(db.tunnel(&tunnel.id).unwrap().unwrap(), tunnel);
    }

    #[test]
    fn rename_group_rejects_duplicates_but_allows_its_own_name() {
        let db = Db::open_in_memory().unwrap();
        let first = db.ensure_group("First").unwrap();
        let second = db.ensure_group("Équipe Web").unwrap();
        let error = db.rename_group(&first.id, "éQUIPE  web").unwrap_err();
        assert_eq!(error.to_string(), "A group with that name already exists.");
        assert_eq!(db.groups().unwrap()[0].name, "First");
        assert_eq!(db.groups().unwrap()[1].id, second.id);
        db.rename_group(&first.id, "First").unwrap();
        db.rename_group(&first.id, " FIRST ").unwrap();
        assert_eq!(db.groups().unwrap()[0].name, "FIRST");
        assert_eq!(db.rename_group("missing", "New").unwrap_err().to_string(), "Group not found.");
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    use rusqlite::types::Value;

    fn rows(conn: &Connection, sql: &str) -> Vec<Vec<Value>> {
        let mut statement = conn.prepare(sql).unwrap();
        let columns = statement.column_count();
        statement
            .query_map([], |row| (0..columns).map(|index| row.get(index)).collect())
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    fn snapshot(conn: &Connection) -> (Vec<Vec<Value>>, Vec<Vec<Vec<Value>>>, i64) {
        let schema = rows(
            conn,
            "SELECT type, name, tbl_name, sql FROM sqlite_schema ORDER BY name",
        );
        let group_table = if conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'groups' AND type = 'table')",
            [],
            |row| row.get::<_, bool>(0),
        ).unwrap() { "groups" } else { "projects" };
        let data = ["settings", group_table, "tunnels", "logs", "requests", "sqlite_sequence"]
            .iter()
            .map(|table| rows(conn, &format!("SELECT * FROM {table} ORDER BY rowid")))
            .collect();
        let version = conn.query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap();
        (schema, data, version)
    }

    fn populated_v1(path: &Path) -> Connection {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.execute_batch(
            "INSERT INTO settings VALUES ('settings', '{\"theme\":\"dark\"}');
             INSERT INTO settings VALUES ('fixture', 'preserve me');
             INSERT INTO projects VALUES ('p1', 'Shop', 7);
             INSERT INTO tunnels VALUES ('t1', 'p1', 'web', 'quick', NULL, 'http://localhost:3000', 1, 1, 'debug', 123, 4);
             INSERT INTO logs VALUES (42, 't1', 456, 'info', 'fixture log', '{\"key\":\"value\"}');
             INSERT INTO requests VALUES ('r1', 't1', 2, 789, 12, 'http', 'POST', 'app.example.com', '/items', 201,
                '[[\"content-type\",\"application/json\"]]', '[]', X'010203', 5, 1, X'040506', 3, 0, NULL);
             PRAGMA user_version = 1;",
        )
        .unwrap();
        conn
    }

    #[test]
    fn fresh_creation_uses_current_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fresh.db");
        let db = Db::open(&path).unwrap();
        let conn = db.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        assert_eq!(
            rows(
                &conn,
                "SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name"
            ),
            ["groups", "logs", "requests", "settings", "sqlite_sequence", "tunnels"]
                .map(|name| vec![Value::Text(name.into())])
        );
        assert!(snapshot(&conn).1.iter().all(Vec::is_empty));
        assert_eq!(
            conn.query_row("PRAGMA auto_vacuum", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(Db::open_in_memory().unwrap().settings().unwrap(), Settings::default());
    }

    #[test]
    fn populated_v1_renames_groups_and_repeated_opens_preserve_every_row() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.db");
        let conn = populated_v1(&path);
        let before = snapshot(&conn);
        drop(conn);
        let mut migrated = None;
        for _ in 0..3 {
            let db = Db::open(&path).unwrap();
            let after = snapshot(&db.conn.lock().unwrap());
            assert_eq!(after.1, before.1, "migration preserves all stored rows");
            assert_eq!(after.2, SCHEMA_VERSION);
            if let Some(previous) = &migrated {
                assert_eq!(&after, previous, "reopening does not repeat migrations");
            }
            migrated = Some(after);
            assert_eq!(db.settings().unwrap().theme, "dark");
            assert_eq!(db.groups().unwrap()[0].name, "Shop");
            assert_eq!(db.tunnel("t1").unwrap().unwrap().group_id.as_deref(), Some("p1"));
            assert_eq!(db.tunnel("t1").unwrap().unwrap().sort, 4);
            let conn = db.conn.lock().unwrap();
            assert!(rows(&conn, "SELECT name FROM sqlite_schema WHERE name = 'projects'").is_empty());
            let columns = rows(&conn, "SELECT name FROM pragma_table_info('tunnels')");
            assert!(columns.contains(&vec![Value::Text("group_id".into())]));
            assert!(!columns.contains(&vec![Value::Text("project_id".into())]));
            assert_eq!(
                rows(&conn, "SELECT [table], [from], [to], on_delete FROM pragma_foreign_key_list('tunnels')"),
                vec![vec![Value::Text("groups".into()), Value::Text("group_id".into()), Value::Text("id".into()), Value::Text("CASCADE".into())]]
            );
            assert!(rows(&conn, "PRAGMA foreign_key_check").is_empty());
            assert!(conn.execute("UPDATE tunnels SET group_id = 'missing' WHERE id = 't1'", []).is_err());
            drop(conn);
            assert_eq!(
                db.query_logs(&LogQuery {
                    limit: 10,
                    ..Default::default()
                })
                .unwrap()[0]
                    .message,
                "fixture log"
            );
            assert_eq!(db.get_request("r1").unwrap().unwrap().req_body.bytes, vec![1, 2, 3]);
        }
    }

    #[test]
    fn populated_v2_allows_ungrouped_tunnels_without_losing_data_or_constraints() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grouped.db");
        let conn = populated_v1(&path);
        conn.execute_batch(MIGRATIONS[1]).unwrap();
        conn.execute_batch(
            "INSERT INTO tunnels VALUES ('config', 'p1', 'Config', 'config', '/tmp/config.yml', NULL, 0, 1, 'warn', 321, 9);
             PRAGMA user_version = 2;",
        ).unwrap();
        let before = snapshot(&conn);
        let columns = rows(&conn, "SELECT name, type, [notnull], dflt_value, pk FROM pragma_table_info('tunnels')");
        let foreign_keys = rows(&conn, "PRAGMA foreign_key_list('tunnels')");
        drop(conn);

        let db = Db::open(&path).unwrap();
        let conn = db.conn.lock().unwrap();
        assert_eq!(snapshot(&conn).1, before.1);
        assert_eq!(snapshot(&conn).2, SCHEMA_VERSION);
        let mut expected_columns = columns;
        expected_columns[1][2] = Value::Integer(0);
        assert_eq!(rows(&conn, "SELECT name, type, [notnull], dflt_value, pk FROM pragma_table_info('tunnels')"), expected_columns);
        assert_eq!(rows(&conn, "PRAGMA foreign_key_list('tunnels')"), foreign_keys);
        assert!(rows(&conn, "PRAGMA foreign_key_check").is_empty());
        assert!(conn.execute("UPDATE tunnels SET group_id = 'missing' WHERE id = 't1'", []).is_err());
        drop(conn);
        assert_eq!(db.tunnel_by_config_path("/tmp/config.yml").unwrap().unwrap().id, "config");

        let mut tunnel = db.tunnel("t1").unwrap().unwrap();
        let original = tunnel.clone();
        tunnel.group_id = None;
        db.upsert_tunnel(&tunnel).unwrap();
        db.prune_empty_groups().unwrap();
        assert_eq!(db.groups().unwrap().len(), 1, "the config tunnel still uses its group");
        let empty = db.ensure_group("Empty").unwrap();
        db.prune_empty_groups().unwrap();
        assert!(!db.groups().unwrap().iter().any(|group| group.id == empty.id), "NULL membership must not prevent pruning");
        assert_eq!(db.tunnel("t1").unwrap().unwrap(), tunnel);
        assert_eq!(serde_json::to_value(&tunnel).unwrap()["group_id"], serde_json::Value::Null);
        assert_eq!(db.get_request("r1").unwrap().unwrap().req_body.bytes, vec![1, 2, 3]);
        drop(db);

        let db = Db::open(&path).unwrap();
        assert_eq!(db.tunnel("t1").unwrap().unwrap(), tunnel, "ungrouped membership survives reopening");
        let new_group = db.ensure_group("Reassigned").unwrap();
        tunnel.group_id = Some(new_group.id.clone());
        db.upsert_tunnel(&tunnel).unwrap();
        assert_eq!(db.tunnel("t1").unwrap().unwrap(), tunnel);
        let conn = db.conn.lock().unwrap();
        conn.execute("DELETE FROM groups WHERE id = ?1", [&new_group.id]).unwrap();
        drop(conn);
        assert!(db.tunnel("t1").unwrap().is_none(), "group deletion still cascades");
        assert_eq!(db.tunnel("config").unwrap().unwrap().group_id, original.group_id);
    }

    #[test]
    fn clearing_last_group_preserves_the_tunnel() {
        let db = Db::open_in_memory().unwrap();
        let mut tunnel = crate::import::add_simple(&db, TunnelKind::Quick, "Quick", "Personal", Some("http://localhost:1".into()), "info").unwrap();
        tunnel.group_id = None;
        db.upsert_tunnel(&tunnel).unwrap();
        db.prune_empty_groups().unwrap();
        assert!(db.groups().unwrap().is_empty());
        assert_eq!(db.tunnels().unwrap(), vec![tunnel]);
    }

    #[test]
    fn failed_step_rolls_back_schema_rows_and_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollback.db");
        let conn = populated_v1(&path);
        let before = snapshot(&conn);
        let failing_v2 = format!("{} ALTER TABLE groups ADD COLUMN test_column TEXT;
                          UPDATE settings SET value = 'changed' WHERE key = 'fixture';
                          CREATE TABLE test_table (group_id TEXT REFERENCES groups(id) DEFERRABLE INITIALLY DEFERRED);
                          INSERT INTO test_table VALUES ('missing-group');", MIGRATIONS[1]);
        let error = Db::init_with_migrations(conn, &[MIGRATIONS[0], &failing_v2])
            .err()
            .unwrap();
        assert!(error.to_string().contains("FOREIGN KEY constraint failed"));
        let conn = Connection::open(&path).unwrap();
        assert_eq!(snapshot(&conn), before);
    }

    #[test]
    fn failed_fresh_creation_rolls_back_all_ddl_and_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollback-fresh.db");
        let failing_v1 = format!("{} INSERT INTO nonexistent_table VALUES (1);", MIGRATIONS[0]);
        assert!(Db::init_with_migrations(Connection::open(&path).unwrap(), &[&failing_v1]).is_err());
        let conn = Connection::open(&path).unwrap();
        assert!(rows(&conn, "SELECT name FROM sqlite_schema").is_empty());
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn test_only_v4_advances_in_order_and_does_not_repeat() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("advance.db");
        let conn = populated_v1(&path);
        let before = snapshot(&conn);
        let test_v4 = "CREATE TABLE migration_probe (value TEXT NOT NULL);
                       INSERT INTO migration_probe SELECT value FROM settings WHERE key = 'fixture';";
        let migrations = [MIGRATIONS[0], MIGRATIONS[1], MIGRATIONS[2], test_v4];
        let db = Db::init_with_migrations(conn, &migrations).unwrap();
        let after = snapshot(&db.conn.lock().unwrap());
        assert_eq!(after.1, before.1);
        assert_eq!(after.2, 4);
        assert_eq!(
            rows(&db.conn.lock().unwrap(), "SELECT value FROM migration_probe"),
            vec![vec![Value::Text("preserve me".into())]]
        );
        drop(db);
        let db = Db::init_with_migrations(Connection::open(&path).unwrap(), &migrations).unwrap();
        assert_eq!(snapshot(&db.conn.lock().unwrap()), after);

        let fresh = Db::init_with_migrations(Connection::open_in_memory().unwrap(), &migrations).unwrap();
        assert_eq!(snapshot(&fresh.conn.lock().unwrap()).2, 4);
    }

    #[test]
    fn future_version_is_rejected_before_any_database_write() {
        // The empty-schema case catches auto_vacuum writes as well as journal-mode changes.
        for schema in [
            "",
            "CREATE TABLE future_data (value TEXT); INSERT INTO future_data VALUES ('keep');",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("future.db");
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(schema).unwrap();
            conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1).unwrap();
            drop(conn);
            let before = std::fs::read(&path).unwrap();
            let error = Db::open(&path).err().unwrap();
            assert!(error
                .to_string()
                .contains(&format!("schema version {} is newer than supported version {SCHEMA_VERSION}", SCHEMA_VERSION + 1)));
            assert_eq!(std::fs::read(&path).unwrap(), before);
            assert_eq!(
                std::fs::read_dir(dir.path()).unwrap().count(),
                1,
                "no WAL or journal files created"
            );
        }
    }
}
