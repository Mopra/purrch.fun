//! Hunts — one execution of a chore, and the clock that sets them off.
//!
//! [`chores`](crate::chores) knows *when*; this file is *who, and one at a
//! time*. Two things make that more than a timer:
//!
//! 1. **A cat is one cat.** `bridge_send` runs one agent per window, and a
//!    second turn on the same cat supersedes the first. So chores queue behind
//!    each other, and behind you: the cat you're talking to finishes talking to
//!    you before it goes anywhere. A hunt the user's own turn cut short goes
//!    back to the front of the queue rather than counting as a failed errand.
//!
//! 2. **Nobody is watching.** A hunt has no panel open and no one reading its
//!    output, so what it did has to survive the moment. It streams to the
//!    window on its own channel — that's the cat you see padding across the
//!    taskbar — and what it comes back with is written down as a gift.
//!
//! The burn this implies is real and unsolved: every hunt spends the user's
//! subscription in the background, and they find out when their own session
//! hits a wall. What's here is the cheap half of an answer — a floor on how
//! often a chore may fire ([`chores::MIN_EVERY_MS`]), a run count on the board,
//! missed slots dropped rather than replayed, and a cat that yields the moment
//! you start typing. The rest is still open.

use crate::bridge::detect;
use crate::bridge::session::{self, BridgeState, Hunt, Outcome};
use crate::bridge::{has_adapter, TurnRequest};
use crate::chores::{Board, Chore, Gift};
use crate::memory::{now_ms, Memory};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Channel a finished hunt's gift arrives on.
pub const GIFT_EVENT: &str = "purrch://gift";

/// How often the calendar is read.
///
/// Nothing fires more than once every few minutes, so this only decides how
/// promptly a due chore gets going — including one that has been waiting for
/// the user to stop typing.
const TICK: Duration = Duration::from_secs(5);

/// What a cat is doing right now, for when you look over its shoulder.
///
/// Deliberately thin: the running commentary — "reading your inbox", "reviewing
/// PR #212" — is built by the panel from the event stream, because that's where
/// the events already are. This is what survives a window reload, which is just
/// enough to say *which* errand it's on.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Live {
    pub hunt: String,
    pub chore: String,
    pub name: String,
    pub since: i64,
}

/// What one cat is up to and what's lined up behind it.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub live: Option<Live>,
    /// Chores queued behind the current one, waiting their turn.
    pub waiting: usize,
}

#[derive(Default)]
struct Pack {
    queues: HashMap<String, VecDeque<String>>,
    live: HashMap<String, Live>,
}

/// Every cat's queue, and what each is hunting.
///
/// One lock over both maps rather than one each: "is this cat free, and if so
/// what's next" has to be a single question, or two threads both find it free
/// and set two agents running on the same window.
#[derive(Default)]
pub struct Hunts {
    pack: Mutex<Pack>,
}

impl Hunts {
    /// Lines a chore up for its cat. A chore already queued or already running
    /// isn't queued twice — a slow hunt must not stack up behind itself.
    pub fn queue(&self, cat: &str, chore: &str) {
        let mut pack = self.pack.lock().unwrap();
        if pack.live.get(cat).is_some_and(|l| l.chore == chore) {
            return;
        }
        let queue = pack.queues.entry(cat.to_string()).or_default();
        if queue.iter().any(|id| id == chore) {
            return;
        }
        queue.push_back(chore.to_string());
    }

    /// Puts a chore back at the head of the queue — used when the user's own
    /// turn cut a hunt short, so the cat picks it up again once you're done.
    fn requeue(&self, cat: &str, chore: &str) {
        let mut pack = self.pack.lock().unwrap();
        pack.queues
            .entry(cat.to_string())
            .or_default()
            .push_front(chore.to_string());
    }

    /// Takes the next chore for a free cat and marks it as under way, in one
    /// step so two callers can't both decide the cat is idle.
    fn claim(&self, cat: &str, board: &Board) -> Option<(Chore, Live)> {
        let mut pack = self.pack.lock().unwrap();
        if pack.live.contains_key(cat) {
            return None;
        }
        let queue = pack.queues.get_mut(cat)?;
        // A chore deleted while it sat in the queue is quietly dropped.
        let chore = loop {
            let id = queue.pop_front()?;
            if let Some(chore) = board.get(&id) {
                break chore;
            }
        };
        let live = Live {
            hunt: uuid::Uuid::new_v4().to_string(),
            chore: chore.id.clone(),
            name: chore.name.clone(),
            since: now_ms(),
        };
        pack.live.insert(cat.to_string(), live.clone());
        Some((chore, live))
    }

    fn release(&self, cat: &str) {
        self.pack.lock().unwrap().live.remove(cat);
    }

    /// Every cat with something waiting or running, so the pump only visits
    /// the ones that have a reason to be visited.
    fn active(&self) -> Vec<String> {
        let pack = self.pack.lock().unwrap();
        let mut out: Vec<String> = pack.live.keys().cloned().collect();
        for (cat, queue) in &pack.queues {
            if !queue.is_empty() && !out.contains(cat) {
                out.push(cat.clone());
            }
        }
        out
    }

    pub fn status(&self, cat: &str) -> Status {
        let pack = self.pack.lock().unwrap();
        Status {
            live: pack.live.get(cat).cloned(),
            waiting: pack.queues.get(cat).map_or(0, VecDeque::len),
        }
    }

    /// A cat that's gone home takes its queue with it.
    pub fn forget(&self, cat: &str) {
        let mut pack = self.pack.lock().unwrap();
        pack.queues.remove(cat);
        pack.live.remove(cat);
    }
}

/// Starts the colony's clock. One task for every cat, since a chore is only a
/// due-check and a queue push — the expensive part happens in its own task.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(TICK);
        // A tick delayed behind a slow one must not immediately fire again to
        // catch up: that would be two due-checks back to back for no reason.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            sweep(&app);
        }
    });
}

/// One pass of the clock: what's due, and who's free to go.
fn sweep(app: &AppHandle) {
    let board = app.state::<Arc<Board>>().inner().clone();
    let hunts = app.state::<Arc<Hunts>>().inner().clone();

    for id in board.due(now_ms()) {
        let Some(chore) = board.get(&id) else { continue };
        // A chore belongs to a cat, and a cat that isn't on the desktop isn't
        // there to do it. The slot has already been advanced, so this is a
        // miss rather than a backlog.
        if app.get_webview_window(&chore.cat).is_none() {
            continue;
        }
        hunts.queue(&chore.cat, &id);
    }

    // Also how a hunt that was queued behind the user's own typing eventually
    // gets going: every tick asks again whether the cat is free yet.
    for cat in hunts.active() {
        pump(app, &cat);
    }
}

/// Sends a cat after the next thing on its list, if it's free to go.
pub fn pump(app: &AppHandle, cat: &str) {
    let hunts = app.state::<Arc<Hunts>>().inner().clone();

    if app.get_webview_window(cat).is_none() {
        hunts.forget(cat);
        return;
    }
    // The cat you're talking to is busy talking to you.
    if app.state::<Arc<BridgeState>>().busy(cat) {
        return;
    }

    let board = app.state::<Arc<Board>>().inner().clone();
    let Some((chore, live)) = hunts.claim(cat, &board) else {
        return;
    };

    let app = app.clone();
    let cat = cat.to_string();
    tauri::async_runtime::spawn(async move {
        run(app.clone(), cat.clone(), chore, live).await;
        app.state::<Arc<Hunts>>().release(&cat);
        // Straight on to the next one, rather than waiting for the clock.
        pump(&app, &cat);
    });
}

/// Which CLI a cat hunts with.
///
/// The one it was last thinking with, as long as that's still installed and
/// driveable — otherwise anything that is. A hunt has nobody to ask.
fn brain(remembered: Option<&str>) -> Option<String> {
    let installed = detect::detect_all();
    let driveable = |id: &str| installed.iter().any(|b| b.id == id) && has_adapter(id);
    if let Some(id) = remembered.filter(|id| driveable(id)) {
        return Some(id.to_string());
    }
    installed
        .into_iter()
        .find(|b| has_adapter(&b.id))
        .map(|b| b.id)
}

/// What the cat is actually told, which is the chore plus the fact that it's a
/// chore.
///
/// The framing earns its place twice over: without it the cat writes as though
/// you'd just asked it something and are sitting there waiting, and — because
/// nothing here is watching — the last line it says is the entire gift, so it
/// has to be asked for one.
fn brief(chore: &Chore) -> String {
    format!(
        "{prompt}\n\n\
         [This is a chore called \"{name}\" that you do on a schedule. Nobody is \
         watching right now and nobody can answer a question, so don't ask one — \
         do the work, and if something genuinely needs a person, do what you can \
         and say so at the end. Finish with one short line saying what you found \
         or did. If there was nothing to do, say so in those words and stop; that \
         is a perfectly good outcome and most checks end that way.]",
        prompt = chore.prompt,
        name = chore.name,
    )
}

/// Runs one hunt and leaves whatever it caught by the door.
async fn run(app: AppHandle, cat: String, chore: Chore, live: Live) {
    let memory = app.state::<Arc<Memory>>().inner().clone();
    let board = app.state::<Arc<Board>>().inner().clone();
    let hunts = app.state::<Arc<Hunts>>().inner().clone();

    let past = memory.peek(&cat).unwrap_or_default();
    let Some(backend) = brain(past.backend.as_deref()) else {
        drop_gift(
            &app,
            &board,
            &chore,
            &live,
            false,
            "no agent CLI I can drive is installed",
            0,
        );
        return;
    };

    board.started(&chore.id, now_ms());

    let tag = Hunt {
        id: live.hunt.clone(),
        chore: chore.id.clone(),
        name: chore.name.clone(),
    };
    let request = |resume: Option<String>| TurnRequest {
        backend: backend.clone(),
        prompt: brief(&chore),
        resume,
        cwd: chore.cwd.clone().or_else(|| Some(crate::user_home())),
        model: None,
        name: past.name.clone(),
    };

    let mut outcome = turn(&app, &cat, request(chore.session.clone()), &tag).await;

    // The remembered conversation may be gone — agent CLIs prune transcripts,
    // and they're stored per working directory. Start over rather than letting
    // a chore fail every hour forever over a session id from last week.
    if outcome.error.is_some() && chore.session.is_some() && outcome.tools == 0 {
        board.forget_session(&chore.id);
        outcome = turn(&app, &cat, request(None), &tag).await;
    }

    // The user started typing, or the cat went home. Not a failed errand —
    // put it back at the front and it goes again when the cat is free.
    if outcome.cancelled {
        hunts.requeue(&cat, &chore.id);
        return;
    }

    board.finished(&chore.id, outcome.session.clone());

    let ok = outcome.ok && outcome.error.is_none();
    let text = outcome
        .text
        .clone()
        .or_else(|| outcome.error.clone())
        .unwrap_or_else(|| {
            if ok {
                "nothing new".to_string()
            } else {
                "the hunt ended badly and didn't say why".to_string()
            }
        });
    drop_gift(&app, &board, &chore, &live, ok, &text, outcome.tools);
}

/// One attempt, with a failure to even start turned into the same shape as a
/// failure part way through — the panel is watching this channel and a hunt
/// that goes quiet without ending would leave the cat working forever.
async fn turn(app: &AppHandle, cat: &str, request: TurnRequest, tag: &Hunt) -> Outcome {
    let state = app.state::<Arc<BridgeState>>().inner().clone();
    let creds = app.state::<Arc<crate::bridge::creds::Creds>>().inner().clone();

    match session::run(
        app.clone(),
        state,
        creds,
        cat.to_string(),
        request,
        Some(tag.clone()),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(message) => {
            let _ = app.emit_to(
                cat,
                session::HUNT_EVENT,
                serde_json::json!({
                    "hunt": tag,
                    "event": { "kind": "failed", "message": message },
                }),
            );
            Outcome {
                error: Some(message),
                ..Default::default()
            }
        }
    }
}

/// Leaves the gift on the doorstep and tells the cat's window about it.
fn drop_gift(
    app: &AppHandle,
    board: &Board,
    chore: &Chore,
    live: &Live,
    ok: bool,
    text: &str,
    tools: u32,
) {
    let gift: Gift = board.gift(chore, ok, text, tools);
    let _ = app.emit_to(
        &chore.cat,
        GIFT_EVENT,
        serde_json::json!({ "hunt": live.hunt, "gift": gift }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chores::Draft;

    fn scratch_board() -> Board {
        let dir = std::env::temp_dir().join(format!("purrch-hunt-{}", uuid::Uuid::new_v4()));
        Board::load(&dir)
    }

    fn chore(board: &Board, cat: &str, name: &str) -> Chore {
        board.add(
            cat,
            Draft {
                name: name.into(),
                prompt: "look at the thing".into(),
                cwd: None,
                every_ms: crate::chores::MIN_EVERY_MS,
                catch_up: false,
            },
        )
    }

    #[test]
    fn a_cat_hunts_one_thing_at_a_time() {
        let board = scratch_board();
        let hunts = Hunts::default();
        let first = chore(&board, "main", "inbox");
        let second = chore(&board, "main", "repo");

        hunts.queue("main", &first.id);
        hunts.queue("main", &second.id);

        let (taken, _) = hunts.claim("main", &board).unwrap();
        assert_eq!(taken.id, first.id);
        // Still out on the first errand: the second waits.
        assert!(hunts.claim("main", &board).is_none());
        assert_eq!(hunts.status("main").waiting, 1);

        hunts.release("main");
        let (taken, _) = hunts.claim("main", &board).unwrap();
        assert_eq!(taken.id, second.id);
    }

    /// The colony invariant, again: one cat being busy must not hold up another.
    #[test]
    fn cats_hunt_independently() {
        let board = scratch_board();
        let hunts = Hunts::default();
        let mine = chore(&board, "main", "inbox");
        let theirs = chore(&board, "cat-1", "repo");

        hunts.queue("main", &mine.id);
        hunts.queue("cat-1", &theirs.id);
        assert!(hunts.claim("main", &board).is_some());
        assert!(hunts.claim("cat-1", &board).is_some());
    }

    #[test]
    fn a_slow_chore_doesnt_stack_up_behind_itself() {
        let board = scratch_board();
        let hunts = Hunts::default();
        let c = chore(&board, "main", "inbox");

        hunts.queue("main", &c.id);
        hunts.queue("main", &c.id);
        assert_eq!(hunts.status("main").waiting, 1);

        // ...and it can't be queued again while it's out.
        hunts.claim("main", &board).unwrap();
        hunts.queue("main", &c.id);
        assert_eq!(hunts.status("main").waiting, 0);
    }

    #[test]
    fn a_hunt_you_interrupted_goes_back_to_the_front() {
        let board = scratch_board();
        let hunts = Hunts::default();
        let first = chore(&board, "main", "inbox");
        let second = chore(&board, "main", "repo");

        hunts.queue("main", &first.id);
        hunts.queue("main", &second.id);
        hunts.claim("main", &board).unwrap();
        // You started typing; the cat drops what it was doing.
        hunts.requeue("main", &first.id);
        hunts.release("main");

        let (taken, _) = hunts.claim("main", &board).unwrap();
        assert_eq!(taken.id, first.id, "the interrupted hunt lost its place");
    }

    #[test]
    fn a_chore_deleted_while_queued_is_quietly_dropped() {
        let board = scratch_board();
        let hunts = Hunts::default();
        let gone = chore(&board, "main", "inbox");
        let kept = chore(&board, "main", "repo");

        hunts.queue("main", &gone.id);
        hunts.queue("main", &kept.id);
        board.remove(&gone.id);

        let (taken, _) = hunts.claim("main", &board).unwrap();
        assert_eq!(taken.id, kept.id);
    }

    #[test]
    fn an_empty_queue_sends_nobody_anywhere() {
        let board = scratch_board();
        let hunts = Hunts::default();
        assert!(hunts.claim("main", &board).is_none());
        assert_eq!(hunts.status("main").waiting, 0);
        assert!(hunts.status("main").live.is_none());
    }

    #[test]
    fn a_cat_sent_home_takes_its_queue_with_it() {
        let board = scratch_board();
        let hunts = Hunts::default();
        let c = chore(&board, "cat-1", "inbox");
        hunts.queue("cat-1", &c.id);
        hunts.claim("cat-1", &board).unwrap();

        hunts.forget("cat-1");
        assert!(hunts.status("cat-1").live.is_none());
        assert_eq!(hunts.status("cat-1").waiting, 0);
        assert!(hunts.active().is_empty());
    }

    #[test]
    fn the_brief_says_it_is_a_chore_and_asks_for_one_line() {
        let board = scratch_board();
        let c = chore(&board, "main", "inbox");
        let brief = brief(&c);
        assert!(brief.starts_with("look at the thing"));
        assert!(brief.contains("\"inbox\""));
        // Both halves are load-bearing: no questions, and one line back.
        assert!(brief.contains("don't ask one"));
        assert!(brief.contains("one short line"));
    }
}
