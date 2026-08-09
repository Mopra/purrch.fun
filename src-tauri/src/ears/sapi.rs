//! The ear itself: Windows' own speech recognizer, driven from PowerShell.
//!
//! Purrch shells out for its brains already, so it shells out for its hearing
//! too. `System.Speech` ships with every Windows install and its desktop
//! recognizer runs entirely offline — no model to download, no audio leaving
//! the machine, and nothing to build beyond what the app already needs.
//!
//! The trick that makes always-on listening bearable is the grammar. Rather
//! than transcribing the room and looking for a cat's name in the text, the
//! recognizer is only ever given one shape of sentence: *a wake phrase,
//! followed by free dictation*. Speech that doesn't open with a cat's name
//! doesn't match anything, so it is rejected by the engine before it ever
//! reaches us — the cats genuinely cannot hear a conversation they weren't
//! addressed in.
//!
//! What it is *not* good at is writing down the rest of the sentence, so it
//! doesn't have to: every match also hands back the audio it recorded, and
//! [`whisper`](super::whisper) turns that into the actual command. See there
//! for why the work is split this way.

use super::{shell, Listener};
use serde::Deserialize;
use tokio::process::{Child, Command};

/// One line of the listener's stdout.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Word {
    /// The engine is up and the mic is open.
    Listening,
    /// It never got that far, or it fell over. Carries something sayable.
    Deaf { message: String },
    /// A guess at an utterance still being spoken.
    Partial { text: String },
    /// A finished utterance that matched the grammar.
    Heard {
        text: String,
        #[serde(default)]
        confidence: f64,
        /// A WAV of what was actually said, for a better transcriber to read.
        /// Absent if the engine didn't keep the audio; the caller then has
        /// nothing but SAPI's own guess at the words, and deletes nothing.
        #[serde(default)]
        audio: Option<String>,
    },
    /// Speech that didn't match — somebody talking near an open mic.
    Missed,
}

pub fn parse(line: &str) -> Option<Word> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    serde_json::from_str(line).ok()
}

/// Lead-ins a cat answers to, longest first so `hey tofu` isn't matched as a
/// bare `hey` by a cat unfortunate enough to be called that.
///
/// The empty lead is what makes "Tofu, open my mail" work as well as "Hey
/// Tofu". It's last because a name on its own is the weakest claim of the set.
pub const LEADS: &[&str] = &["hey", "hi", "okay", "ok", "yo", ""];

/// Trims a cat's name into something a speech grammar can hold.
///
/// Grammar phrases are words, not text: punctuation has no pronunciation and
/// makes the engine refuse the whole grammar rather than skip the phrase. A
/// name that survives this as nothing at all is a cat that can't be called,
/// which is a quiet cat rather than a broken one.
pub fn sayable(name: &str) -> String {
    let loose: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect();
    loose.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every phrase the grammar will open on, for the whole colony.
fn phrases(roster: &[Listener]) -> Vec<String> {
    let mut out = Vec::new();
    for cat in roster {
        let name = sayable(&cat.name);
        if name.is_empty() {
            continue;
        }
        for lead in LEADS {
            let phrase = if lead.is_empty() {
                name.clone()
            } else {
                format!("{lead} {name}")
            };
            if !out.contains(&phrase) {
                out.push(phrase);
            }
        }
    }
    out
}

/// The listener. Emits one JSON object per line and never returns on its own.
const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
$out = [Console]::Out

function Say($o) {
  $out.WriteLine(($o | ConvertTo-Json -Compress -Depth 3))
  $out.Flush()
}

function Fail($why) {
  Say @{ kind = 'deaf'; message = [string]$why }
  exit 1
}

try { Add-Type -AssemblyName System.Speech } catch { Fail 'this Windows has no speech engine installed' }

$all = [System.Speech.Recognition.SpeechRecognitionEngine]::InstalledRecognizers()
if (-not $all -or @($all).Count -eq 0) { Fail 'no speech recognizer is installed on this Windows' }
$info = @($all) | Where-Object { $_.Culture.TwoLetterISOLanguageName -eq 'en' } | Select-Object -First 1
if (-not $info) { $info = @($all)[0] }

try {
  $rec = New-Object System.Speech.Recognition.SpeechRecognitionEngine $info
} catch { Fail $_.Exception.Message }

# One sentence shape only: a cat's name, then whatever you want it to do.
$choices = New-Object System.Speech.Recognition.Choices
foreach ($p in __PHRASES__) { $choices.Add([string]$p) }
$gb = New-Object System.Speech.Recognition.GrammarBuilder
$gb.Culture = $info.Culture
$gb.Append($choices)
$gb.AppendDictation()
try {
  $grammar = New-Object System.Speech.Recognition.Grammar $gb
  $grammar.Name = 'purrch'
  $rec.LoadGrammar($grammar)
} catch { Fail "couldn't teach the cats their names: $($_.Exception.Message)" }

try { $rec.SetInputToDefaultAudioDevice() } catch { Fail 'no microphone Purrch is allowed to listen through' }

# Recordings of the user's voice, waiting to be transcribed. They are deleted
# the moment they have been read, but a crash between the two would leave one
# behind, so anything older than an hour goes now.
$scratch = Join-Path $env:TEMP 'purrch-heard'
New-Item -ItemType Directory -Force -Path $scratch | Out-Null
Get-ChildItem -Path $scratch -Filter '*.wav' -File -ErrorAction SilentlyContinue |
  Where-Object { $_.LastWriteTime -lt (Get-Date).AddHours(-1) } |
  Remove-Item -Force -ErrorAction SilentlyContinue

# Keeps what was said, so something better at English can read it back.
function Save-Utterance($result) {
  if (-not $result.Audio) { return $null }
  try {
    $path = Join-Path $scratch ([guid]::NewGuid().ToString('N') + '.wav')
    $fs = [System.IO.File]::Create($path)
    $result.Audio.WriteToWaveStream($fs)
    $fs.Close()
    return $path
  } catch {
    # Losing the recording only costs accuracy: SAPI's own guess still goes.
    return $null
  }
}

# Zero means "no timeout" — the mic stays open for as long as Purrch is running.
$rec.InitialSilenceTimeout = [TimeSpan]::Zero
$rec.BabbleTimeout = [TimeSpan]::Zero
$rec.EndSilenceTimeout = [TimeSpan]::FromMilliseconds(650)
$rec.EndSilenceTimeoutAmbiguous = [TimeSpan]::FromMilliseconds(900)

# Handlers have to come off the event queue rather than run as script blocks:
# the recognizer raises them on its own thread, where this runspace isn't.
$null = Register-ObjectEvent -InputObject $rec -EventName SpeechRecognized -SourceIdentifier heard
$null = Register-ObjectEvent -InputObject $rec -EventName SpeechHypothesized -SourceIdentifier partial
$null = Register-ObjectEvent -InputObject $rec -EventName SpeechRecognitionRejected -SourceIdentifier missed

try {
  $rec.RecognizeAsync([System.Speech.Recognition.RecognizeMode]::Multiple)
} catch { Fail $_.Exception.Message }

Say @{ kind = 'listening' }

# Nothing kills a child process on Windows just because its parent died, and a
# listener that outlives Purrch is a microphone nobody can see holding itself
# open. So it watches: every couple of quiet seconds it checks that the app is
# still there, which covers being closed, crashing, and being killed outright.
$parent = 0
if ($env:PURRCH_PARENT) { $parent = [int]$env:PURRCH_PARENT }

try {
  while ($true) {
    $evt = Wait-Event -Timeout 2
    if (-not $evt) {
      if ($parent -gt 0 -and -not (Get-Process -Id $parent -ErrorAction SilentlyContinue)) { break }
      continue
    }
    $res = $evt.SourceEventArgs.Result
    switch ($evt.SourceIdentifier) {
      'heard'   { if ($res) { Say @{ kind = 'heard';   text = [string]$res.Text; confidence = [double]$res.Confidence; audio = (Save-Utterance $res) } } }
      'partial' { if ($res) { Say @{ kind = 'partial'; text = [string]$res.Text } } }
      'missed'  { Say @{ kind = 'missed' } }
    }
    Remove-Event -EventIdentifier $evt.EventIdentifier
  }
} catch { Fail $_.Exception.Message }

try { $rec.RecognizeAsyncStop() } catch { }
$rec.Dispose()
"#;

/// Starts a listener for everyone on `roster`. Its stdout is the caller's to
/// drain; dropping the child stops the mic.
pub fn spawn(roster: &[Listener]) -> Result<Child, String> {
    watching(roster, std::process::id())
}

/// As [`spawn`], but told which process to hang its life on — so the "let go
/// when Purrch is gone" half can be tested against a process that really is.
fn watching(roster: &[Listener], parent: u32) -> Result<Child, String> {
    let script = SCRIPT.replace("__PHRASES__", &shell::array(&phrases(roster)));

    let mut cmd: Command = shell::command(&script);
    // How the listener knows Purrch is still alive. `kill_on_drop` only fires
    // if this process gets to run its destructors, which a crash or a Task
    // Manager kill doesn't — and the microphone must not outlive the app
    // either way.
    cmd.env("PURRCH_PARENT", parent.to_string());

    cmd.spawn()
        .map_err(|e| format!("couldn't start listening: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(name: &str) -> Listener {
        Listener {
            label: "main".into(),
            name: name.into(),
        }
    }

    #[test]
    fn a_name_becomes_something_pronounceable() {
        assert_eq!(sayable("Tofu"), "tofu");
        assert_eq!(sayable("Mr. Whiskers!"), "mr whiskers");
        assert_eq!(sayable("  Bean   Jr  "), "bean jr");
        // Nothing left to say is a cat that simply can't be called by voice.
        assert_eq!(sayable("🐱"), "");
    }

    #[test]
    fn every_lead_in_gets_a_phrase_and_the_bare_name_is_one_of_them() {
        let all = phrases(&[cat("Tofu")]);
        assert!(all.contains(&"hey tofu".to_string()));
        assert!(all.contains(&"tofu".to_string()));
        assert_eq!(all.len(), LEADS.len());
    }

    #[test]
    fn an_unsayable_cat_contributes_nothing_to_the_grammar() {
        assert!(phrases(&[cat("...")]).is_empty());
        // ...and doesn't stop its siblings being heard.
        assert_eq!(phrases(&[cat("..."), cat("Tofu")]).len(), LEADS.len());
    }

    #[test]
    fn two_cats_with_one_name_are_listened_for_once() {
        assert_eq!(phrases(&[cat("Tofu"), cat("tofu")]).len(), LEADS.len());
    }

    #[test]
    fn the_script_carries_the_colony_and_nothing_else_changes() {
        let script = SCRIPT.replace("__PHRASES__", &shell::array(&phrases(&[cat("Tofu")])));
        assert!(script.contains("@('hey tofu',"));
        assert!(!script.contains("__PHRASES__"));
        // A name is the one piece of user-written text that reaches the
        // script, and it goes in as a quoted literal or it goes in as code.
        let risky = SCRIPT.replace("__PHRASES__", &shell::array(&phrases(&[cat("o'malley")])));
        assert!(risky.contains("'hey o''malley'"));
    }

    /// The one thing the rest of this module can't tell you: whether any of it
    /// actually starts. Spawns the real listener, on the real speech engine,
    /// against the real microphone, and waits for it to say it's open.
    ///
    /// Ignored by default because it needs a machine with a microphone the user
    /// has let desktop apps use — a CI box has neither, and "no microphone" is
    /// a legitimate answer there rather than a failure. Run it by hand with
    /// `cargo test -- --ignored` after touching the script.
    #[tokio::test]
    #[ignore = "needs a microphone"]
    async fn the_listener_really_opens_a_microphone() {
        use tokio::io::{AsyncBufReadExt, BufReader};

        let mut child = spawn(&[cat("Tofu")]).expect("couldn't spawn the listener");
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();

        let first = tokio::time::timeout(std::time::Duration::from_secs(30), lines.next_line())
            .await
            .expect("the listener said nothing for 30s")
            .expect("stdout died")
            .expect("stdout closed without a word");

        let _ = child.kill().await;
        match parse(&first) {
            Some(Word::Listening) => {}
            Some(Word::Deaf { message }) => panic!("the listener gave up: {message}"),
            other => panic!("unexpected first word: {other:?} from {first:?}"),
        }
    }

    /// The other half of that: nothing is holding the microphone once Purrch
    /// isn't there. Watches a process that has already exited, so the listener
    /// should come up, notice, and let go on its own.
    #[tokio::test]
    #[ignore = "needs a microphone"]
    async fn the_listener_lets_go_when_purrch_is_gone() {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, BufReader};

        // A pid that is definitely not running any more.
        let mut corpse = Command::new(shell::powershell())
            .arg("-NoProfile")
            .arg("-Command")
            .arg("exit")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("couldn't spawn a process to bury");
        let buried = corpse.id().expect("no pid");
        let _ = corpse.wait().await;

        let mut child = watching(&[cat("Tofu")], buried).expect("couldn't spawn the listener");
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();

        // It gets as far as opening the mic...
        let first = tokio::time::timeout(std::time::Duration::from_secs(30), lines.next_line())
            .await
            .expect("the listener said nothing for 30s")
            .expect("stdout died");
        assert!(matches!(
            first.as_deref().and_then(parse),
            Some(Word::Listening)
        ));

        // ...and then closes it again without being asked, well inside the
        // couple of seconds it waits between checks.
        let ended = tokio::time::timeout(std::time::Duration::from_secs(15), child.wait())
            .await
            .expect("the listener kept the microphone after Purrch was gone");
        assert!(ended.is_ok());
    }

    #[test]
    fn lines_of_stdout_become_words() {
        assert!(matches!(
            parse(r#"{"kind":"listening"}"#),
            Some(Word::Listening)
        ));
        assert!(matches!(
            parse(r#"{"kind":"heard","text":"hey tofu hello","confidence":0.9}"#),
            Some(Word::Heard { .. })
        ));
        // Confidence is optional so a malformed line still routes as speech.
        assert!(matches!(
            parse(r#"{"kind":"heard","text":"hey tofu hello"}"#),
            Some(Word::Heard { confidence, .. }) if confidence == 0.0
        ));
        assert!(parse("").is_none());
        assert!(parse("not json at all").is_none());
        assert!(parse(r#"{"kind":"purring"}"#).is_none());
    }
}
