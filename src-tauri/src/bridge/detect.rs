//! Finds agent CLIs the user has already installed and signed in to.
//!
//! This is the whole point of the subscription bridge: by default we hold none
//! of the user's AI credentials. Their own first-party CLI already did the
//! OAuth dance against their subscription, so we just drive it. A user who
//! would rather spend an API key can say so — see [`creds`](super::creds) —
//! and [`Candidate::key_env`] is where each CLI reads that key from.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Windows: don't flash a console window every time we shell out.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Apply platform-specific spawn flags shared by every command we launch.
pub fn hide_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd;
}

#[derive(Debug, Clone, Serialize)]
pub struct Backend {
    /// Stable id used by the frontend and by `Backend::for_id`.
    pub id: String,
    pub label: String,
    /// What subscription this rides on, for the UI to explain itself.
    pub subscription: String,
    pub program: String,
    pub version: Option<String>,
    /// Best-effort: a credential file the CLI writes after login exists.
    pub signed_in: bool,
    /// Environment variable this CLI takes a bring-your-own API key in, if it
    /// takes one at all. `None` means the backend can only ride a login.
    pub key_env: Option<String>,
    /// How the user has chosen to pay for this backend. Detection can't know
    /// it — the `bridge_detect` command fills it in from the credential store,
    /// and it stays `Auth::Inherit` until then.
    pub auth: super::creds::Auth,
    /// Whether a key is actually saved for it.
    pub has_key: bool,
}

struct Candidate {
    id: &'static str,
    label: &'static str,
    subscription: &'static str,
    /// Executable names to look for, in preference order.
    bins: &'static [&'static str],
    /// Extra install locations to probe when the binary isn't on PATH.
    extra_dirs: fn(&Path) -> Vec<PathBuf>,
    /// Credential files (relative to home) that indicate a completed login.
    creds: &'static [&'static str],
    /// Args that print a version quickly.
    version_args: &'static [&'static str],
    /// Where this CLI reads an API key from, if it reads one.
    key_env: Option<&'static str>,
}

const CANDIDATES: &[Candidate] = &[
    Candidate {
        id: "claude",
        label: "Claude Code",
        subscription: "Claude Pro / Max",
        bins: &["claude"],
        extra_dirs: |home| vec![home.join(".local/bin"), home.join(".claude/local")],
        creds: &[".claude/.credentials.json", ".claude/auth.json"],
        version_args: &["--version"],
        key_env: Some("ANTHROPIC_API_KEY"),
    },
    Candidate {
        id: "codex",
        label: "Codex CLI",
        subscription: "ChatGPT Plus / Pro",
        bins: &["codex"],
        extra_dirs: |home| {
            vec![
                home.join(".codex/bin"),
                home.join("AppData/Local/OpenAI/Codex"),
                home.join("AppData/Local/OpenAI/Codex/bin"),
                home.join(".local/bin"),
            ]
        },
        creds: &[".codex/auth.json"],
        version_args: &["--version"],
        key_env: Some("OPENAI_API_KEY"),
    },
    Candidate {
        id: "gemini",
        label: "Gemini CLI",
        subscription: "Google AI Pro / Ultra",
        bins: &["gemini"],
        extra_dirs: |home| vec![home.join(".local/bin")],
        creds: &[".gemini/oauth_creds.json"],
        version_args: &["--version"],
        key_env: Some("GEMINI_API_KEY"),
    },
    Candidate {
        id: "opencode",
        label: "opencode",
        subscription: "whichever provider you logged in with",
        bins: &["opencode"],
        extra_dirs: |home| vec![home.join(".opencode/bin"), home.join(".local/bin")],
        creds: &[
            ".local/share/opencode/auth.json",
            "AppData/Local/opencode/auth.json",
        ],
        version_args: &["--version"],
        // opencode fronts many providers at once, so there is no single key
        // variable to set. Its own `opencode auth login` owns that.
        key_env: None,
    },
];

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Executable suffixes to try. On Windows an npm-installed CLI is usually a
/// `.cmd` shim, which `Command` will not find without the extension.
#[cfg(windows)]
const EXE_SUFFIXES: &[&str] = &[".exe", ".cmd", ".bat", ""];
#[cfg(not(windows))]
const EXE_SUFFIXES: &[&str] = &[""];

fn probe_dir(dir: &Path, bin: &str) -> Option<PathBuf> {
    for suffix in EXE_SUFFIXES {
        let path = dir.join(format!("{bin}{suffix}"));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Look for `bin` on PATH, then in the candidate's known install dirs.
fn find_program(cand: &Candidate, home: Option<&Path>) -> Option<PathBuf> {
    for bin in cand.bins {
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                if let Some(hit) = probe_dir(&dir, bin) {
                    return Some(hit);
                }
            }
        }
        if let Some(home) = home {
            for dir in (cand.extra_dirs)(home) {
                if let Some(hit) = probe_dir(&dir, bin) {
                    return Some(hit);
                }
            }
        }
    }
    None
}

fn read_version(program: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    hide_console(&mut cmd);
    let out = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// Every CLI on this machine we know how to look for, driveable or not.
fn installed() -> Vec<Backend> {
    let home = home();
    CANDIDATES
        .iter()
        .filter_map(|cand| {
            let program = find_program(cand, home.as_deref())?;
            let signed_in = home.as_ref().is_some_and(|h| {
                cand.creds.iter().any(|rel| h.join(rel).exists())
            });
            Some(Backend {
                id: cand.id.to_string(),
                label: cand.label.to_string(),
                subscription: cand.subscription.to_string(),
                version: read_version(&program, cand.version_args),
                program: program.to_string_lossy().into_owned(),
                signed_in,
                key_env: cand.key_env.map(str::to_string),
                auth: super::creds::Auth::default(),
                has_key: false,
            })
        })
        .collect()
}

/// Everything the user could actually put behind a cat.
///
/// Narrower than [`installed`] on purpose: we know how to *find* more CLIs than
/// we know how to *drive*, and a backend in the picker that dies at spawn time
/// with "no adapter for backend" is worse than one that was never offered.
pub fn detect_all() -> Vec<Backend> {
    installed()
        .into_iter()
        .filter(|b| super::has_adapter(&b.id))
        .collect()
}

/// Resolve a single backend by id, re-running detection so a CLI the user
/// installed after launch is picked up without restarting the cat.
///
/// Searches everything installed rather than only what's driveable, so a cat
/// still holding a stale backend id gets the accurate "no adapter" error
/// instead of being told the CLI isn't installed.
pub fn find(id: &str) -> Option<Backend> {
    installed().into_iter().find(|b| b.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Machine-dependent by nature, so this asserts only invariants: probing
    /// must never panic, and anything reported must be a real file.
    #[test]
    fn detection_reports_only_real_executables() {
        for backend in installed() {
            assert!(
                Path::new(&backend.program).is_file(),
                "{} pointed at a non-file: {}",
                backend.id,
                backend.program
            );
            assert!(!backend.label.is_empty());
            eprintln!(
                "found {} at {} (version {:?}, signed_in {})",
                backend.id, backend.program, backend.version, backend.signed_in
            );
        }
    }

    #[test]
    fn unknown_ids_resolve_to_nothing() {
        assert!(find("definitely-not-an-agent").is_none());
    }

    /// The picker may only offer backends a turn can actually run on.
    #[test]
    fn only_driveable_backends_are_offered() {
        for backend in detect_all() {
            assert!(
                super::super::has_adapter(&backend.id),
                "{} was offered without an adapter",
                backend.id
            );
        }
    }

    /// Every backend the user can pick either takes a key or is honest that it
    /// doesn't, so the panel never offers a key box that goes nowhere.
    #[test]
    fn key_variables_are_named_for_every_candidate() {
        for cand in CANDIDATES {
            if let Some(var) = cand.key_env {
                assert!(
                    var.ends_with("_API_KEY"),
                    "{} has a suspicious key variable: {var}",
                    cand.id
                );
            }
        }
    }
}
