//! Shipping fixes to a machine you can't reach.
//!
//! This is not a nicety. The Claude Code adapter is pinned to a JSONL schema
//! captured from one version of one CLI — see the header of
//! [`claude`](crate::bridge::claude) — and the day those key names change,
//! every installed cat goes quiet. The same is true of the Codex adapter, only
//! sooner, because it was written from documentation rather than from a
//! captured run. An app whose correctness depends on somebody else's release
//! cadence has no business shipping without a way to update itself.
//!
//! **How the trust works.** Updates are signed with a minisign key whose public
//! half is in `tauri.conf.json` and whose private half never leaves the release
//! machine. The updater refuses anything that doesn't verify, so the endpoint
//! being a plain HTTPS file on GitHub is not the thing standing between a user
//! and a hostile download — the signature is. `RELEASING.md` has the rest.
//!
//! **What it deliberately doesn't do** is install anything on its own. Purrch
//! runs an agent with every permission check off; an app in that position
//! replacing its own binary unattended is a bigger ask than the one the user
//! already agreed to. So: it looks when asked, says what it found, and waits.

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

/// Channel the panel hears about updates on. Broadcast rather than addressed:
/// an update is news for the whole colony, not for one cat.
pub const EVENT: &str = "purrch://update";

/// What was found, if anything.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Available {
    /// `None` means "looked, nothing there" — which is a different answer from
    /// an error, and the panel says so differently.
    pub version: Option<String>,
    pub notes: Option<String>,
    /// Set when the check itself failed: offline, endpoint down, bad signature.
    pub error: Option<String>,
}

/// Asks the endpoint what the newest release is.
///
/// Returns the answer rather than acting on it. A check that fails is not
/// worth interrupting anyone over — an offline laptop is the common case, and
/// "couldn't check for updates" is noise unless you asked.
pub async fn look(app: &AppHandle) -> Available {
    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(e) => {
            log::error!("no updater: {e}");
            return Available {
                error: Some(e.to_string()),
                ..Default::default()
            };
        }
    };

    match updater.check().await {
        Ok(Some(update)) => {
            log::info!("update available: {}", update.version);
            Available {
                version: Some(update.version.clone()),
                notes: update.body.clone(),
                error: None,
            }
        }
        Ok(None) => {
            log::info!("no update available");
            Available::default()
        }
        Err(e) => {
            log::warn!("update check failed: {e}");
            Available {
                error: Some(e.to_string()),
                ..Default::default()
            }
        }
    }
}

/// Downloads and installs the newest release, then restarts.
///
/// Only ever reached from a button the user pressed. The download is verified
/// against the public key in `tauri.conf.json` before a single byte is run —
/// that check is inside the plugin, and it is the whole security model here.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Err("there's nothing newer to install".into());
    };

    log::info!("installing {}", update.version);
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| {
            log::error!("update failed: {e}");
            e.to_string()
        })?;

    // The installer has replaced the binary; what's running is the old one.
    log::info!("restarting into {}", update.version);
    app.restart();
}

/// The tray's "check for updates", which has no window to return a value to.
///
/// Broadcasts the result instead, so any open panel can say what happened. A
/// colony with every panel shut gets a line in the log and nothing else, which
/// is the right amount of noise for a check nobody is waiting on.
pub fn check_from_tray(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let found = look(&app).await;
        let _ = app.emit(EVENT, &found);
    });
}
