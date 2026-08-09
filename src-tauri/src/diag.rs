//! Where Purrch writes down what happened to it.
//!
//! Everything in this app is a windowless window: `skipTaskbar`, no
//! decorations, no console. `eprintln!` in a release build goes to a stream
//! nobody is attached to, so before this existed the honest answer to "my cat
//! stopped working" was that there was nothing to look at — not a log, not a
//! stack trace, not even the version it happened on.
//!
//! Two things, therefore:
//!
//! * a rolling log file next to the rest of the app's data, and
//! * a panic hook that gets the panic *into* that file before the process goes.
//!   The release profile is `panic = "abort"`, so the hook is the only chance —
//!   there is no unwind, no `catch_unwind`, and no second attempt.
//!
//! What is deliberately **not** here is anything that leaves the machine. There
//! is no crash reporter and no telemetry: this app drives an agent across the
//! user's whole filesystem, and a log of that is the last thing that should be
//! uploaded anywhere by default. The user reads their own log, or attaches it
//! to an issue themselves. `PRIVACY.md` says so out loud.

use tauri_plugin_log::{Target, TargetKind};

/// Logs kept before the oldest is dropped. Small on purpose — these are for
/// "what happened just now", not an archive.
const MAX_LOG_BYTES: u128 = 2 * 1024 * 1024;

/// The logging plugin, pointed at the app's log directory and at stderr for
/// `npm run tauri dev`.
pub fn logger() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_log::Builder::new()
        .targets([
            Target::new(TargetKind::LogDir {
                file_name: Some("purrch".into()),
            }),
            Target::new(TargetKind::Stderr),
        ])
        .max_file_size(MAX_LOG_BYTES)
        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
        // `info` is the level at which the interesting things here happen — a
        // cat coming back, a hunt refused for budget, an adapter failing. Debug
        // would be dominated by the webview's own chatter.
        .level(log::LevelFilter::Info)
        .build()
}

/// Sends panics to the log before the process dies.
///
/// Installed as early as possible in `run()`: a panic during setup is exactly
/// the one worth having, and it is also the one most likely to happen before
/// any of the rest of the app exists.
pub fn catch_panics() {
    // Keep whatever was there — in a debug build that's the default hook, and
    // its message on the console is still the fastest way to read a panic.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        // `location` is the file and line; the payload is the message, which is
        // a `&str` or a `String` depending on how it was raised.
        let what = panic
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| panic.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panicked with no message".to_string());
        match panic.location() {
            Some(at) => log::error!("panic at {}:{}: {what}", at.file(), at.line()),
            None => log::error!("panic somewhere: {what}"),
        }
        previous(panic);
    }));
}

/// One line at startup, so every log opens by saying what it is a log of.
///
/// Sounds trivial and isn't: a bug report is worth about half as much without
/// the version, and asking a user which build they are on is a round trip that
/// this avoids entirely.
pub fn hello() {
    log::info!(
        "purrch {} starting on {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );
}
