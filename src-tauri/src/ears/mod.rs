//! The colony's hearing.
//!
//! One microphone, one recogniser, however many cats. A cat is called by name
//! — "Tofu, open my mail" — so the ear belongs to the colony rather than to any
//! one of them, and its only real job is deciding whose window an utterance
//! belongs to before it goes anywhere near an agent.
//!
//! Hearing is split across two engines that are good at opposite things:
//!
//! - [`sapi`] holds the microphone. Windows' own recogniser, given a fixed
//!   grammar of "a cat's name, then some words", is very good at picking a name
//!   out of a room and rejecting everything else. It never gets a chance to
//!   mishear a conversation it wasn't part of.
//! - [`whisper`] reads back what was actually said. SAPI's own dictation is
//!   from the Windows 7 era and produces confident nonsense on anything with a
//!   proper noun in it; whisper.cpp does not.
//!
//! This file is the part worth reading: who is being listened for, and how what
//! comes back is routed.
//!
//! Three things it deliberately does not do. It never listens for a cat whose
//! window hasn't asked to be listened for — the frontend owns that switch,
//! because it is also where the user's one-time agreement lives, and a cat must
//! not have its ears open before that is given. A deliberate retune is silent:
//! the old listener is stopped without announcing deafness, because a new one
//! is already on its way and a flicker of "no microphone" in between would read
//! as a fault. And a recording of the user's voice never outlives the turn it
//! was made for.

pub mod sapi;
pub mod shell;
pub mod whisper;

use sapi::{sayable, Word, LEADS};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Notify;

/// Channel the frontend listens on for every [`EarEvent`].
pub const EVENT: &str = "purrch://ears";

/// How sure the grammar has to be that it heard a cat's name.
///
/// Deliberately low, and it is only ever a judgement about the *wake phrase* —
/// the words after it are whisper's business, not SAPI's. Everything not
/// addressed to a cat has already been thrown out by the grammar, so what
/// survives to here is nearly always meant for one, and a command dropped for
/// being mumbled is a cat that ignores you.
const SURE_ENOUGH: f64 = 0.3;

/// One cat the ear is listening for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listener {
    /// Window label — where anything heard for this cat gets sent.
    pub label: String,
    pub name: String,
}

/// What a cat's window hears back.
///
/// [`Listening`](EarEvent::Listening), [`Deaf`](EarEvent::Deaf) and
/// [`Learning`](EarEvent::Learning) are about the ear itself and go to the
/// whole colony; the rest are addressed to one cat, because a sibling must
/// never react to being called by somebody else's name.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EarEvent {
    /// The mic is open and the colony is being listened for.
    Listening,
    /// It isn't, and here's something sayable about why.
    Deaf { message: String },
    /// Off getting better at English. Says so because it is a large download
    /// and the cats are noticeably worse at listening until it lands.
    Learning { message: String },
    /// This cat's name has just been said — it's still being spoken to.
    Perked,
    /// Whatever was said after the name. Empty means it was only called.
    Heard { text: String },
    /// It was addressed, then the rest didn't come out as anything.
    Missed,
}

/// If `text` opens by addressing `name`, everything said after that.
///
/// Matching is on words rather than characters: both recognisers punctuate,
/// and "Tofu, open my mail" has to land the same as "tofu open my mail". The
/// command keeps its original spelling — only the wake phrase is normalised,
/// because that's the only part being compared against anything.
fn addressed(text: &str, name: &str) -> Option<String> {
    let name = sayable(name);
    if name.is_empty() {
        return None;
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    let spoken: Vec<String> = words.iter().map(|w| sayable(w)).collect();

    for lead in LEADS {
        let wake: Vec<&str> = lead
            .split_whitespace()
            .chain(name.split_whitespace())
            .collect();
        if spoken.len() < wake.len() {
            continue;
        }
        if wake.iter().zip(&spoken).all(|(want, got)| *want == got) {
            return Some(words[wake.len()..].join(" "));
        }
    }
    None
}

/// Splits an utterance into the cat it was aimed at and what it was asked to do.
fn hear(text: &str, roster: &[Listener]) -> Option<(String, String)> {
    // Longest name first, so a cat called "Mr Whiskers" isn't answered for by
    // a sibling called "Mr".
    let mut cats: Vec<&Listener> = roster.iter().collect();
    cats.sort_by_key(|c| std::cmp::Reverse(sayable(&c.name).split_whitespace().count()));

    cats.into_iter()
        .find_map(|cat| addressed(text, &cat.name).map(|said| (cat.label.clone(), said)))
}

/// Takes the wake phrase off a transcript that didn't come from the grammar.
///
/// whisper spells names its own way, so this has to fail softly: a transcript
/// we can't find the wake phrase in is still the command. The cat's persona
/// already knows what it's called, so a stray "Hey Tofu," left on the front
/// costs nothing, where dropping the sentence would cost everything.
fn undress(text: &str, name: &str) -> String {
    addressed(text, name).unwrap_or_else(|| text.trim().to_string())
}

/// What one cat's window wants of the ear.
struct Wish {
    name: String,
    listening: bool,
}

#[derive(Default)]
struct State {
    wishes: BTreeMap<String, Wish>,
    /// The roster the listener currently running was built for. Compared
    /// against on every change so the mic is only reopened when the colony
    /// actually sounds different — cats rename rarely, and the frontend
    /// reports its wishes far more often than that.
    tuned: Vec<Listener>,
    stop: Option<Arc<Notify>>,
}

/// Who is being listened for, and the processes doing it.
pub struct Ears {
    state: Mutex<State>,
    /// Where the transcriber and its model live, once fetched.
    dir: PathBuf,
    /// Set while that fetch is running, so a second cat opening its ears
    /// doesn't start a second download of the same 150 MB.
    learning: Arc<AtomicBool>,
}

impl Ears {
    pub fn new(dir: PathBuf) -> Self {
        Ears {
            state: Mutex::new(State::default()),
            dir,
            learning: Arc::new(AtomicBool::new(false)),
        }
    }

    /// A cat says what it's called and whether it wants its ears open.
    pub fn tune(&self, app: &AppHandle, label: &str, name: String, listening: bool) {
        let mut state = self.state.lock().unwrap();
        state
            .wishes
            .insert(label.to_string(), Wish { name, listening });
        self.settle(app, &mut state);
    }

    /// A cat's window has gone. Its name must stop being answered for.
    pub fn forget(&self, app: &AppHandle, label: &str) {
        let mut state = self.state.lock().unwrap();
        if state.wishes.remove(label).is_none() {
            return;
        }
        self.settle(app, &mut state);
    }

    /// Brings the running listener into line with what the colony is asking for.
    fn settle(&self, app: &AppHandle, state: &mut State) {
        let next = roster(&state.wishes);
        if next == state.tuned {
            return;
        }
        // Silent on purpose — see the module note.
        if let Some(stop) = state.stop.take() {
            stop.notify_one();
        }
        state.tuned = next.clone();
        if next.is_empty() {
            return;
        }

        // Somebody is listening now, so it's worth going and getting the half
        // of the ear that can actually spell.
        self.study(app);

        let stop = Arc::new(Notify::new());
        state.stop = Some(stop.clone());
        open(app.clone(), self.dir.clone(), next, stop);
    }

    /// Fetches the transcriber, once, in the background. Voice works without
    /// it — worse — so nothing here blocks and nothing here fails loudly.
    fn study(&self, app: &AppHandle) {
        if whisper::ready(&self.dir) {
            return;
        }
        if self.learning.swap(true, Ordering::SeqCst) {
            return;
        }

        let app = app.clone();
        let dir = self.dir.clone();
        let learning = self.learning.clone();
        tauri::async_runtime::spawn(async move {
            let mut child = match whisper::fetch(&dir) {
                Ok(child) => child,
                Err(message) => {
                    learning.store(false, Ordering::SeqCst);
                    return broadcast(&app, &EarEvent::Learning { message });
                }
            };

            if let Some(stdout) = child.stdout.take() {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    match whisper::parse(&line) {
                        Some(whisper::Fetched::Fetching { what }) => broadcast(
                            &app,
                            &EarEvent::Learning {
                                message: format!(
                                    "getting better at listening ({what}, {})",
                                    whisper::size_note()
                                ),
                            },
                        ),
                        Some(whisper::Fetched::Ready) => broadcast(
                            &app,
                            &EarEvent::Learning {
                                message: "that's better — say it again?".into(),
                            },
                        ),
                        Some(whisper::Fetched::Failed { message }) => broadcast(
                            &app,
                            &EarEvent::Learning {
                                message: format!("couldn't get better at listening: {message}"),
                            },
                        ),
                        None => {}
                    }
                }
            }
            let _ = child.wait().await;
            learning.store(false, Ordering::SeqCst);
        });
    }
}

/// Everyone whose ears are open, in a stable order so two identical colonies
/// compare equal and don't reopen the microphone for nothing.
fn roster(wishes: &BTreeMap<String, Wish>) -> Vec<Listener> {
    wishes
        .iter()
        .filter(|(_, wish)| wish.listening && !sayable(&wish.name).is_empty())
        .map(|(label, wish)| Listener {
            label: label.clone(),
            name: wish.name.clone(),
        })
        .collect()
}

fn broadcast(app: &AppHandle, event: &EarEvent) {
    let _ = app.emit(EVENT, event);
}

/// Turns one recorded utterance into a command and sends it to its cat.
///
/// Runs off the listener's own loop: transcribing takes about a second, and the
/// microphone must not go deaf for that second. SAPI's guess at the words is
/// the fallback, so a cat still does *something* when the transcriber isn't
/// there yet or falls over.
fn understand(
    app: AppHandle,
    dir: PathBuf,
    label: String,
    name: String,
    fallback: String,
    wav: Option<String>,
) {
    tauri::async_runtime::spawn(async move {
        let mut text = fallback;

        if let Some(wav) = wav {
            let wav = PathBuf::from(wav);
            if whisper::ready(&dir) {
                match whisper::transcribe(&dir, &wav).await {
                    Ok(said) => {
                        let command = undress(&said, &name);
                        if !command.is_empty() {
                            text = command;
                        }
                    }
                    // Nothing the user can do about it, and they still have a
                    // working — if dimmer — cat.
                    Err(why) => eprintln!("purrch: couldn't transcribe: {why}"),
                }
            }
            // A recording of the user's voice has no business outliving the
            // turn it was made for.
            let _ = std::fs::remove_file(&wav);
        }

        let _ = app.emit_to(&label, EVENT, EarEvent::Heard { text });
    });
}

/// Runs one listener until it's stopped or it dies.
fn open(app: AppHandle, dir: PathBuf, roster: Vec<Listener>, stop: Arc<Notify>) {
    tauri::async_runtime::spawn(async move {
        let mut child = match sapi::spawn(&roster) {
            Ok(child) => child,
            Err(message) => return broadcast(&app, &EarEvent::Deaf { message }),
        };

        let Some(stdout) = child.stdout.take() else {
            return broadcast(
                &app,
                &EarEvent::Deaf {
                    message: "the listener started but said nothing".into(),
                },
            );
        };

        // Drained concurrently: a full stderr pipe would block the child, and
        // a blocked child is a cat that stops hearing you without saying so.
        let stderr = child.stderr.take();
        let noise = tauri::async_runtime::spawn(async move {
            let mut buf = String::new();
            if let Some(stderr) = stderr {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    buf.push_str(&line);
                    buf.push('\n');
                }
            }
            buf
        });

        let mut lines = BufReader::new(stdout).lines();
        // Which cat has already sat up for the utterance being spoken now, so
        // a stream of guesses at the same sentence perks it once.
        let mut perked: Option<String> = None;
        let mut said_why = false;

        loop {
            tokio::select! {
                biased;

                line = lines.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    let Some(word) = sapi::parse(&line) else { continue };
                    match word {
                        Word::Listening => broadcast(&app, &EarEvent::Listening),
                        Word::Deaf { message } => {
                            said_why = true;
                            broadcast(&app, &EarEvent::Deaf { message });
                        }
                        Word::Partial { text } => {
                            if let Some((label, _)) = hear(&text, &roster) {
                                if perked.as_deref() != Some(label.as_str()) {
                                    let _ = app.emit_to(&label, EVENT, EarEvent::Perked);
                                    perked = Some(label);
                                }
                            }
                        }
                        Word::Heard { text, confidence, audio } => {
                            perked = None;
                            if confidence < SURE_ENOUGH {
                                if let Some(wav) = audio { let _ = std::fs::remove_file(wav); }
                                continue;
                            }
                            match hear(&text, &roster) {
                                Some((label, fallback)) => {
                                    let name = roster
                                        .iter()
                                        .find(|c| c.label == label)
                                        .map(|c| c.name.clone())
                                        .unwrap_or_default();
                                    understand(
                                        app.clone(),
                                        dir.clone(),
                                        label,
                                        name,
                                        fallback,
                                        audio,
                                    );
                                }
                                // The grammar matched but we can't say which
                                // cat: nobody is told, and nothing is kept.
                                None => {
                                    if let Some(wav) = audio { let _ = std::fs::remove_file(wav); }
                                }
                            }
                        }
                        Word::Missed => {
                            if let Some(label) = perked.take() {
                                let _ = app.emit_to(&label, EVENT, EarEvent::Missed);
                            }
                        }
                    }
                }

                _ = stop.notified() => {
                    let _ = child.kill().await;
                    return;
                }
            }
        }

        // Out of the loop without being asked: the listener is gone.
        let _ = child.wait().await;
        if said_why {
            return;
        }
        let why = noise.await.unwrap_or_default();
        let why = why.trim();
        let message = if why.is_empty() {
            "stopped listening unexpectedly".to_string()
        } else {
            let tail: String = why.chars().rev().take(300).collect();
            tail.chars().rev().collect()
        };
        broadcast(&app, &EarEvent::Deaf { message });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colony() -> Vec<Listener> {
        vec![
            Listener {
                label: "main".into(),
                name: "Tofu".into(),
            },
            Listener {
                label: "cat-1".into(),
                name: "Mochi".into(),
            },
        ]
    }

    fn heard(text: &str) -> Option<(String, String)> {
        hear(text, &colony())
    }

    #[test]
    fn a_cat_is_called_by_name_and_handed_the_rest() {
        assert_eq!(
            heard("hey tofu lets play league of legends"),
            Some(("main".into(), "lets play league of legends".into()))
        );
        assert_eq!(
            heard("mochi commit my changes"),
            Some(("cat-1".into(), "commit my changes".into()))
        );
    }

    #[test]
    fn punctuation_and_case_are_the_recognizers_business_not_ours() {
        assert_eq!(
            heard("Hey Tofu, open my mail."),
            Some(("main".into(), "open my mail.".into()))
        );
    }

    #[test]
    fn nobody_answers_to_a_name_that_wasnt_said() {
        assert_eq!(heard("open my mail"), None);
        // A cat's name in the middle of a sentence is somebody talking *about*
        // it, not to it.
        assert_eq!(heard("i think tofu is asleep"), None);
    }

    #[test]
    fn a_sibling_never_answers_for_another_cat() {
        let (label, _) = heard("hey mochi open notepad").unwrap();
        assert_eq!(label, "cat-1");
    }

    #[test]
    fn being_called_with_nothing_after_it_is_still_being_called() {
        assert_eq!(heard("hey tofu"), Some(("main".into(), String::new())));
    }

    #[test]
    fn the_longest_name_wins_over_a_sibling_hiding_inside_it() {
        let cats = vec![
            Listener {
                label: "short".into(),
                name: "Mr".into(),
            },
            Listener {
                label: "long".into(),
                name: "Mr Whiskers".into(),
            },
        ];
        assert_eq!(
            hear("hey mr whiskers open notepad", &cats),
            Some(("long".into(), "open notepad".into()))
        );
        // ...and the short one is still reachable on its own.
        assert_eq!(
            hear("hey mr open notepad", &cats),
            Some(("short".into(), "open notepad".into()))
        );
    }

    #[test]
    fn an_unsayable_cat_is_never_routed_to() {
        let cats = vec![Listener {
            label: "main".into(),
            name: "...".into(),
        }];
        assert_eq!(hear("open notepad", &cats), None);
    }

    /// What whisper hands back is a whole sentence including the name, spelled
    /// however it felt like spelling it.
    #[test]
    fn a_transcript_loses_the_wake_phrase_it_opens_with() {
        assert_eq!(
            undress("Hey Tofu, open the Riot Games launcher.", "Tofu"),
            "open the Riot Games launcher."
        );
        assert_eq!(undress("Tofu, stop", "Tofu"), "stop");
    }

    /// The important half: a name whisper heard differently must not cost the
    /// user their command.
    #[test]
    fn a_transcript_that_lost_the_name_is_still_the_command() {
        assert_eq!(
            undress("Hey Toby, open the launcher.", "Tofu"),
            "Hey Toby, open the launcher."
        );
        assert_eq!(undress("  open notepad  ", "Tofu"), "open notepad");
        // Called and nothing else, so there is nothing to do.
        assert_eq!(undress("Hey Tofu.", "Tofu"), "");
    }

    #[test]
    fn only_cats_that_asked_are_listened_for() {
        let mut wishes = BTreeMap::new();
        wishes.insert(
            "main".to_string(),
            Wish {
                name: "Tofu".into(),
                listening: true,
            },
        );
        wishes.insert(
            "cat-1".to_string(),
            Wish {
                name: "Mochi".into(),
                listening: false,
            },
        );
        // A cat with a name nobody can pronounce is left out too — it would
        // contribute no phrases, so listening for it means listening for
        // nothing.
        wishes.insert(
            "cat-2".to_string(),
            Wish {
                name: "  ".into(),
                listening: true,
            },
        );

        let open = roster(&wishes);
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].label, "main");
    }

    /// The microphone must only be reopened when the colony really changed:
    /// the frontend reports its wishes on every rename, coat change and
    /// restore, and reopening on each would stutter the ear.
    #[test]
    fn an_identical_colony_compares_equal() {
        let mut wishes = BTreeMap::new();
        wishes.insert(
            "cat-1".to_string(),
            Wish {
                name: "Mochi".into(),
                listening: true,
            },
        );
        wishes.insert(
            "main".to_string(),
            Wish {
                name: "Tofu".into(),
                listening: true,
            },
        );
        let first = roster(&wishes);

        // Same colony, described in the other order.
        let mut again = BTreeMap::new();
        again.insert(
            "main".to_string(),
            Wish {
                name: "Tofu".into(),
                listening: true,
            },
        );
        again.insert(
            "cat-1".to_string(),
            Wish {
                name: "Mochi".into(),
                listening: true,
            },
        );
        assert_eq!(first, roster(&again));

        // Renaming one of them is a different colony.
        again.get_mut("main").unwrap().name = "Biscuit".into();
        assert_ne!(first, roster(&again));
    }
}
