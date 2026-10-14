//! Tunnel tokens live in the system keychain, never in SQLite or on a command line.

use underlane_core::secret::Secret;

const SERVICE: &str = "com.davidsonsns.underlane.tunnel-token";

fn entry(id: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, id).ok()
}

pub fn set(id: &str, token: &str) -> anyhow::Result<()> {
    entry(id).ok_or_else(|| anyhow::anyhow!("keychain unavailable"))?.set_password(token.trim())?;
    Ok(())
}

pub fn get(id: &str) -> Option<Secret<String>> {
    entry(id)?.get_password().ok().map(Secret::new)
}

pub fn delete(id: &str) {
    if let Some(e) = entry(id) {
        let _ = e.delete_credential();
    }
}
