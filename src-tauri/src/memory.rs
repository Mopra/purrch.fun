//! What a cat remembers between launches.
//!
//! A cat that starts over every time you open Purrch is a different cat wearing
//! the same sprite. This is the file that makes it the same one: the patch of
//! taskbar it was standing on, whether it had dozed off, how many times you've
//! scratched it, and the conversation you were in the middle of.
//!
//! One JSON file for the whole colony, keyed by window label — the same key the
//! bridge uses for sessions, and the same key `identity.ts` files a cat's name
//! and coat under, so everything about one cat stays in step.
//!
//! Nothing in here is load-bearing for the app running: a missing, unreadable
//! or outdated file just means a cat with no past, which is exactly what the
//! first launch is.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Transcript lines kept per cat: enough to recognise the conversation you were
/// in the middle of, not so many that this becomes a chat log.
const MAX_ENTRIES: usize = 80;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Everything one cat carries across a restart.
///
/// The frontend owns the meaning of most of these; Rust only stores them, caps
/// the transcript, and keeps `born_at` honest. `entries` stays an opaque
/// `Value` so the chat's shape can change without touching this file.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CatMemory {
    /// What this cat is called. Minted and changed in `identity.ts`, which
    /// every cat window shares so siblings don't end up with the same name;
    /// kept here too because the agent runs as this cat, by name.
    pub name: String,
    /// Whether this cat was on the desktop when Purrch last closed, and so
    /// should be brought back. Cleared when you send one home.
    pub present: bool,
    /// First time this cat was ever seen, in ms since the epoch.
    pub born_at: i64,
    /// Last time it did anything. Used to decide whether a cat you're coming
    /// back to should be found awake or asleep.
    pub last_seen: i64,
    /// Window position in physical pixels, where it was last standing.
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub asleep: bool,
    pub pets: u64,
    pub naps: u64,
    pub plays: u64,
    /// Turns the agent has completed for you.
    pub turns: u64,
    /// Tools it has picked up doing them.
    pub tools: u64,
    /// Which backend it was last thinking with.
    pub backend: Option<String>,
    pub cwd: Option<String>,
    /// Agent session id, so the conversation resumes rather than restarts.
    pub session: Option<String>,
    pub entries: Vec<Value>,
}

impl CatMemory {
    /// The spot it was last standing on, if it has ever stood anywhere.
    pub fn at(&self) -> Option<(f64, f64)> {
        Some((self.x?, self.y?))
    }
}

/// Applies a patch from the frontend on top of what's already remembered.
///
/// Shallow, key by key, so the cat's two writers — the animation loop with the
/// position and mood, the chat with the session and transcript — can each send
/// only their own fields without clobbering the other's.
fn merge(cat: &CatMemory, patch: &Value) -> Result<CatMemory, String> {
    let Some(patch) = patch.as_object() else {
        return Err("a memory patch has to be an object".into());
    };
    let Ok(Value::Object(mut base)) = serde_json::to_value(cat) else {
        return Err("couldn't read the cat's memory".into());
    };
    for (key, value) in patch {
        base.insert(key.clone(), value.clone());
    }

    let mut next: CatMemory =
        serde_json::from_value(Value::Object(base)).map_err(|e| e.to_string())?;
    if next.entries.len() > MAX_ENTRIES {
        next.entries.drain(..next.entries.len() - MAX_ENTRIES);
    }
    // A cat's age is ours to keep, not the frontend's to overwrite.
    next.born_at = cat.born_at;
    next.last_seen = now_ms();
    Ok(next)
}

fn write(path: &Path, cats: &BTreeMap<String, CatMemory>) {
    let Ok(json) = serde_json::to_string_pretty(cats) else {
        return;
    };
    // Through a temporary first: a half-written file after a power cut would
    // cost the user every cat they have.
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, json).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

/// The colony's memory, shared by every cat window.
pub struct Memory {
    path: PathBuf,
    cats: Mutex<BTreeMap<String, CatMemory>>,
}

impl Memory {
    /// Reads the store, or starts an empty one. An unreadable file is treated
    /// as no file at all — a forgetful cat beats an app that won't open.
    pub fn load(dir: &Path) -> Self {
        let _ = fs::create_dir_all(dir);
        let path = dir.join("cats.json");
        let cats = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<BTreeMap<String, CatMemory>>(&raw).ok())
            .unwrap_or_default();
        Memory {
            path,
            cats: Mutex::new(cats),
        }
    }

    /// Every cat that was on the desktop when Purrch last closed.
    pub fn present(&self) -> Vec<String> {
        self.cats
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, cat)| cat.present)
            .map(|(label, _)| label.clone())
            .collect()
    }

    /// What `label` remembers, marking it as on the desktop. A label seen for
    /// the first time is a kitten: dated and written down on the spot, so the
    /// next launch brings it back even if nothing else ever happens to it.
    pub fn recall(&self, label: &str) -> CatMemory {
        let mut cats = self.cats.lock().unwrap();
        let cat = cats.entry(label.to_string()).or_insert_with(|| {
            let now = now_ms();
            CatMemory {
                born_at: now,
                last_seen: now,
                ..Default::default()
            }
        });
        cat.present = true;
        let cat = cat.clone();
        write(&self.path, &cats);
        cat
    }

    /// What `label` remembers, without claiming it's on the desktop.
    ///
    /// [`recall`](Self::recall) is a cat opening its eyes: it marks the cat
    /// present and dates a new one. A chore firing needs to know which backend
    /// and which name to run as, and asking that must not bring a cat back from
    /// the dead — so it reads and nothing else.
    pub fn peek(&self, label: &str) -> Option<CatMemory> {
        self.cats.lock().unwrap().get(label).cloned()
    }

    pub fn remember(&self, label: &str, patch: &Value) -> Result<(), String> {
        let mut cats = self.cats.lock().unwrap();
        // Writing before ever reading shouldn't cost a cat its birthday.
        let cat = cats.get(label).cloned().unwrap_or(CatMemory {
            present: true,
            born_at: now_ms(),
            ..Default::default()
        });
        let next = merge(&cat, patch)?;
        cats.insert(label.to_string(), next);
        write(&self.path, &cats);
        Ok(())
    }

    /// A cat sent home stops being restored at launch, but keeps what it knows.
    /// Window labels are handed back out (see `spawn_cat`), and `identity.ts`
    /// treats that slot as still belonging to the same cat — so a cat that
    /// comes back to a slot should come back to its own life, not a blank one.
    pub fn leave(&self, label: &str) {
        let mut cats = self.cats.lock().unwrap();
        let Some(cat) = cats.get_mut(label) else { return };
        cat.present = false;
        cat.last_seen = now_ms();
        write(&self.path, &cats);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("purrch-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_new_cat_is_dated_and_on_the_desktop() {
        let mem = Memory::load(&scratch());
        let cat = mem.recall("main");
        assert!(cat.born_at > 0);
        assert!(cat.present);
        // Recalling again is the same cat, not a new one.
        assert_eq!(mem.recall("main").born_at, cat.born_at);
    }

    #[test]
    fn patches_from_two_writers_dont_clobber_each_other() {
        let mem = Memory::load(&scratch());
        mem.recall("main");

        // The animation loop reports where it's standing...
        mem.remember("main", &json!({ "x": 100.0, "y": 900.0, "pets": 3 }))
            .unwrap();
        // ...and the chat reports the conversation, knowing nothing about it.
        mem.remember("main", &json!({ "session": "abc", "turns": 2 }))
            .unwrap();

        let cat = mem.recall("main");
        assert_eq!(cat.x, Some(100.0));
        assert_eq!(cat.pets, 3);
        assert_eq!(cat.session.as_deref(), Some("abc"));
        assert_eq!(cat.turns, 2);
    }

    #[test]
    fn the_transcript_is_capped_and_keeps_the_newest() {
        let mem = Memory::load(&scratch());
        let entries: Vec<Value> = (0..MAX_ENTRIES + 20).map(|i| json!({ "n": i })).collect();
        mem.remember("main", &json!({ "entries": entries })).unwrap();

        let kept = mem.recall("main").entries;
        assert_eq!(kept.len(), MAX_ENTRIES);
        assert_eq!(kept[0]["n"], json!(20));
    }

    #[test]
    fn age_survives_a_frontend_that_says_otherwise() {
        let mem = Memory::load(&scratch());
        let born = mem.recall("main").born_at;
        mem.remember("main", &json!({ "bornAt": 0 })).unwrap();
        assert_eq!(mem.recall("main").born_at, born);
    }

    #[test]
    fn memories_outlive_the_process() {
        let dir = scratch();
        {
            let mem = Memory::load(&dir);
            mem.recall("main");
            mem.remember("main", &json!({ "x": 42.0, "asleep": true }))
                .unwrap();
        }
        let mem = Memory::load(&dir);
        assert_eq!(mem.present(), vec!["main".to_string()]);
        let cat = mem.recall("main");
        assert_eq!(cat.x, Some(42.0));
        assert!(cat.asleep);
    }

    #[test]
    fn a_cat_sent_home_stays_home_but_keeps_its_life() {
        let dir = scratch();
        let mem = Memory::load(&dir);
        mem.recall("main");
        mem.recall("cat-1");
        mem.remember("cat-1", &json!({ "pets": 9 })).unwrap();
        mem.leave("cat-1");

        // Not brought back at the next launch...
        assert_eq!(Memory::load(&dir).present(), vec!["main".to_string()]);
        // ...but if that slot takes a cat again, it's the same cat.
        assert_eq!(Memory::load(&dir).recall("cat-1").pets, 9);
    }

    #[test]
    fn peeking_doesnt_bring_a_cat_back_from_the_dead() {
        let dir = scratch();
        let mem = Memory::load(&dir);
        mem.recall("cat-1");
        mem.leave("cat-1");

        assert!(mem.peek("cat-1").is_some());
        assert!(mem.present().is_empty(), "a peek put the cat back on the desktop");
        // ...and a cat that never existed isn't invented by asking about it.
        assert!(mem.peek("cat-9").is_none());
        assert!(Memory::load(&dir).peek("cat-9").is_none());
    }

    #[test]
    fn a_corrupt_store_costs_the_cats_but_not_the_app() {
        let dir = scratch();
        fs::write(dir.join("cats.json"), "{ this is not json").unwrap();
        let mem = Memory::load(&dir);
        assert!(mem.present().is_empty());
        assert!(mem.recall("main").born_at > 0);
    }

    #[test]
    fn a_patch_that_isnt_an_object_is_refused() {
        let mem = Memory::load(&scratch());
        assert!(mem.remember("main", &json!("hello")).is_err());
    }
}
