//! Bring-your-own-key credentials.
//!
//! The default is still the subscription bridge described in [`super`]: drive a
//! CLI the user already logged in to, and hold nothing. This module is the one
//! deliberate exception. A user who would rather spend a metered API key than
//! their subscription allowance can hand one over, and it goes to the OS
//! credential store — never into `cats.json`, never into a log, never into argv.
//!
//! Each backend is in one of three states, and the difference is *whose money
//! the turn spends*, so none of them may be guessed at:
//!
//! * [`Auth::Inherit`] — the default. Whatever is already in the environment
//!   wins. Someone who has always had `ANTHROPIC_API_KEY` exported keeps the
//!   behaviour they had before this module existed.
//! * [`Auth::Subscription`] — spend the CLI's own login, and scrub any ambient
//!   key out of the child's environment so billing cannot quietly drift onto it.
//! * [`Auth::Key`] — spend the key stored here, and make sure the CLI actually
//!   reads it rather than falling back to a login it still has on disk.
//!
//! The secret is deliberately kept away from the adapters: they are handed the
//! [`Auth`] mode only, because that is all they need to pick the right flags.
//! [`session`](super::session) is the only place the key itself is touched.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Which purse a turn is paid from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Auth {
    /// Take the environment as we found it.
    #[default]
    Inherit,
    /// The CLI's own login.
    Subscription,
    /// A key the user gave us.
    Key,
}

/// Keychain service; one account under it per backend id.
const SERVICE: &str = "fun.purrch.keys";

/// How one turn should be paid for.
///
/// No `Debug` or `Serialize`, on purpose — this is the one type that carries a
/// plaintext secret, and it should be impossible to print it by accident.
pub struct Plan {
    pub auth: Auth,
    /// The key to export. Only ever `Some` when `auth` is [`Auth::Key`].
    pub key: Option<String>,
}

/// What the picker needs to show. Carries no secret.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub auth: Auth,
    /// A key for this backend is actually in the store — the mode alone can
    /// lie, if the keychain was cleared out from under us.
    pub has_key: bool,
}

/// Per-backend choices, plus the keys behind them.
pub struct Creds {
    /// Where the *choice* lives. Not secret, and readable by a human who wants
    /// to know what their cat is spending.
    path: PathBuf,
    /// Fallback secret store, used only where there is no working keychain.
    fallback: PathBuf,
    modes: Mutex<BTreeMap<String, Auth>>,
}

impl Creds {
    pub fn load(dir: &Path) -> Self {
        let _ = fs::create_dir_all(dir);
        let path = dir.join("auth.json");
        let modes = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<BTreeMap<String, Auth>>(&raw).ok())
            .unwrap_or_default();
        Creds {
            path,
            fallback: dir.join("keys.json"),
            modes: Mutex::new(modes),
        }
    }

    fn mode(&self, backend: &str) -> Auth {
        self.modes
            .lock()
            .unwrap()
            .get(backend)
            .copied()
            .unwrap_or_default()
    }

    fn set_mode(&self, backend: &str, auth: Auth) {
        let mut modes = self.modes.lock().unwrap();
        match auth {
            Auth::Inherit => {
                modes.remove(backend);
            }
            other => {
                modes.insert(backend.to_string(), other);
            }
        }
        if let Ok(json) = serde_json::to_string_pretty(&*modes) {
            let tmp = self.path.with_extension("json.tmp");
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, &self.path);
            }
        }
    }

    /// What to do with the child's environment for one turn.
    ///
    /// Errs rather than guessing when the user asked for a key we no longer
    /// have: falling through to the subscription would spend the wrong purse
    /// silently, which is the one outcome nobody could have consented to.
    pub fn plan(&self, backend: &str) -> Result<Plan, String> {
        let auth = self.mode(backend);
        if auth != Auth::Key {
            return Ok(Plan { auth, key: None });
        }
        match self.key(backend) {
            Some(key) => Ok(Plan {
                auth,
                key: Some(key),
            }),
            None => Err(format!(
                "{backend} is set to use your API key, but there's no key saved. \
                 Open the panel and add one, or switch back to your subscription."
            )),
        }
    }

    pub fn status(&self, backend: &str) -> Status {
        Status {
            auth: self.mode(backend),
            has_key: self.key(backend).is_some(),
        }
    }

    /// Saves a key and switches the backend onto it.
    pub fn set_key(&self, backend: &str, key: &str) -> Result<(), String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("that's an empty key".into());
        }
        write_secret(&self.fallback, backend, key)?;
        self.set_mode(backend, Auth::Key);
        Ok(())
    }

    /// Goes back to the CLI's own login, dropping the key.
    pub fn use_subscription(&self, backend: &str) -> Result<(), String> {
        delete_secret(&self.fallback, backend);
        self.set_mode(backend, Auth::Subscription);
        Ok(())
    }

    /// Forgets the choice entirely, back to inheriting the environment.
    pub fn clear(&self, backend: &str) -> Result<(), String> {
        delete_secret(&self.fallback, backend);
        self.set_mode(backend, Auth::Inherit);
        Ok(())
    }

    fn key(&self, backend: &str) -> Option<String> {
        read_secret(&self.fallback, backend)
    }
}

// ---------------------------------------------------------------------------
// Secret storage. Keychain first; a locked-down file only where there is no
// keychain to talk to — a headless Linux box has no secret service running, and
// refusing to work there would be worse than a 0600 file in the config dir.
// ---------------------------------------------------------------------------

fn entry(backend: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, backend).ok()
}

fn write_secret(fallback: &Path, backend: &str, key: &str) -> Result<(), String> {
    if let Some(entry) = entry(backend) {
        if entry.set_password(key).is_ok() {
            // Belt and braces: a key that used to live in the fallback must not
            // linger there now that the keychain has it.
            let _ = remove_from_file(fallback, backend);
            return Ok(());
        }
    }
    write_to_file(fallback, backend, key)
}

fn read_secret(fallback: &Path, backend: &str) -> Option<String> {
    if let Some(entry) = entry(backend) {
        if let Ok(key) = entry.get_password() {
            return Some(key);
        }
    }
    read_from_file(fallback, backend)
}

fn delete_secret(fallback: &Path, backend: &str) {
    if let Some(entry) = entry(backend) {
        let _ = entry.delete_credential();
    }
    let _ = remove_from_file(fallback, backend);
}

fn read_file(path: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Writes the fallback store with owner-only permissions.
///
/// The mode is set as part of opening the file rather than afterwards: between
/// a default-permission create and a chmod there is a window in which another
/// local user can open the file, and it holds an API key.
fn write_file(path: &Path, keys: &BTreeMap<String, String>) -> Result<(), String> {
    use std::io::Write;

    let json = serde_json::to_string_pretty(keys).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");

    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options.open(&tmp).map_err(|e| e.to_string())?;
    file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
    drop(file);
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn write_to_file(path: &Path, backend: &str, key: &str) -> Result<(), String> {
    let mut keys = read_file(path);
    keys.insert(backend.to_string(), key.to_string());
    write_file(path, &keys)
}

fn read_from_file(path: &Path, backend: &str) -> Option<String> {
    read_file(path).remove(backend)
}

fn remove_from_file(path: &Path, backend: &str) -> Result<(), String> {
    let mut keys = read_file(path);
    if keys.remove(backend).is_none() {
        return Ok(());
    }
    if keys.is_empty() {
        let _ = fs::remove_file(path);
        return Ok(());
    }
    write_file(path, &keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests must not touch the developer's real keychain, so they drive the
    /// file store directly for anything involving a secret.
    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("purrch-creds-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_unconfigured_backend_inherits_the_environment() {
        let creds = Creds::load(&tempdir("default"));
        let plan = creds.plan("claude").unwrap();
        assert_eq!(plan.auth, Auth::Inherit);
        assert!(plan.key.is_none());
        assert!(!creds.status("claude").has_key);
    }

    #[test]
    fn choosing_the_subscription_carries_no_key() {
        let creds = Creds::load(&tempdir("sub"));
        creds.use_subscription("claude").unwrap();
        let plan = creds.plan("claude").unwrap();
        assert_eq!(plan.auth, Auth::Subscription);
        assert!(plan.key.is_none());
    }

    #[test]
    fn the_choice_survives_a_restart_but_never_holds_the_key() {
        let dir = tempdir("persist");
        Creds::load(&dir).use_subscription("codex").unwrap();

        assert_eq!(Creds::load(&dir).plan("codex").unwrap().auth, Auth::Subscription);
        // The plaintext side of the store must be exactly the choice, so that
        // a user reading it finds no secret and no surprise.
        let raw = fs::read_to_string(dir.join("auth.json")).unwrap();
        assert!(raw.contains("subscription"));
        assert!(!raw.contains("sk-"));
    }

    /// The failure that would otherwise be silent: a mode saying "key" with no
    /// key behind it must stop the turn, not fall through to the subscription.
    #[test]
    fn a_missing_key_refuses_to_spend_the_subscription_instead() {
        let dir = tempdir("missing");
        let creds = Creds::load(&dir);
        // Reach past `set_key` to build exactly the inconsistent state a wiped
        // keychain would leave behind.
        creds.set_mode("claude", Auth::Key);
        // `Plan` has no `Debug` on purpose, so no `unwrap_err` here.
        let Err(err) = creds.plan("claude") else {
            panic!("a missing key was allowed to fall through to the subscription");
        };
        assert!(err.contains("no key saved"), "unhelpful error: {err}");
    }

    #[test]
    fn the_fallback_store_round_trips_and_forgets() {
        let dir = tempdir("file");
        let path = dir.join("keys.json");

        write_to_file(&path, "claude", "sk-ant-test").unwrap();
        write_to_file(&path, "codex", "sk-oai-test").unwrap();
        assert_eq!(read_from_file(&path, "claude").as_deref(), Some("sk-ant-test"));

        remove_from_file(&path, "claude").unwrap();
        assert!(read_from_file(&path, "claude").is_none());
        assert_eq!(read_from_file(&path, "codex").as_deref(), Some("sk-oai-test"));

        // Emptying it takes the file with it rather than leaving `{}` behind.
        remove_from_file(&path, "codex").unwrap();
        assert!(!path.exists());
    }

    /// Touches the developer's real credential store, so it's opt-in:
    /// `cargo test --lib -- --ignored keychain`.
    ///
    /// Worth running by hand on any platform this ships to, because the
    /// failure it catches is invisible: if the keychain can't be reached,
    /// every key silently lands in the fallback file instead, and the app
    /// still works while quietly keeping secrets somewhere it promised not to.
    #[test]
    #[ignore = "writes to the real OS keychain"]
    fn the_real_keychain_round_trips() {
        let backend = "purrch-selftest";
        let entry = entry(backend).expect("no keychain entry could be built");

        entry.set_password("sk-not-a-real-key").expect("keychain write failed");
        assert_eq!(
            entry.get_password().ok().as_deref(),
            Some("sk-not-a-real-key"),
            "the key did not come back out of the keychain"
        );

        entry.delete_credential().expect("keychain delete failed");
        assert!(
            entry.get_password().is_err(),
            "the key outlived its deletion"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_fallback_store_is_not_world_readable() {
        use std::os::unix::fs::PermissionsExt;
        let path = tempdir("perms").join("keys.json");
        write_to_file(&path, "claude", "sk-ant-test").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "keys.json is readable by others: {mode:o}");
    }
}
