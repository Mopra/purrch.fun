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
#[serde(rename_all = "camelCase")]
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
    /// ...and whether it's in a file rather than the OS credential store.
    pub key_in_file: bool,
    /// This adapter has never been run against the real CLI.
    ///
    /// Said out loud in the picker rather than hidden: an adapter written from
    /// documentation is a guess, and a user whose turns come back strangely
    /// deserves to know which half of the bridge to suspect.
    pub experimental: bool,
}

/// Everything detection found, split by whether a turn can actually run on it.
///
/// Two lists rather than one flag, so the picker structurally cannot offer a
/// backend that would die at spawn time with "no adapter" — while `others` is
/// still there to explain why an installed CLI isn't on the menu, instead of
/// the app pretending it never saw it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub backends: Vec<Backend>,
    pub others: Vec<Other>,
}

/// A CLI on this machine that Purrch knows about but can't drive yet.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Other {
    pub id: String,
    pub label: String,
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
    /// Whether this adapter has ever been run against the real CLI.
    experimental: bool,
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
        // Verified against a live `--output-format stream-json` run.
        experimental: false,
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
        // The adapter is written from documentation, not from a captured run —
        // see the header of `codex.rs`. Until someone drives it against the
        // real CLI, the picker says so.
        experimental: true,
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
        experimental: true,
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
        experimental: true,
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
///
/// Probes the filesystem and shells out for a version string, so it is not
/// free — [`detect_all`] runs it once and splits the result rather than asking
/// twice.
fn installed() -> Vec<Backend> {
    let home = home();
    CANDIDATES
        .iter()
        .filter_map(|cand| {
            let program = find_program(cand, home.as_deref())?;
            let signed_in = home
                .as_ref()
                .is_some_and(|h| cand.creds.iter().any(|rel| h.join(rel).exists()));
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
                key_in_file: false,
                experimental: cand.experimental,
            })
        })
        .collect()
}

/// Everything the user could actually put behind a cat, plus what was found
/// and can't be.
///
/// The split is the point: we know how to *find* more CLIs than we know how to
/// *drive*, and a backend in the picker that dies at spawn time with "no
/// adapter for backend" is worse than one that was never offered. But silently
/// ignoring an installed CLI is its own kind of broken — someone who installs
/// Gemini CLI and is then told "no agent CLI found" has every reason to think
/// the app can't see it. So the undriveable ones come back too, separately, for
/// the panel to explain.
pub fn detect_all() -> Detected {
    let (backends, rest): (Vec<Backend>, Vec<Backend>) = installed()
        .into_iter()
        .partition(|b| super::has_adapter(&b.id));
    Detected {
        backends,
        others: rest
            .into_iter()
            .map(|b| Other {
                id: b.id,
                label: b.label,
            })
            .collect(),
    }
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

    /// The picker may only offer backends a turn can actually run on — and
    /// everything else that was found has to land in the other list rather
    /// than being dropped on the floor.
    #[test]
    fn driveable_backends_are_offered_and_the_rest_are_still_reported() {
        let found = detect_all();
        for backend in &found.backends {
            assert!(
                super::super::has_adapter(&backend.id),
                "{} was offered without an adapter",
                backend.id
            );
        }
        for other in &found.others {
            assert!(
                !super::super::has_adapter(&other.id),
                "{} is driveable but was filed as unsupported",
                other.id
            );
        }
        // Nothing detection found may go missing between the two lists.
        assert_eq!(
            found.backends.len() + found.others.len(),
            installed().len(),
            "a detected CLI ended up in neither list"
        );
    }

    /// Claude Code is the one adapter verified against a captured run; anything
    /// else has to admit it. Getting this backwards would have the panel vouch
    /// for a parser nobody has ever seen work.
    #[test]
    fn only_the_verified_adapter_is_unmarked() {
        for cand in CANDIDATES {
            assert_eq!(
                cand.experimental,
                cand.id != "claude",
                "{} disagrees with what has actually been verified",
                cand.id
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
