use std::path::Path;

use underlane_core::db::Db;
use underlane_core::projects::{available_configs, create, default_directory, needs_setup, preview};

const SOURCE: &str = "tunnel: example-id\ncredentials-file: ./credentials.json\ningress:\n  - hostname: app.example.com\n    service: http://localhost:3000\n  - service: http_status:404\n";

fn source(directory: &Path) -> std::path::PathBuf {
    let path = directory.join("source.yaml");
    std::fs::write(&path, SOURCE).unwrap();
    path
}

#[test]
fn discovers_only_valid_unused_configs_including_nested_files() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open_in_memory().unwrap();
    let used = source(directory.path());
    let mut registered = create(&db, "Used", "", Some(&used), None, "info").unwrap();
    registered.config_path = Some(directory.path().join("./source.yaml").to_string_lossy().into_owned());
    db.upsert_tunnel(&registered).unwrap();
    let nested = directory.path().join("underlane");
    std::fs::create_dir(&nested).unwrap();
    let unused = source(&nested);
    let single_origin = directory.path().join("local.YML");
    std::fs::write(&single_origin, "url: http://localhost:3000").unwrap();
    for (filename, contents) in [
        ("broken.yml", "ingress: ["),
        ("unrelated.yml", "services: {}"),
        ("invalid.yml", "ingress: [{service: false}]"),
        ("credentials.yml", "credentials-file: private.json"),
        ("wrong.txt", SOURCE),
    ] {
        std::fs::write(directory.path().join(filename), contents).unwrap();
    }
    #[cfg(unix)] {
        use std::os::unix::fs::symlink;
        symlink(&used, directory.path().join("used-alias.yml")).unwrap();
        symlink(directory.path(), nested.join("cycle")).unwrap();
    }
    let paths: Vec<_> = available_configs(&db, directory.path()).unwrap().into_iter().map(|c| c.source).collect();
    assert_eq!(paths, vec![single_origin, unused.clone()]);
    create(&db, "New", "", Some(&unused), None, "info").unwrap();
    assert_eq!(available_configs(&db, directory.path()).unwrap().len(), 1);
    assert!(available_configs(&db, &directory.path().join("missing")).unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(used).unwrap(), SOURCE);
}

#[cfg(unix)]
#[test]
fn discovery_expands_registered_tildes_and_skips_unreadable_subdirectories() {
    use std::os::unix::fs::PermissionsExt;
    let home = underlane_core::config::home_dir();
    let directory = tempfile::tempdir_in(&home).unwrap();
    let db = Db::open_in_memory().unwrap();
    let used = source(directory.path());
    let mut registered = create(&db, "Used", "", Some(&used), None, "info").unwrap();
    registered.config_path = Some(format!("~/{}", used.strip_prefix(&home).unwrap().display()));
    db.upsert_tunnel(&registered).unwrap();
    let available = directory.path().join("available.yml");
    std::fs::write(&available, SOURCE).unwrap();
    let inaccessible = directory.path().join("private");
    std::fs::create_dir(&inaccessible).unwrap();
    std::fs::set_permissions(&inaccessible, std::fs::Permissions::from_mode(0o000)).unwrap();
    let result = available_configs(&db, directory.path());
    std::fs::set_permissions(&inaccessible, std::fs::Permissions::from_mode(0o700)).unwrap();
    let paths: Vec<_> = result.unwrap().into_iter().map(|c| c.source).collect();
    assert_eq!(paths, vec![available]);
}

#[test]
fn previews_routes_without_secrets_and_preserves_existing_source() {
    let directory = tempfile::tempdir().unwrap();
    let path = source(directory.path());
    let preview = preview(&path).unwrap();
    assert_eq!(preview.name, "source");
    assert_eq!(preview.path, path.to_string_lossy());
    assert_eq!(preview.routes.len(), 2);
    assert_eq!(preview.routes[0].hostname.as_deref(), Some("app.example.com"));
    assert_eq!(preview.routes[1].service, "http_status:404");
    let db = Db::open_in_memory().unwrap();
    let project = create(&db, "  My  project  ", "", Some(&path), None, "info").unwrap();
    assert_eq!(project.name, "My project");
    assert_eq!(project.group_id, None);
    assert!(!project.autostart);
    assert!(project.inspect);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), SOURCE);
    let config = underlane_core::config::TunnelConfig::load(&path).unwrap();
    assert_eq!(config.credentials_file.unwrap(), directory.path().join("./credentials.json"));
    assert!(db.groups().unwrap().is_empty());
    assert_eq!(db.tunnel(&project.id).unwrap(), Some(project));
}

#[test]
fn rejects_invalid_files_without_echoing_yaml_values() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.yml");
    for (text, expected) in [
        ("services: {}", "does not look like"),
        ("- ingress", "mapping"),
        ("ingress: invented-secret", "list"),
        ("ingress: [{service: ''}]", "empty service"),
        ("ingress: [{service: false}]", "must be a string"),
        ("tunnel: true", "must be a string"),
        ("ingress: [{service: http://localhost:1, secret: [invented-secret}", "not valid YAML"),
    ] {
        std::fs::write(&path, text).unwrap();
        let error = preview(&path).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("invented-secret"), "{error}");
    }
    assert!(preview(directory.path()).unwrap_err().to_string().contains("regular file"));
    let wrong_extension = directory.path().join("config.txt");
    std::fs::write(&wrong_extension, SOURCE).unwrap();
    assert!(preview(&wrong_extension).unwrap_err().to_string().contains(".yml"));
    assert!(preview(&directory.path().join("missing.yml")).is_err());
}

#[test]
fn create_revalidates_the_selected_source() {
    let directory = tempfile::tempdir().unwrap();
    let path = source(directory.path());
    preview(&path).unwrap();
    std::fs::write(&path, "ingress: [").unwrap();
    let db = Db::open_in_memory().unwrap();
    assert!(create(&db, "App", "", Some(&path), None, "info").is_err());
    assert!(db.tunnels().unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "ingress: [");
}

#[test]
fn drafts_use_safe_names_and_collisions_preserve_existing_files() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open_in_memory().unwrap();
    for (name, filename) in [("../../ App / Café", "app-café.yml"), ("🦆", "project.yml"), ("CON", "project-con.yml")] {
        let tunnel = create(&db, name, "", None, Some(directory.path()), "info").unwrap();
        assert_eq!(Path::new(tunnel.config_path.as_deref().unwrap()).file_name().unwrap(), filename);
        let config = underlane_core::config::TunnelConfig::load(Path::new(tunnel.config_path.as_deref().unwrap())).unwrap();
        assert!(needs_setup(&config));
        assert_eq!(config.ingress.len(), 1);
        assert_eq!(config.ingress[0].service, "http_status:404");
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(config.source).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
    let path = directory.path().join("existing.yml");
    std::fs::write(&path, "user file").unwrap();
    assert!(create(&db, "Existing", "", None, Some(directory.path()), "info").unwrap_err().to_string().contains("already exists"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "user file");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 4);
    let nested = directory.path().join("does-not-exist");
    assert!(create(&db, "Missing", "", None, Some(&nested), "info").is_err());
    assert!(!nested.exists());
}

#[test]
fn group_membership_and_projects_persist_after_reload() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("test.db");
    let db = Db::open(&database).unwrap();
    let first = create(&db, "First", "  Client  work ", None, Some(directory.path()), "debug").unwrap();
    assert!(first.inspect);
    let second = create(&db, "Second", "client work", None, Some(directory.path()), "info").unwrap();
    let third = create(&db, "Third", "  ", None, Some(directory.path()), "info").unwrap();
    assert_eq!(first.group_id, second.group_id);
    assert_eq!(third.group_id, None);
    assert_eq!(db.groups().unwrap().len(), 1);
    drop(db);
    let reloaded = Db::open(&database).unwrap();
    assert_eq!(reloaded.tunnels().unwrap(), vec![first, second, third]);
}

#[test]
fn duplicate_legacy_path_alias_does_not_change_the_existing_project() {
    let directory = tempfile::tempdir().unwrap();
    let path = source(directory.path());
    let db = Db::open_in_memory().unwrap();
    let existing = create(&db, "Original", "", Some(&path), None, "info").unwrap();
    let mut legacy = existing.clone();
    legacy.config_path = Some(directory.path().join("./source.yaml").to_string_lossy().into_owned());
    db.upsert_tunnel(&legacy).unwrap();
    assert!(create(&db, "Replacement", "Other", Some(&path), None, "info").unwrap_err().to_string().contains("already belongs"));
    assert_eq!(db.tunnels().unwrap(), vec![legacy]);
    assert!(db.groups().unwrap().is_empty());
    #[cfg(unix)] {
        let alias = directory.path().join("alias.yml");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        assert!(create(&db, "Alias", "", Some(&alias), None, "info").is_err());
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), SOURCE);
}

#[cfg(unix)]
#[test]
fn selected_config_symlinks_keep_their_parent_for_relative_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let shared = directory.path().join("shared");
    let selected_directory = directory.path().join("project/.cloudflared");
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::create_dir_all(&selected_directory).unwrap();
    let target = source(&shared);
    let selected = selected_directory.join("config.yml");
    std::os::unix::fs::symlink(&target, &selected).unwrap();
    let preview = preview(&selected).unwrap();
    assert_eq!(preview.path, selected.to_string_lossy());
    let db = Db::open_in_memory().unwrap();
    let tunnel = create(&db, "Selected", "", Some(&selected), None, "info").unwrap();
    assert_eq!(tunnel.config_path.as_deref(), selected.to_str());
    let config = underlane_core::config::TunnelConfig::load(Path::new(tunnel.config_path.as_deref().unwrap())).unwrap();
    assert_eq!(config.credentials_file.unwrap(), selected_directory.join("./credentials.json"));
    assert!(create(&db, "Target", "", Some(&target), None, "info").unwrap_err().to_string().contains("already belongs"));
    assert_eq!(std::fs::read_to_string(target).unwrap(), SOURCE);
}

#[test]
fn database_failure_rolls_back_the_new_draft_and_group() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("test.db");
    let db = Db::open(&database).unwrap();
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_project BEFORE INSERT ON tunnels BEGIN SELECT RAISE(FAIL, 'test registration failure'); END;").unwrap();
    assert!(create(&db, "Rollback", "New group", None, Some(directory.path()), "info").is_err());
    assert!(!directory.path().join("rollback.yml").exists());
    assert!(db.groups().unwrap().is_empty());
    assert!(db.tunnels().unwrap().is_empty());
    assert!(!std::fs::read_dir(directory.path()).unwrap().flatten().any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp")));
}

#[test]
fn simultaneous_creates_register_an_existing_config_only_once() {
    let directory = tempfile::tempdir().unwrap();
    let path = source(directory.path());
    let db = std::sync::Arc::new(Db::open_in_memory().unwrap());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2).map(|_| {
        let db = db.clone();
        let path = path.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            create(&db, "Concurrent", "Same group", Some(&path), None, "info")
        })
    }).collect();
    let successes = handles.into_iter().map(|handle| handle.join().unwrap()).filter(|result| result.is_ok()).count();
    assert_eq!(successes, 1);
    assert_eq!(db.tunnels().unwrap().len(), 1);
    assert_eq!(db.groups().unwrap().len(), 1);
}

#[test]
fn default_directory_is_computed_without_creating_it_and_single_origin_is_ready() {
    let expected = underlane_core::config::home_dir().join(".cloudflared/underlane");
    let existed = expected.exists();
    assert_eq!(default_directory(), expected);
    assert!(default_directory().is_absolute());
    assert_eq!(expected.exists(), existed);
    let config = underlane_core::config::TunnelConfig::parse(Path::new("config.yml"), "url: http://localhost:1").unwrap();
    assert!(!needs_setup(&config));
    let empty_id = underlane_core::config::TunnelConfig::parse(Path::new("config.yml"), "tunnel: ' '\ningress: [{service: http_status:404}]").unwrap();
    assert!(needs_setup(&empty_id));
}

#[test]
fn config_tokens_and_token_files_satisfy_identity_without_reaching_preview() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("token.yml");
    for identity in ["token: invented-project-secret", "token-file: ./private-token.txt"] {
        std::fs::write(&path, format!("{identity}\ningress: [{{service: http_status:404}}]\n")).unwrap();
        let config = underlane_core::config::TunnelConfig::load(&path).unwrap();
        assert!(!needs_setup(&config));
        let serialized = serde_json::to_string(&preview(&path).unwrap()).unwrap();
        assert!(!serialized.contains("invented-project-secret"));
        assert!(!serialized.contains("private-token.txt"));
    }
    let empty_token = underlane_core::config::TunnelConfig::parse(&path, "token: ' '\ntoken-file: ''\ningress: [{service: http_status:404}]").unwrap();
    assert!(needs_setup(&empty_token));
}
