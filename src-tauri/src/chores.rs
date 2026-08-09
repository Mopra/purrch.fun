//! The chore board, and the pile of gifts it produces.
//!
//! A chore is a standing instruction you hand to a cat: a prompt, a folder to
//! stand in, and how often to go and look. One execution of a chore is a
//! *hunt* (see [`crate::hunt`]), and whatever the cat comes back with is a
//! *gift* — left in a pile by the door for whenever you next look over.
//!
//! This file is the store and the calendar; it starts nothing and runs
//! nothing. It knows when a chore is due, what to do about one whose moment
//! passed while the PC was off, and how to keep the pile from growing forever.
//!
//! One JSON file for the whole colony, beside `cats.json` and with the same
//! promise: nothing here is load-bearing for the app running. A missing or
//! unreadable file means a colony with no chores yet, which is what the first
//! launch is anyway.

use crate::memory::now_ms;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The floor on how often a chore may fire.
///
/// Every hunt spends the user's subscription in the background, and they find
/// out about it when their own session hits a wall in the afternoon. A cat that
/// wanders over to look at something every few minutes is the product; one that
/// checks every ten seconds is a bill. Typing a smaller number is not refused,
/// it's just rounded up to this — see [`Chore::every`].
pub const MIN_EVERY_MS: i64 = 5 * 60 * 1000;

/// Gifts kept per cat before the oldest start falling off the pile.
const MAX_GIFTS: usize = 40;

/// How much of what a cat caught is kept. A gift is what it brings to the
/// door, not a transcript — the whole point is that it fits in a glance.
const GIFT_TEXT_MAX: usize = 600;

/// Steps kept per gift.
///
/// Enough to see what a chore actually did, capped so a runaway hunt can't
/// turn the board into a log file. A hunt that goes past this says so rather
/// than silently showing you the first sixty of four hundred.
const MAX_TRAIL: usize = 60;

/// One tool a cat picked up while it was out.
///
/// This is the part of a hunt nobody watched. The turn you ask for streams its
/// tool calls into the panel where you can see them go by, and that visible
/// feed is the only thing standing between the user and a silent action — but a
/// chore firing at 09:00 has no panel open and no one reading it. Without this,
/// "you can see what it did afterwards" was not true of the half of the app
/// most likely to be steered by something it read.
/// `default` on the serde side as well as the derive: a `Step` written by an
/// older build, or one hand-edited out of shape, should cost that one line of
/// the trail rather than the whole gift it belongs to.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Step {
    pub tool: String,
    /// The one-line summary the bridge already builds for the speech bubble.
    pub detail: String,
    /// `None` while it was still running — which is how a hunt that died
    /// mid-tool reads, and worth being able to tell apart from one that failed.
    pub ok: Option<bool>,
}

/// A standing instruction handed to one cat.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Chore {
    pub id: String,
    /// Window label of the cat that owns it. Chores are assignments — this one
    /// watches the repo, that one watches the inbox — so they belong to a cat
    /// rather than to the colony.
    pub cat: String,
    /// What you call it. Shows up on the board and on the gift.
    pub name: String,
    /// What the cat is actually told to do.
    pub prompt: String,
    /// Where it stands while doing it. `None` means the user's home.
    pub cwd: Option<String>,
    /// How often it goes and looks, in ms. Read through [`Chore::every`],
    /// which applies [`MIN_EVERY_MS`].
    pub every_ms: i64,
    pub enabled: bool,
    /// What to do about a slot that passed while the PC was off.
    ///
    /// `false` — the default — skips it: "check the inbox at 08:00" that comes
    /// due at 14:00 is not worth doing late. `true` runs it on wake, which is
    /// what you want for anything shaped like "tidy up since last time".
    pub catch_up: bool,
    /// When it next comes due, in ms since the epoch.
    pub next_due: i64,
    pub last_run: i64,
    pub runs: u64,
    /// This chore's own agent session, so a cat that checks the same thing
    /// every hour remembers what it saw last hour. Deliberately *not* the chat
    /// panel's session: a chore must not turn up in your conversation, and
    /// your conversation must not be what a chore resumes from.
    pub session: Option<String>,
}

impl Default for Chore {
    fn default() -> Self {
        Chore {
            id: String::new(),
            cat: String::new(),
            name: String::new(),
            prompt: String::new(),
            cwd: None,
            every_ms: MIN_EVERY_MS,
            enabled: true,
            catch_up: false,
            next_due: 0,
            last_run: 0,
            runs: 0,
            session: None,
        }
    }
}

impl Chore {
    /// The interval actually used, whatever the store says.
    pub fn every(&self) -> i64 {
        self.every_ms.max(MIN_EVERY_MS)
    }
}

/// What a cat brought back. The mouse on the doorstep.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Gift {
    pub id: String,
    pub cat: String,
    /// The chore it came from, and what that chore was called at the time —
    /// kept by value so a renamed or deleted chore doesn't rewrite history.
    pub chore: String,
    pub chore_name: String,
    pub at: i64,
    pub ok: bool,
    /// What the cat said it found. One or two lines, by the persona.
    pub text: String,
    /// Tools it picked up getting there. Can exceed `trail.len()` — that's how
    /// you know the trail was cut off at [`MAX_TRAIL`].
    pub tools: u32,
    /// What it actually did, in order. See [`Step`].
    pub trail: Vec<Step>,
    /// Whether you've looked at it yet. Unread gifts are the pile.
    pub read: bool,
}

impl Default for Gift {
    fn default() -> Self {
        Gift {
            id: String::new(),
            cat: String::new(),
            chore: String::new(),
            chore_name: String::new(),
            at: 0,
            ok: true,
            text: String::new(),
            tools: 0,
            trail: Vec::new(),
            read: false,
        }
    }
}

/// A chore as it arrives from the board UI, before it has an id or a slot.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub name: String,
    pub prompt: String,
    #[serde(default)]
    pub cwd: Option<String>,
    pub every_ms: i64,
    #[serde(default)]
    pub catch_up: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    chores: Vec<Chore>,
    #[serde(default)]
    gifts: Vec<Gift>,
}

fn write(path: &Path, store: &Store) {
    let Ok(json) = serde_json::to_string_pretty(store) else {
        return;
    };
    // Through a temporary first, as with the cats: a half-written file after a
    // power cut would cost the user every chore they've written.
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, json).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

/// Trims what a cat caught down to something that fits by the door.
fn clip(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= GIFT_TEXT_MAX {
        return trimmed.to_string();
    }
    let head: String = trimmed.chars().take(GIFT_TEXT_MAX - 1).collect();
    format!("{head}…")
}

/// The board itself, shared by every cat window and by the scheduler.
pub struct Board {
    path: PathBuf,
    store: Mutex<Store>,
}

impl Board {
    pub fn load(dir: &Path) -> Self {
        let _ = fs::create_dir_all(dir);
        let path = dir.join("chores.json");
        let store = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Store>(&raw).ok())
            .unwrap_or_default();
        Board {
            path,
            store: Mutex::new(store),
        }
    }

    /// Everything on one cat's board, in the order it was written down.
    pub fn for_cat(&self, cat: &str) -> Vec<Chore> {
        self.store
            .lock()
            .unwrap()
            .chores
            .iter()
            .filter(|c| c.cat == cat)
            .cloned()
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Chore> {
        self.store
            .lock()
            .unwrap()
            .chores
            .iter()
            .find(|c| c.id == id)
            .cloned()
    }

    /// Adds a chore, due one full interval from now.
    ///
    /// Not due immediately, deliberately: writing a chore down shouldn't set an
    /// agent running the instant you stop typing, before you've read back what
    /// you wrote. "Run now" is right there on the board for when you do want it.
    pub fn add(&self, cat: &str, draft: Draft) -> Chore {
        let now = now_ms();
        let chore = Chore {
            id: uuid::Uuid::new_v4().to_string(),
            cat: cat.to_string(),
            name: draft.name,
            prompt: draft.prompt,
            cwd: draft.cwd,
            every_ms: draft.every_ms.max(MIN_EVERY_MS),
            enabled: true,
            catch_up: draft.catch_up,
            next_due: now + draft.every_ms.max(MIN_EVERY_MS),
            last_run: 0,
            runs: 0,
            session: None,
        };
        let mut store = self.store.lock().unwrap();
        store.chores.push(chore.clone());
        write(&self.path, &store);
        chore
    }

    /// Applies a shallow patch from the board UI.
    ///
    /// The chore's identity and its history are ours, not the frontend's: an
    /// edit changes what the cat is asked to do, never what it has already
    /// done. Changing the interval re-slots it from now, so making a chore
    /// hourly doesn't leave it due on the old minute.
    ///
    /// `cat` is the window asking, and a chore that isn't its own is not
    /// found — the same answer as a chore that doesn't exist. See
    /// [`Board::mine`] for why that's the shape rather than a permission error.
    pub fn update(&self, cat: &str, id: &str, patch: &Value) -> Result<Chore, String> {
        let Some(patch) = patch.as_object() else {
            return Err("a chore patch has to be an object".into());
        };
        let mut store = self.store.lock().unwrap();
        let Some(slot) = store.chores.iter().position(|c| c.id == id && c.cat == cat) else {
            return Err("no such chore".into());
        };
        let before = store.chores[slot].clone();

        let Ok(Value::Object(mut base)) = serde_json::to_value(&before) else {
            return Err("couldn't read the chore".into());
        };
        for (key, value) in patch {
            base.insert(key.clone(), value.clone());
        }
        let mut next: Chore =
            serde_json::from_value(Value::Object(base)).map_err(|e| e.to_string())?;

        next.id = before.id;
        next.cat = before.cat;
        next.runs = before.runs;
        next.last_run = before.last_run;
        next.every_ms = next.every_ms.max(MIN_EVERY_MS);
        if next.every_ms != before.every_ms {
            next.next_due = now_ms() + next.every_ms;
        } else {
            next.next_due = before.next_due;
        }
        // A rewritten chore is a different job; resuming the old conversation
        // would have the cat answering the question it was asked last week.
        if next.prompt != before.prompt || next.cwd != before.cwd {
            next.session = None;
        } else {
            next.session = before.session;
        }

        store.chores[slot] = next.clone();
        write(&self.path, &store);
        Ok(next)
    }

    pub fn remove(&self, cat: &str, id: &str) {
        let mut store = self.store.lock().unwrap();
        let before = store.chores.len();
        store.chores.retain(|c| !(c.id == id && c.cat == cat));
        if store.chores.len() != before {
            write(&self.path, &store);
        }
    }

    /// This cat's own copy of a chore, or `None` if the id belongs to a sibling.
    ///
    /// Every command a window can reach goes through here rather than through
    /// [`Board::get`], which is the scheduler's unscoped view. Not a permission
    /// check — nothing in Purrch is — but a cat's board is *its* board, and a
    /// window that could rewrite another's would make the colony one shared
    /// to-do list shown several times.
    pub fn mine(&self, cat: &str, id: &str) -> Option<Chore> {
        self.store
            .lock()
            .unwrap()
            .chores
            .iter()
            .find(|c| c.id == id && c.cat == cat)
            .cloned()
    }

    /// Advances the calendar to `now` and reports which chores should be hunted.
    ///
    /// Every chore whose moment has come is re-slotted whether or not it gets
    /// hunted, so a PC that was off all weekend comes back to a board that is
    /// due once — not to sixty backed-up runs firing at breakfast. A slot
    /// missed by more than a whole interval is one that passed while nobody was
    /// home, and by default it's simply let go: see [`Chore::catch_up`].
    pub fn due(&self, now: i64) -> Vec<String> {
        let mut store = self.store.lock().unwrap();
        let mut out = Vec::new();
        for chore in &mut store.chores {
            if !chore.enabled || chore.next_due > now {
                continue;
            }
            let every = chore.every();
            let late = now - chore.next_due;
            chore.next_due = now + every;
            if late > every && !chore.catch_up {
                continue;
            }
            out.push(chore.id.clone());
        }
        if !out.is_empty() {
            write(&self.path, &store);
        }
        out
    }

    /// Brings a chore's next slot forward to now — the board's "run now".
    pub fn nudge(&self, cat: &str, id: &str) {
        let mut store = self.store.lock().unwrap();
        if let Some(chore) = store.chores.iter_mut().find(|c| c.id == id && c.cat == cat) {
            chore.next_due = now_ms();
            write(&self.path, &store);
        }
    }

    /// A hunt has set off.
    pub fn started(&self, id: &str, now: i64) {
        let mut store = self.store.lock().unwrap();
        if let Some(chore) = store.chores.iter_mut().find(|c| c.id == id) {
            chore.last_run = now;
            chore.runs += 1;
            write(&self.path, &store);
        }
    }

    /// A hunt is over; remember the conversation so the next one continues it.
    pub fn finished(&self, id: &str, session: Option<String>) {
        let mut store = self.store.lock().unwrap();
        if let Some(chore) = store.chores.iter_mut().find(|c| c.id == id) {
            if session.is_some() {
                chore.session = session;
            }
            write(&self.path, &store);
        }
    }

    /// Forgets a chore's conversation, so its next hunt starts fresh. Used when
    /// a resumed session turns out to be gone.
    pub fn forget_session(&self, id: &str) {
        let mut store = self.store.lock().unwrap();
        if let Some(chore) = store.chores.iter_mut().find(|c| c.id == id) {
            chore.session = None;
            write(&self.path, &store);
        }
    }

    /// Leaves what a cat caught by the door, and the trail of how it got there.
    pub fn gift(&self, chore: &Chore, ok: bool, text: &str, tools: u32, trail: &[Step]) -> Gift {
        let gift = Gift {
            id: uuid::Uuid::new_v4().to_string(),
            cat: chore.cat.clone(),
            chore: chore.id.clone(),
            chore_name: chore.name.clone(),
            at: now_ms(),
            ok,
            text: clip(text),
            tools,
            // The tail rather than the head: a hunt that ran long went wrong
            // near the end, and what it did last is what you want to see.
            trail: trail
                .iter()
                .skip(trail.len().saturating_sub(MAX_TRAIL))
                .cloned()
                .collect(),
            read: false,
        };
        let mut store = self.store.lock().unwrap();
        store.gifts.push(gift.clone());

        // Cap per cat rather than overall, or one busy cat would sweep a quiet
        // one's pile away before it was ever looked at.
        let mut seen = 0;
        let cat = gift.cat.clone();
        let over = store.gifts.iter().filter(|g| g.cat == cat).count();
        if over > MAX_GIFTS {
            let drop = over - MAX_GIFTS;
            store.gifts.retain(|g| {
                if g.cat != cat {
                    return true;
                }
                seen += 1;
                seen > drop
            });
        }
        write(&self.path, &store);
        gift
    }

    /// One cat's pile, newest first — the order you'd look at it in.
    pub fn gifts_for(&self, cat: &str) -> Vec<Gift> {
        let mut out: Vec<Gift> = self
            .store
            .lock()
            .unwrap()
            .gifts
            .iter()
            .filter(|g| g.cat == cat)
            .cloned()
            .collect();
        out.reverse();
        out
    }

    /// Marks gifts as looked at. An empty `ids` means the whole pile.
    pub fn read(&self, cat: &str, ids: &[String]) {
        let mut store = self.store.lock().unwrap();
        for gift in store.gifts.iter_mut().filter(|g| g.cat == cat) {
            if ids.is_empty() || ids.iter().any(|id| id == &gift.id) {
                gift.read = true;
            }
        }
        write(&self.path, &store);
    }

    /// Sweeps the pile away.
    pub fn clear(&self, cat: &str) {
        let mut store = self.store.lock().unwrap();
        store.gifts.retain(|g| g.cat != cat);
        write(&self.path, &store);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("purrch-chores-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn draft(every_ms: i64) -> Draft {
        Draft {
            name: "watch the inbox".into(),
            prompt: "read new mail, put anything with a date in the calendar".into(),
            cwd: None,
            every_ms,
            catch_up: false,
        }
    }

    #[test]
    fn a_new_chore_isnt_due_the_moment_you_write_it() {
        let board = Board::load(&scratch());
        let chore = board.add("main", draft(MIN_EVERY_MS));
        assert!(chore.next_due > now_ms());
        assert!(board.due(now_ms()).is_empty());
    }

    #[test]
    fn nothing_may_fire_faster_than_the_floor() {
        let board = Board::load(&scratch());
        // Ten seconds would be an expensive way to find out about the burn.
        let chore = board.add("main", draft(10_000));
        assert_eq!(chore.every_ms, MIN_EVERY_MS);
        let patched = board
            .update("main", &chore.id, &json!({ "everyMs": 1000 }))
            .unwrap();
        assert_eq!(patched.every_ms, MIN_EVERY_MS);
    }

    #[test]
    fn a_due_chore_is_reported_once_and_re_slotted() {
        let board = Board::load(&scratch());
        let chore = board.add("main", draft(MIN_EVERY_MS));
        let at = chore.next_due;

        assert_eq!(board.due(at), vec![chore.id.clone()]);
        // Asking again a moment later must not fire it a second time.
        assert!(board.due(at + 1).is_empty());
        assert!(board.get(&chore.id).unwrap().next_due > at);
    }

    #[test]
    fn a_slot_missed_while_the_pc_was_off_is_let_go() {
        let board = Board::load(&scratch());
        let chore = board.add("main", draft(MIN_EVERY_MS));
        // Woken a whole weekend past the moment it was meant to run.
        let late = chore.next_due + MIN_EVERY_MS * 600;
        assert!(board.due(late).is_empty());
        // ...but it's back on the calendar rather than stuck in the past.
        assert!(board.get(&chore.id).unwrap().next_due > late);
    }

    #[test]
    fn a_catch_up_chore_runs_late_instead() {
        let board = Board::load(&scratch());
        let chore = board.add("main", {
            let mut d = draft(MIN_EVERY_MS);
            d.catch_up = true;
            d
        });
        let late = chore.next_due + MIN_EVERY_MS * 600;
        assert_eq!(board.due(late), vec![chore.id]);
    }

    #[test]
    fn a_disabled_chore_never_comes_due() {
        let board = Board::load(&scratch());
        let chore = board.add("main", draft(MIN_EVERY_MS));
        board
            .update("main", &chore.id, &json!({ "enabled": false }))
            .unwrap();
        assert!(board.due(chore.next_due + MIN_EVERY_MS).is_empty());
    }

    #[test]
    fn run_now_brings_a_chore_forward() {
        let board = Board::load(&scratch());
        let chore = board.add("main", draft(MIN_EVERY_MS));
        board.nudge("main", &chore.id);
        assert_eq!(board.due(now_ms()), vec![chore.id]);
    }

    #[test]
    fn editing_a_chore_cant_rewrite_what_it_has_done() {
        let board = Board::load(&scratch());
        let chore = board.add("main", draft(MIN_EVERY_MS));
        board.started(&chore.id, now_ms());

        let patched = board
            .update(
                "main",
                &chore.id,
                &json!({ "name": "watch the other inbox", "runs": 999, "cat": "cat-9" }),
            )
            .unwrap();
        assert_eq!(patched.name, "watch the other inbox");
        assert_eq!(patched.runs, 1);
        assert_eq!(patched.cat, "main");
    }

    #[test]
    fn rewriting_the_job_drops_the_conversation_behind_it() {
        let board = Board::load(&scratch());
        let chore = board.add("main", draft(MIN_EVERY_MS));
        board.finished(&chore.id, Some("session-1".into()));

        // A cosmetic edit keeps its continuity...
        let renamed = board
            .update("main", &chore.id, &json!({ "name": "mail" }))
            .unwrap();
        assert_eq!(renamed.session.as_deref(), Some("session-1"));

        // ...but asking for something else entirely does not.
        let rewritten = board
            .update(
                "main",
                &chore.id,
                &json!({ "prompt": "tidy the downloads folder" }),
            )
            .unwrap();
        assert_eq!(rewritten.session, None);
    }

    #[test]
    fn chores_belong_to_one_cat() {
        let board = Board::load(&scratch());
        board.add("main", draft(MIN_EVERY_MS));
        board.add("cat-1", draft(MIN_EVERY_MS));
        assert_eq!(board.for_cat("main").len(), 1);
        assert_eq!(board.for_cat("cat-1").len(), 1);
    }

    /// ...and a cat holding a sibling's id can do nothing with it. What makes
    /// the colony a colony rather than one shared list shown several times.
    #[test]
    fn one_cat_cannot_touch_anothers_board() {
        let board = Board::load(&scratch());
        let theirs = board.add("cat-1", draft(MIN_EVERY_MS));
        let due_at = theirs.next_due;

        assert!(board.mine("main", &theirs.id).is_none());
        assert!(board.mine("cat-1", &theirs.id).is_some());

        // An edit is refused rather than quietly applied to somebody else's.
        assert!(board
            .update("main", &theirs.id, &json!({ "name": "mine now" }))
            .is_err());
        assert_eq!(board.get(&theirs.id).unwrap().name, "watch the inbox");

        // A delete does nothing at all...
        board.remove("main", &theirs.id);
        assert_eq!(board.for_cat("cat-1").len(), 1);

        // ...and neither does dragging its slot forward.
        board.nudge("main", &theirs.id);
        assert_eq!(board.get(&theirs.id).unwrap().next_due, due_at);

        // The owner is still free to do all three.
        assert!(board
            .update("cat-1", &theirs.id, &json!({ "name": "mail" }))
            .is_ok());
        board.remove("cat-1", &theirs.id);
        assert!(board.for_cat("cat-1").is_empty());
    }

    #[test]
    fn gifts_pile_up_newest_first_and_stay_the_cats_own() {
        let board = Board::load(&scratch());
        let mine = board.add("main", draft(MIN_EVERY_MS));
        let theirs = board.add("cat-1", draft(MIN_EVERY_MS));

        board.gift(&mine, true, "nothing new", 2, &[]);
        board.gift(&mine, true, "three PRs reviewed", 9, &[]);
        board.gift(&theirs, false, "couldn't reach GitHub", 1, &[]);

        let pile = board.gifts_for("main");
        assert_eq!(pile.len(), 2);
        assert_eq!(pile[0].text, "three PRs reviewed");
        assert_eq!(board.gifts_for("cat-1").len(), 1);
    }

    #[test]
    fn one_busy_cat_cant_sweep_away_a_quiet_ones_pile() {
        let board = Board::load(&scratch());
        let quiet = board.add("cat-1", draft(MIN_EVERY_MS));
        let busy = board.add("main", draft(MIN_EVERY_MS));
        board.gift(&quiet, true, "one for later", 0, &[]);
        for i in 0..MAX_GIFTS + 10 {
            board.gift(&busy, true, &format!("catch {i}"), 0, &[]);
        }
        assert_eq!(board.gifts_for("cat-1").len(), 1);
        let pile = board.gifts_for("main");
        assert_eq!(pile.len(), MAX_GIFTS);
        // The oldest fell off the bottom, not the newest off the top.
        assert_eq!(pile[0].text, format!("catch {}", MAX_GIFTS + 9));
    }

    /// The record of an unwatched turn. Nobody saw the tool feed go by, so if
    /// this doesn't land there is no answer to "what did it actually do".
    #[test]
    fn a_gift_carries_the_trail_of_what_the_cat_did() {
        let board = Board::load(&scratch());
        let chore = board.add("main", draft(MIN_EVERY_MS));
        let trail = vec![
            Step {
                tool: "Bash".into(),
                detail: "gh pr list".into(),
                ok: Some(true),
            },
            Step {
                tool: "Write".into(),
                detail: "C:\\notes\\prs.md".into(),
                ok: Some(false),
            },
            // Still open: the hunt died holding this one.
            Step {
                tool: "WebFetch".into(),
                detail: "https://example.invalid".into(),
                ok: None,
            },
        ];

        let gift = board.gift(&chore, true, "three PRs reviewed", 3, &trail);
        assert_eq!(gift.trail.len(), 3);
        assert_eq!(gift.trail[0].detail, "gh pr list");
        assert_eq!(gift.trail[1].ok, Some(false));
        assert_eq!(gift.trail[2].ok, None);

        // ...and it survives the trip through the store, which is the only
        // reason it's worth keeping at all.
        let kept = &board.gifts_for("main")[0];
        assert_eq!(kept.trail.len(), 3);
        assert_eq!(kept.trail[2].tool, "WebFetch");
    }

    #[test]
    fn a_long_hunt_keeps_the_end_of_its_trail_and_says_how_much_it_did() {
        let board = Board::load(&scratch());
        let chore = board.add("main", draft(MIN_EVERY_MS));
        let trail: Vec<Step> = (0..MAX_TRAIL + 25)
            .map(|i| Step {
                tool: "Bash".into(),
                detail: format!("step {i}"),
                ok: Some(true),
            })
            .collect();

        let gift = board.gift(&chore, true, "busy morning", trail.len() as u32, &trail);
        assert_eq!(gift.trail.len(), MAX_TRAIL);
        // The tail, not the head — a hunt that ran long went wrong near the end.
        assert_eq!(gift.trail[0].detail, format!("step {}", 25));
        assert_eq!(
            gift.trail[MAX_TRAIL - 1].detail,
            format!("step {}", MAX_TRAIL + 24)
        );
        // And the count still tells you the trail isn't the whole story.
        assert!(gift.tools as usize > gift.trail.len());
    }

    #[test]
    fn a_gift_is_what_fits_by_the_door() {
        let board = Board::load(&scratch());
        let chore = board.add("main", draft(MIN_EVERY_MS));
        let gift = board.gift(&chore, true, &"x".repeat(5000), 0, &[]);
        assert_eq!(gift.text.chars().count(), GIFT_TEXT_MAX);
    }

    #[test]
    fn reading_and_sweeping_the_pile() {
        let board = Board::load(&scratch());
        let chore = board.add("main", draft(MIN_EVERY_MS));
        let a = board.gift(&chore, true, "one", 0, &[]);
        board.gift(&chore, true, "two", 0, &[]);

        board.read("main", std::slice::from_ref(&a.id));
        let pile = board.gifts_for("main");
        assert_eq!(pile.iter().filter(|g| !g.read).count(), 1);

        board.read("main", &[]);
        assert!(board.gifts_for("main").iter().all(|g| g.read));

        board.clear("main");
        assert!(board.gifts_for("main").is_empty());
    }

    #[test]
    fn a_deleted_chore_leaves_its_gifts_behind() {
        // What the cat caught happened; deleting the standing order doesn't
        // un-happen it, and the gift carries the name it had at the time.
        let board = Board::load(&scratch());
        let chore = board.add("main", draft(MIN_EVERY_MS));
        board.gift(&chore, true, "one email needed you", 4, &[]);
        board.remove("main", &chore.id);

        assert!(board.for_cat("main").is_empty());
        let pile = board.gifts_for("main");
        assert_eq!(pile.len(), 1);
        assert_eq!(pile[0].chore_name, "watch the inbox");
    }

    #[test]
    fn the_board_outlives_the_process() {
        let dir = scratch();
        let id = {
            let board = Board::load(&dir);
            let chore = board.add("main", draft(MIN_EVERY_MS));
            board.gift(&chore, true, "nothing new", 1, &[]);
            chore.id
        };
        let board = Board::load(&dir);
        assert_eq!(board.get(&id).unwrap().name, "watch the inbox");
        assert_eq!(board.gifts_for("main").len(), 1);
    }

    #[test]
    fn a_corrupt_board_costs_the_chores_but_not_the_app() {
        let dir = scratch();
        fs::write(dir.join("chores.json"), "{ not json").unwrap();
        let board = Board::load(&dir);
        assert!(board.for_cat("main").is_empty());
        assert!(board.add("main", draft(MIN_EVERY_MS)).runs == 0);
    }
}
