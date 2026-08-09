//! Spawns the bridged CLI and turns its stdout into a live event stream.

use super::creds::{Auth, Creds};
use super::{adapter_for, detect, BridgeEvent, TurnRequest};
use serde::Serialize;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Notify;

/// Channel the frontend listens on for every [`BridgeEvent`] of a turn the
/// user asked for.
pub const EVENT: &str = "purrch://agent";

/// ...and the one hunts arrive on. Deliberately separate: a chore firing at
/// 09:00 must not append itself to the conversation you were having, and the
/// panel must not mistake a cat's own errand for something you said.
pub const HUNT_EVENT: &str = "purrch://hunt";

/// Which hunt a stream of events belongs to, when it isn't the user's turn.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunt {
    /// This execution. New every time the chore fires.
    pub id: String,
    /// The chore it came from, and what that chore is called.
    pub chore: String,
    pub name: String,
}

/// One turn's stream, boiled down to what the caller has to decide with.
///
/// The chat panel builds its own picture from the events as they arrive and
/// ignores all of this; a hunt has nobody watching, so this is the only record
/// of what happened — it's what the gift is made of.
#[derive(Debug, Default)]
pub struct Outcome {
    /// What to pass back as `resume` next time, if the agent got that far.
    pub session: Option<String>,
    pub ok: bool,
    /// The last thing the cat actually said.
    pub text: Option<String>,
    pub tools: u32,
    /// Set when the turn ended in a `Failed` event rather than a `Finished`.
    pub error: Option<String>,
    /// The turn was cut short — the user started talking, or the cat went
    /// home. Not a failure, and not something to bring anyone a gift about.
    pub cancelled: bool,
}

/// The envelope a hunt's events travel in.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HuntEvent<'a> {
    hunt: &'a Hunt,
    event: &'a BridgeEvent,
}

/// Turn state for the whole colony, keyed by window label.
///
/// Every cat is an independent agent: its own conversation, its own
/// subprocess, its own cancellation. Nothing here may be global, or one cat
/// stopping work would stop its siblings too.
#[derive(Default)]
pub struct BridgeState {
    turns: Mutex<HashMap<String, Arc<Notify>>>,
}

impl BridgeState {
    /// Registers a turn for `cat`, cancelling that cat's previous turn (and
    /// only that cat's) if one is still running.
    fn begin(&self, cat: &str) -> Arc<Notify> {
        let token = Arc::new(Notify::new());
        let mut turns = self.turns.lock().unwrap();
        if let Some(previous) = turns.insert(cat.to_string(), token.clone()) {
            previous.notify_waiters();
        }
        token
    }

    fn end(&self, cat: &str, token: &Arc<Notify>) {
        let mut turns = self.turns.lock().unwrap();
        // Only clear if we're still the current turn, so a newer turn that
        // superseded us isn't cancelled by our cleanup.
        if turns.get(cat).is_some_and(|t| Arc::ptr_eq(t, token)) {
            turns.remove(cat);
        }
    }

    pub fn cancel(&self, cat: &str) {
        if let Some(token) = self.turns.lock().unwrap().remove(cat) {
            token.notify_waiters();
        }
    }

    /// Whether this cat has a turn in flight. A chore about to fire asks first
    /// and waits its turn — the cat you're talking to is busy talking to you.
    pub fn busy(&self, cat: &str) -> bool {
        self.turns.lock().unwrap().contains_key(cat)
    }

    /// Stops a cat's work when its window closes, so a dismissed cat can't
    /// leave an agent running against the user's subscription.
    pub fn cancel_and_forget(&self, cat: &str) {
        self.cancel(cat);
    }
}

/// Events go to one cat's window, never broadcast — a sibling must not react
/// to work it isn't doing. A hunt's events go to the same window down a
/// different channel, so the panel can show them without owning them.
fn emit(app: &AppHandle, cat: &str, hunt: Option<&Hunt>, event: &BridgeEvent) {
    match hunt {
        None => {
            let _ = app.emit_to(cat, EVENT, event);
        }
        Some(hunt) => {
            let _ = app.emit_to(cat, HUNT_EVENT, HuntEvent { hunt, event });
        }
    }
}

/// Folds one event into the running picture of how the turn went.
fn observe(out: &mut Outcome, event: &BridgeEvent) {
    match event {
        BridgeEvent::Started { session, .. } => out.session = Some(session.clone()),
        // Kept as a fallback: a CLI that dies after saying something useful
        // still has something to show for itself.
        BridgeEvent::Text { text } => out.text = Some(text.clone()),
        BridgeEvent::ToolStart { .. } => out.tools += 1,
        BridgeEvent::Finished { ok, text, .. } => {
            out.ok = *ok;
            if text.is_some() {
                out.text = text.clone();
            }
        }
        BridgeEvent::Failed { message } => {
            out.ok = false;
            out.error = Some(message.clone());
        }
        _ => {}
    }
}

/// Runs one turn to completion, streaming events to `cat`'s window.
///
/// `hunt` is set when the turn is a chore firing rather than something the
/// user asked for; it only changes where the events are addressed. Everything
/// else — the permission bypass, the persona, whose money it spends — is
/// identical, because a cat doing a chore is the same cat.
pub async fn run(
    app: AppHandle,
    state: Arc<BridgeState>,
    creds: Arc<Creds>,
    cat: String,
    req: TurnRequest,
    hunt: Option<Hunt>,
) -> Result<Outcome, String> {
    let backend = detect::find(&req.backend)
        .ok_or_else(|| format!("{} isn't installed on this machine", req.backend))?;
    let mut adapter = adapter_for(&req.backend)
        .ok_or_else(|| format!("no adapter for backend '{}'", req.backend))?;

    // Settled before the command is built: the adapter needs the mode to pick
    // its flags, and a turn the user can't pay for should never be spawned.
    let plan = creds.plan(&req.backend)?;

    let mut cmd = adapter.command(&backend.program, &req, plan.auth);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("NO_COLOR", "1")
        .env("FORCE_COLOR", "0")
        // Prevent a nested agent from thinking it's inside another one.
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .kill_on_drop(true);

    // The only place a key is ever handled. It goes into the child's
    // environment and nowhere else — not argv, not a config file we write, not
    // an event. Purrch itself never sends it anywhere: the CLI does.
    if let Some(var) = backend.key_env.as_deref() {
        match (plan.auth, plan.key.as_deref()) {
            (Auth::Key, Some(key)) => {
                cmd.env(var, key);
            }
            // Asked for the subscription, so an ambient key inherited from the
            // user's shell must not quietly get spent instead.
            (Auth::Subscription, _) => {
                cmd.env_remove(var);
            }
            // Inherit: leave the environment exactly as we found it, which is
            // what everyone got before any of this existed.
            _ => {}
        }
    }

    detect::hide_console(cmd.as_std_mut());

    let mut child = cmd.spawn().map_err(|e| {
        format!("couldn't start {}: {e}", backend.label)
    })?;

    let stdout = child.stdout.take().ok_or("no stdout from agent")?;
    let stderr = child.stderr.take().ok_or("no stderr from agent")?;

    // Drain stderr concurrently — if it fills its pipe buffer the child blocks.
    let stderr_task = tokio::spawn(async move {
        let mut buf = String::new();
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            buf.push_str(&line);
            buf.push('\n');
        }
        buf
    });

    let token = state.begin(&cat);
    let mut lines = BufReader::new(stdout).lines();
    let mut outcome = Outcome::default();
    let hunt = hunt.as_ref();

    loop {
        tokio::select! {
            // Bias toward draining stdout so a cancel doesn't discard events
            // that already arrived.
            biased;

            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        for event in adapter.parse(&line) {
                            observe(&mut outcome, &event);
                            emit(&app, &cat, hunt, &event);
                        }
                    }
                    // EOF or a decode error: the agent is done talking.
                    _ => break,
                }
            }

            _ = token.notified() => {
                outcome.cancelled = true;
                let _ = child.kill().await;
                break;
            }
        }
    }

    let status = child.wait().await.ok();
    let stderr = stderr_task.await.unwrap_or_default();
    state.end(&cat, &token);

    if outcome.cancelled {
        outcome.ok = false;
        emit(
            &app,
            &cat,
            hunt,
            &BridgeEvent::Finished {
                ok: false,
                text: Some("(stopped)".into()),
                ms: None,
            },
        );
        return Ok(outcome);
    }

    if let Some(event) = adapter.on_exit(status.and_then(|s| s.code()), &stderr) {
        observe(&mut outcome, &event);
        emit(&app, &cat, hunt, &event);
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colony invariant: one cat's turns must never disturb another's.
    #[test]
    fn cats_cancel_independently() {
        let state = BridgeState::default();

        let a = state.begin("cat-a");
        let b = state.begin("cat-b");

        state.cancel("cat-a");
        // b's token is untouched and still registered.
        assert!(state.turns.lock().unwrap().contains_key("cat-b"));
        assert!(!state.turns.lock().unwrap().contains_key("cat-a"));

        // Cleaning up a's finished turn must not evict b.
        state.end("cat-b", &a);
        assert!(state.turns.lock().unwrap().contains_key("cat-b"));

        state.end("cat-b", &b);
        assert!(state.turns.lock().unwrap().is_empty());
    }

    /// A second turn on the same cat supersedes the first.
    #[test]
    fn same_cat_supersedes_its_own_turn() {
        let state = BridgeState::default();
        let first = state.begin("cat-a");
        let second = state.begin("cat-a");

        // The stale turn's cleanup must not remove the live one.
        state.end("cat-a", &first);
        assert!(state.turns.lock().unwrap().contains_key("cat-a"));

        state.end("cat-a", &second);
        assert!(state.turns.lock().unwrap().is_empty());
    }

    /// A chore asks this before firing, so it waits rather than cutting in.
    #[test]
    fn a_cat_mid_turn_reports_itself_busy() {
        let state = BridgeState::default();
        assert!(!state.busy("cat-a"));
        let token = state.begin("cat-a");
        assert!(state.busy("cat-a"));
        // ...and its sibling is still free to go hunting.
        assert!(!state.busy("cat-b"));
        state.end("cat-a", &token);
        assert!(!state.busy("cat-a"));
    }

    /// The gift is made entirely out of this, so every part of it has to land.
    #[test]
    fn a_turn_boils_down_to_what_the_cat_caught() {
        let mut out = Outcome::default();
        for event in [
            BridgeEvent::Started {
                session: "s-1".into(),
                model: None,
            },
            BridgeEvent::Thinking,
            BridgeEvent::ToolStart {
                tool: "Bash".into(),
                detail: "gh pr list".into(),
            },
            BridgeEvent::ToolEnd {
                tool: "t1".into(),
                ok: true,
            },
            BridgeEvent::Text {
                text: "having a look".into(),
            },
            BridgeEvent::Finished {
                ok: true,
                text: Some("three PRs, one needs you".into()),
                ms: Some(9000),
            },
        ] {
            observe(&mut out, &event);
        }

        assert_eq!(out.session.as_deref(), Some("s-1"));
        assert!(out.ok);
        assert_eq!(out.tools, 1);
        // The final result wins over the prose that streamed on the way there.
        assert_eq!(out.text.as_deref(), Some("three PRs, one needs you"));
        assert!(out.error.is_none());
    }

    #[test]
    fn a_turn_that_died_says_so() {
        let mut out = Outcome::default();
        observe(
            &mut out,
            &BridgeEvent::Text {
                text: "on it".into(),
            },
        );
        observe(
            &mut out,
            &BridgeEvent::Failed {
                message: "no such session".into(),
            },
        );
        assert!(!out.ok);
        assert_eq!(out.error.as_deref(), Some("no such session"));
        // Whatever it managed to say before dying is still worth having.
        assert_eq!(out.text.as_deref(), Some("on it"));
    }
}
