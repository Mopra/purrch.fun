//! What's true of the whole colony rather than of one cat.
//!
//! Two things live here, and they are both about the same thing — the cats act
//! on their own, and this is what the user gets to say about that before and
//! after they do.
//!
//! **The agreement.** Purrch runs every backend with permission checks off, and
//! the user agrees to that once. That agreement used to live in the frontend's
//! `localStorage`, which meant it protected exactly the one screen that read
//! it: the composer. Anything reaching an agent by another route — the chore
//! board, a hunt firing on the clock, the microphone — went around it. It lives
//! here now because Rust is where every one of those routes actually ends, so
//! the check can be made once, at the door, instead of at each window.
//!
//! **The budget.** Every hunt spends the user's subscription in the background,
//! and they find out about it when their own session hits a wall in the
//! afternoon. `chores.rs` has the floor on how *often* a chore may fire; this is
//! the ceiling on how *much* a cat may do in a day, which is the half that
//! actually bounds the bill. It's a rolling 24-hour window rather than a
//! calendar day on purpose: no timezone to get wrong, and "40 in the last day"
//! is a sentence that means the same thing at any hour.
//!
//! One JSON file for the colony, beside `cats.json`, with the same promise as
//! the rest: unreadable means a colony that hasn't agreed to anything yet and
//! has spent nothing, which is exactly what a first launch is.

use crate::memory::now_ms;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The window the budget is measured over.
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Hunts one cat may run in a rolling day before it has to rest.
///
/// Picked to be generous for the chores the board actually suggests — hourly
/// checks come to 24 — and still low enough that a cat set to the 15-minute
/// floor runs out in the afternoon rather than at four in the morning, which is
/// the difference between finding out from Purrch and finding out from Claude.
pub const DEFAULT_DAILY_HUNTS: u32 = 40;

/// The most a user can set the cap to. Not a safety rail — it's a reminder that
/// this number is the bill, and there's a number beyond which "cap" is a word
/// doing no work.
pub const MAX_DAILY_HUNTS: u32 = 500;

/// What the panel needs to know about the colony as a whole.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// The user has agreed that the cats run with no permission checks.
    pub unleashed: bool,
    /// Hunts allowed per cat per rolling day.
    pub daily_hunts: u32,
}

/// One cat's budget, right now.
#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spend {
    /// Hunts this cat has run in the last 24 hours.
    pub today: u32,
    /// Out of this many.
    pub cap: u32,
    /// When the oldest of those falls out of the window and a slot frees up.
    /// `None` when the cat isn't at its cap.
    pub next_free: Option<i64>,
}

impl Spend {
    pub fn spent_out(&self) -> bool {
        self.today >= self.cap
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Store {
    unleashed: bool,
    /// When the user agreed, so the panel could say. Kept mostly because a
    /// consent flag with no date is a worse record than one with a date.
    unleashed_at: i64,
    daily_hunts: Option<u32>,
    /// Per cat, the times of the hunts in the recent past. Pruned to the window
    /// on every read, so this stays bounded by the cap rather than by uptime.
    hunts: BTreeMap<String, Vec<i64>>,
}

fn write(path: &Path, store: &Store) {
    let Ok(json) = serde_json::to_string_pretty(store) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, json).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

pub struct Colony {
    path: PathBuf,
    store: Mutex<Store>,
}

impl Colony {
    pub fn load(dir: &Path) -> Self {
        let _ = fs::create_dir_all(dir);
        let path = dir.join("colony.json");
        let store = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Store>(&raw).ok())
            .unwrap_or_default();
        Colony {
            path,
            store: Mutex::new(store),
        }
    }

    fn cap_of(store: &Store) -> u32 {
        store
            .daily_hunts
            .unwrap_or(DEFAULT_DAILY_HUNTS)
            .min(MAX_DAILY_HUNTS)
    }

    pub fn settings(&self) -> Settings {
        let store = self.store.lock().unwrap();
        Settings {
            unleashed: store.unleashed,
            daily_hunts: Self::cap_of(&store),
        }
    }

    /// Whether the colony may act unasked at all.
    ///
    /// Everything that can reach an agent asks this first. It is deliberately
    /// the cheapest possible question — a bool behind a mutex — because it is
    /// on the path of every turn, every hunt and every spoken command.
    pub fn unleashed(&self) -> bool {
        self.store.lock().unwrap().unleashed
    }

    /// The user has read the gate and said yes. One way on purpose: there is no
    /// `leash()`, because taking the agreement back doesn't stop an agent that
    /// is already running, and a switch that implies otherwise would be a lie.
    /// Closing the app is what stops a cat.
    pub fn unleash(&self) {
        let mut store = self.store.lock().unwrap();
        if store.unleashed {
            return;
        }
        store.unleashed = true;
        store.unleashed_at = now_ms();
        write(&self.path, &store);
    }

    /// Sets the per-cat daily ceiling, clamped to something meaningful.
    pub fn set_daily_hunts(&self, hunts: u32) -> Settings {
        let mut store = self.store.lock().unwrap();
        store.daily_hunts = Some(hunts.clamp(1, MAX_DAILY_HUNTS));
        write(&self.path, &store);
        Settings {
            unleashed: store.unleashed,
            daily_hunts: Self::cap_of(&store),
        }
    }

    /// What `cat` has spent, without spending anything.
    pub fn spend(&self, cat: &str) -> Spend {
        let mut store = self.store.lock().unwrap();
        Self::look(&mut store, cat, now_ms())
    }

    /// Books one hunt against `cat`'s budget, or refuses.
    ///
    /// Read-and-write in one step under one lock: two chores coming due in the
    /// same tick must not both look at a cat with one slot left and both take
    /// it. The refusal is the *only* thing standing between a busy board and a
    /// subscription spent by lunchtime, so it cannot be advisory.
    pub fn charge(&self, cat: &str) -> Result<Spend, Spend> {
        let now = now_ms();
        let mut store = self.store.lock().unwrap();
        let before = Self::look(&mut store, cat, now);
        if before.spent_out() {
            return Err(before);
        }
        store.hunts.entry(cat.to_string()).or_default().push(now);
        let after = Self::look(&mut store, cat, now);
        write(&self.path, &store);
        Ok(after)
    }

    /// A cat that's gone for good takes its ledger with it. Called when a cat
    /// is dismissed rather than when its window closes — quitting Purrch must
    /// not be a way to reset the budget.
    pub fn forget(&self, cat: &str) {
        let mut store = self.store.lock().unwrap();
        if store.hunts.remove(cat).is_some() {
            write(&self.path, &store);
        }
    }

    /// Prunes `cat`'s ledger to the window and reports where it stands.
    fn look(store: &mut Store, cat: &str, now: i64) -> Spend {
        let cap = Self::cap_of(store);
        let Some(times) = store.hunts.get_mut(cat) else {
            return Spend {
                today: 0,
                cap,
                next_free: None,
            };
        };
        times.retain(|at| now - *at < DAY_MS);
        let today = times.len() as u32;
        Spend {
            today,
            cap,
            // The oldest hunt still in the window is the one whose expiry frees
            // the next slot.
            next_free: (today >= cap)
                .then(|| times.iter().min().copied())
                .flatten()
                .map(|oldest| oldest + DAY_MS),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("purrch-colony-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_fresh_colony_has_agreed_to_nothing() {
        let colony = Colony::load(&scratch());
        assert!(!colony.unleashed());
        assert!(!colony.settings().unleashed);
        assert_eq!(colony.settings().daily_hunts, DEFAULT_DAILY_HUNTS);
    }

    #[test]
    fn the_agreement_outlives_the_process() {
        let dir = scratch();
        Colony::load(&dir).unleash();
        assert!(Colony::load(&dir).unleashed());
    }

    /// The whole reason this moved out of `localStorage`: clearing the
    /// frontend's storage must not un-say what the user said.
    #[test]
    fn the_agreement_is_not_the_frontends_to_forget() {
        let dir = scratch();
        let colony = Colony::load(&dir);
        colony.unleash();
        // Nothing in the public API can take it back...
        assert!(colony.unleashed());
        // ...and a brand-new store over the same file agrees.
        assert!(Colony::load(&dir).unleashed());
    }

    #[test]
    fn a_cap_can_be_set_but_not_to_something_meaningless() {
        let colony = Colony::load(&scratch());
        assert_eq!(colony.set_daily_hunts(10).daily_hunts, 10);
        assert_eq!(colony.set_daily_hunts(0).daily_hunts, 1);
        assert_eq!(
            colony.set_daily_hunts(u32::MAX).daily_hunts,
            MAX_DAILY_HUNTS
        );
    }

    #[test]
    fn hunts_are_charged_until_the_cat_is_spent_out() {
        let colony = Colony::load(&scratch());
        colony.set_daily_hunts(3);

        for expected in 1..=3 {
            let spend = colony.charge("main").expect("still had budget");
            assert_eq!(spend.today, expected);
        }

        let Err(spent) = colony.charge("main") else {
            panic!("a cat over its cap was allowed out anyway");
        };
        assert!(spent.spent_out());
        assert!(spent.next_free.is_some(), "no idea when it can go again");
    }

    /// The colony invariant, in the one place it costs money: one cat running
    /// out must not ground its siblings.
    #[test]
    fn cats_have_their_own_budgets() {
        let colony = Colony::load(&scratch());
        colony.set_daily_hunts(1);
        assert!(colony.charge("main").is_ok());
        assert!(colony.charge("main").is_err());
        assert!(colony.charge("cat-1").is_ok());
    }

    #[test]
    fn spending_survives_a_restart() {
        let dir = scratch();
        {
            let colony = Colony::load(&dir);
            colony.set_daily_hunts(2);
            colony.charge("main").unwrap();
        }
        // Otherwise closing Purrch would be how you reset the bill.
        let colony = Colony::load(&dir);
        assert_eq!(colony.spend("main").today, 1);
        assert!(colony.charge("main").is_ok());
        assert!(colony.charge("main").is_err());
    }

    #[test]
    fn yesterdays_hunts_fall_out_of_the_window() {
        let dir = scratch();
        let colony = Colony::load(&dir);
        colony.set_daily_hunts(2);

        // Two hunts, both a day and a bit ago.
        {
            let mut store = colony.store.lock().unwrap();
            let stale = now_ms() - DAY_MS - 60_000;
            store.hunts.insert("main".into(), vec![stale, stale + 1]);
        }

        assert_eq!(colony.spend("main").today, 0);
        assert!(colony.charge("main").is_ok());
    }

    #[test]
    fn a_dismissed_cat_takes_its_ledger_with_it() {
        let colony = Colony::load(&scratch());
        colony.charge("cat-1").unwrap();
        assert_eq!(colony.spend("cat-1").today, 1);
        colony.forget("cat-1");
        assert_eq!(colony.spend("cat-1").today, 0);
    }

    #[test]
    fn a_corrupt_store_costs_the_settings_but_not_the_app() {
        let dir = scratch();
        fs::write(dir.join("colony.json"), "{ not json at all").unwrap();
        let colony = Colony::load(&dir);
        // Fails closed on the agreement — the one default that matters.
        assert!(!colony.unleashed());
        assert_eq!(colony.settings().daily_hunts, DEFAULT_DAILY_HUNTS);
    }
}
