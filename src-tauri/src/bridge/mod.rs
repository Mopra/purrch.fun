//! The subscription bridge.
//!
//! Purrch does not talk to any model provider directly. It drives an agent CLI
//! the user has already installed and logged in to, so the work is billed
//! against *their* subscription by *their* first-party tool.
//!
//! The one exception is [`creds`]: a user who would rather spend a metered API
//! key can save one, and it is exported into the CLI's environment for the
//! turn. That does not change the shape of anything here — it is still their
//! CLI doing the work, only paid for differently.
//!
//! Each backend gets an [`Adapter`] that knows two things: how to build a
//! headless streaming command, and how to turn that command's JSONL output
//! into provider-neutral [`BridgeEvent`]s the cat can react to.

pub mod claude;
pub mod codex;
pub mod creds;
pub mod detect;
pub mod persona;
pub mod session;

use serde::Deserialize;
use serde::Serialize;
use tokio::process::Command;

/// Purrch runs every backend with all permission checks disabled, by design:
/// the cat is meant to do anything the user could do, without stopping to ask.
/// The user agrees to this once, on first run, before the composer unlocks.
///
/// The consequence to keep in mind when changing this file: any content the
/// agent *reads* — a web page, an email, a file — can attempt to steer what it
/// *does*. The visible tool feed in the UI is the only thing standing between
/// the user and a silent action, so never drop a tool event on the floor.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnRequest {
    /// Backend id from [`detect::detect_all`], e.g. `"claude"` or `"codex"`.
    pub backend: String,
    pub prompt: String,
    /// Present on every turn after the first, to continue the conversation.
    #[serde(default)]
    pub resume: Option<String>,
    /// Working directory for the agent. Defaults to the user's home.
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// What this cat is called, for the persona it runs with. Empty falls back
    /// to the app's own name — see [`persona`](persona::persona), which is also
    /// where it gets trimmed to one short line before going near a prompt.
    #[serde(default)]
    pub name: String,
}

/// What the cat reacts to. One shape regardless of which CLI produced it.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BridgeEvent {
    /// Handshake. `session` is what to pass back as `resume` next turn.
    Started {
        session: String,
        model: Option<String>,
    },
    /// The model is reasoning but hasn't said anything yet.
    Thinking,
    /// A complete chunk of assistant prose.
    Text { text: String },
    /// The agent picked up a tool. `detail` is a human-readable one-liner.
    ToolStart { tool: String, detail: String },
    ToolEnd { tool: String, ok: bool },
    /// Terminal event for the turn. Always exactly one of these or `Failed`.
    Finished {
        ok: bool,
        text: Option<String>,
        ms: Option<u64>,
    },
    Failed { message: String },
}

/// Per-backend translation layer.
pub trait Adapter: Send {
    /// Build the headless, streaming invocation.
    ///
    /// `auth` says which purse the turn spends, because some CLIs need a flag
    /// to honour it rather than just an environment variable. The key itself
    /// is deliberately *not* passed: it would end up in argv, which on most
    /// systems any local process can read. [`session`] exports it instead.
    fn command(&self, program: &str, req: &TurnRequest, auth: creds::Auth) -> Command;
    /// Translate one line of stdout. Returning an empty vec means "ignore".
    fn parse(&mut self, line: &str) -> Vec<BridgeEvent>;
    /// Called when the process exits without having emitted a terminal event,
    /// so a crashed CLI still closes the turn instead of hanging the UI.
    fn on_exit(&mut self, code: Option<i32>, stderr: &str) -> Option<BridgeEvent> {
        let detail = stderr.trim();
        let detail = if detail.is_empty() {
            format!("agent exited with status {}", code.unwrap_or(-1))
        } else {
            // stderr can be enormous; the bubble only needs the tail.
            let tail: String = detail.chars().rev().take(400).collect();
            tail.chars().rev().collect()
        };
        Some(BridgeEvent::Failed { message: detail })
    }
}

pub fn adapter_for(id: &str) -> Option<Box<dyn Adapter>> {
    match id {
        "claude" => Some(Box::new(claude::ClaudeAdapter::default())),
        "codex" => Some(Box::new(codex::CodexAdapter::default())),
        _ => None,
    }
}

/// Whether a detected CLI can actually be driven yet.
///
/// [`detect`] knows about more CLIs than there are adapters for, so that
/// installing one is all it takes once its adapter lands. Until then the picker
/// must not offer it: choosing it would fail the turn at spawn time with an
/// error about a missing adapter, which reads as a broken app.
pub fn has_adapter(id: &str) -> bool {
    adapter_for(id).is_some()
}

/// Shorten a tool's arguments into something that fits in a speech bubble.
pub(crate) fn summarize(tool: &str, input: &serde_json::Value) -> String {
    let pick = |keys: &[&str]| -> Option<String> {
        keys.iter()
            .find_map(|k| input.get(*k).and_then(|v| v.as_str()))
            .map(str::to_string)
    };

    let raw = match tool {
        "Bash" | "PowerShell" | "shell" => pick(&["command"]),
        "Read" | "Write" | "Edit" | "NotebookEdit" => pick(&["file_path", "path"]),
        "Glob" | "Grep" => pick(&["pattern"]),
        "WebFetch" | "WebSearch" => pick(&["url", "query"]),
        _ => None,
    }
    .or_else(|| pick(&["command", "path", "file_path", "query", "prompt"]))
    .unwrap_or_else(|| input.to_string());

    let flat = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 90 {
        let head: String = flat.chars().take(87).collect();
        format!("{head}...")
    } else {
        flat
    }
}
