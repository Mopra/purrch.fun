//! The half of the ear that actually understands English.
//!
//! Windows' own recogniser ([`sapi`](super::sapi)) is excellent at one job and
//! hopeless at another. Given a fixed grammar — "one of these names, then some
//! words" — it reliably picks a cat's name out of a room and rejects
//! everything not addressed to one. Given open dictation it produces confident
//! nonsense: *"open the Riot Games launcher and queue for ARAM"* came back as
//! *"open the right games launch a rank euphoria room"*, at 0.95 confidence.
//! It is a fixed-lexicon engine from the Windows 7 era and no amount of tuning
//! moves it.
//!
//! So SAPI keeps the microphone and keeps the wake word, and hands the audio it
//! recorded to whisper.cpp, which gets that same sentence exactly right. The
//! two engines are good at opposite things and neither is asked to do the
//! other's job.
//!
//! whisper.cpp is fetched at first use rather than built in: compiling it needs
//! a C++ toolchain and libclang on whoever builds Purrch, and a pet shouldn't
//! cost its author a gigabyte of LLVM. The download is pinned to one release
//! and one checksum, and anything that doesn't match is thrown away — this
//! puts an executable on the user's machine, so "whatever GitHub serves today"
//! is not good enough.
//!
//! Until it has arrived, voice still works on SAPI alone. Worse, but working.

use super::shell;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

/// Pinned release of the transcriber, with the checksum of that exact zip.
const ENGINE_URL: &str =
    "https://github.com/ggml-org/whisper.cpp/releases/download/v1.9.2/whisper-bin-x64.zip";
const ENGINE_SHA: &str = "49dcc16de826f20bd53d44f947a1ae49dfa81f86cad67a64d80820cb192d674a";

/// `base.en` is the smallest model that gets proper nouns right, which is the
/// whole reason for being here — "Riot", "ARAM", "Optipeople". `tiny.en` is
/// half the size and loses them again.
///
/// The URL is a branch rather than a revision because that is what Hugging
/// Face publishes; the checksum is what actually pins it. If the file behind
/// the URL ever changes, the fetch fails and the cats stay on SAPI rather than
/// quietly running something nobody checked.
const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";
const MODEL_SHA: &str = "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002";
const MODEL_FILE: &str = "ggml-base.en.bin";

const CLI_FILE: &str = "whisper-cli.exe";

/// Roughly what the two downloads come to, for the one line the user sees.
const DOWNLOAD_MB: u64 = 150;

/// One line of the fetch's stdout.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Fetched {
    /// Started on one of the two pieces. `what` is something sayable.
    Fetching { what: String },
    Ready,
    Failed { message: String },
}

/// Whether everything needed to transcribe is already on disk.
pub fn ready(dir: &Path) -> bool {
    dir.join(CLI_FILE).is_file() && dir.join(MODEL_FILE).is_file()
}

pub fn size_note() -> String {
    format!("about {DOWNLOAD_MB} MB")
}

/// Downloads and verifies whatever is missing. One line of JSON per step.
const FETCH: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
$out = [Console]::Out
$ProgressPreference = 'SilentlyContinue'   # the progress bar dominates the transfer

function Say($o) { $out.WriteLine(($o | ConvertTo-Json -Compress)); $out.Flush() }

$dir = __DIR__
$model = Join-Path $dir '__MODEL_FILE__'
$cli = Join-Path $dir '__CLI_FILE__'

function Get-Verified($url, $target, $sha, $what) {
  Say @{ kind = 'fetching'; what = $what }
  $part = "$target.part"
  if (Test-Path $part) { Remove-Item $part -Force }
  Invoke-WebRequest -Uri $url -OutFile $part -UseBasicParsing
  $got = (Get-FileHash $part -Algorithm SHA256).Hash.ToLower()
  if ($got -ne $sha) {
    Remove-Item $part -Force
    throw "$what did not match its checksum - expected $sha, got $got"
  }
  Move-Item -Force $part $target
}

try {
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

  if (-not (Test-Path $model)) {
    Get-Verified '__MODEL_URL__' $model '__MODEL_SHA__' 'the language model'
  }

  if (-not (Test-Path $cli)) {
    $zip = Join-Path $dir 'engine.zip'
    Get-Verified '__ENGINE_URL__' $zip '__ENGINE_SHA__' 'the transcriber'

    $unpack = Join-Path $dir 'unpack'
    if (Test-Path $unpack) { Remove-Item -Recurse -Force $unpack }
    Expand-Archive -Path $zip -DestinationPath $unpack -Force

    # The archive nests everything under Release\ and ships a pile of demos
    # beside the one program we want. Take the transcriber and the libraries
    # it loads, flattened next to it so Windows finds them, and nothing else.
    Get-ChildItem -Path $unpack -Recurse -File |
      Where-Object { $_.Name -eq '__CLI_FILE__' -or $_.Extension -eq '.dll' } |
      ForEach-Object { Copy-Item $_.FullName (Join-Path $dir $_.Name) -Force }

    Remove-Item -Recurse -Force $unpack
    Remove-Item -Force $zip
    if (-not (Test-Path $cli)) { throw 'the archive had no transcriber in it' }
  }

  Say @{ kind = 'ready' }
} catch {
  Say @{ kind = 'failed'; message = [string]$_.Exception.Message }
  exit 1
}
"#;

/// Starts the fetch. Its stdout is the caller's to drain.
pub fn fetch(dir: &Path) -> Result<tokio::process::Child, String> {
    let script = FETCH
        .replace("__DIR__", &shell::quote(&dir.to_string_lossy()))
        .replace("__MODEL_FILE__", MODEL_FILE)
        .replace("__MODEL_URL__", MODEL_URL)
        .replace("__MODEL_SHA__", MODEL_SHA)
        .replace("__CLI_FILE__", CLI_FILE)
        .replace("__ENGINE_URL__", ENGINE_URL)
        .replace("__ENGINE_SHA__", ENGINE_SHA);

    shell::command(&script)
        .spawn()
        .map_err(|e| format!("couldn't start the download: {e}"))
}

pub fn parse(line: &str) -> Option<Fetched> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    serde_json::from_str(line).ok()
}

/// whisper.cpp's stand-ins for "there were no words here". Never a command.
const NOT_SPEECH: &[&str] = &["[blank_audio]", "[silence]", "(silence)", "[ inaudible ]"];

/// Folds whisper's output down to one line of plain command text.
///
/// It prints one line per segment, sometimes with bracketed annotations where
/// it heard no speech at all — those have to go, or a cough becomes a prompt.
pub fn tidy(raw: &str) -> String {
    let mut words: Vec<&str> = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || NOT_SPEECH.contains(&line.to_lowercase().as_str()) {
            continue;
        }
        words.extend(line.split_whitespace());
    }
    words.join(" ")
}

/// Transcribes one recorded utterance. Blocks for about a second.
pub async fn transcribe(dir: &Path, wav: &Path) -> Result<String, String> {
    let cli: PathBuf = dir.join(CLI_FILE);
    let model = dir.join(MODEL_FILE);

    let mut cmd = Command::new(&cli);
    cmd.arg("-m")
        .arg(&model)
        .arg("-f")
        .arg(wav)
        // No timestamps, no progress chatter, and don't waste a beat deciding
        // the language when the model only has the one.
        .arg("-nt")
        .arg("-np")
        .arg("-l")
        .arg("en")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    crate::bridge::detect::hide_console(cmd.as_std_mut());

    let out = cmd
        .output()
        .await
        .map_err(|e| format!("couldn't run the transcriber: {e}"))?;

    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        let tail: String = why.trim().chars().rev().take(200).collect();
        return Err(tail.chars().rev().collect());
    }

    Ok(tidy(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_fold_into_one_line() {
        assert_eq!(
            tidy(" Hey Tofu, open the launcher\n and queue for Aram.\n"),
            "Hey Tofu, open the launcher and queue for Aram."
        );
    }

    #[test]
    fn a_cough_is_not_a_command() {
        assert_eq!(tidy("[BLANK_AUDIO]"), "");
        assert_eq!(tidy("  [Silence]  \n"), "");
        // ...but a real line beside one still survives.
        assert_eq!(tidy("[BLANK_AUDIO]\n open notepad\n"), "open notepad");
    }

    #[test]
    fn nothing_at_all_is_nothing() {
        assert_eq!(tidy(""), "");
        assert_eq!(tidy("\n\n  \n"), "");
    }

    /// The checksums are the only thing standing between the user and running
    /// whatever happens to sit behind those URLs later, so a truncated or
    /// hand-edited constant must not slip through.
    #[test]
    fn both_downloads_are_pinned_to_a_real_sha256() {
        for sha in [ENGINE_SHA, MODEL_SHA] {
            assert_eq!(sha.len(), 64, "{sha} is not a sha256");
            assert!(sha.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
        }
        assert_ne!(ENGINE_SHA, MODEL_SHA);
        // A moving target would defeat the pin on the engine, which is the
        // half that is an executable.
        assert!(ENGINE_URL.contains("/download/v1.9.2/"));
    }

    #[test]
    fn the_script_carries_every_placeholder_it_needs() {
        let dir = std::path::Path::new(r"C:\Users\o'malley\AppData\Local\purrch\hearing");
        let script = FETCH
            .replace("__DIR__", &shell::quote(&dir.to_string_lossy()))
            .replace("__MODEL_FILE__", MODEL_FILE)
            .replace("__MODEL_URL__", MODEL_URL)
            .replace("__MODEL_SHA__", MODEL_SHA)
            .replace("__CLI_FILE__", CLI_FILE)
            .replace("__ENGINE_URL__", ENGINE_URL)
            .replace("__ENGINE_SHA__", ENGINE_SHA);

        assert!(!script.contains("__"), "a placeholder went unreplaced");
        // The one interpolated value the user can influence is the path, and
        // an apostrophe in their profile name must not end the literal.
        assert!(script.contains(r"'C:\Users\o''malley\AppData\Local\purrch\hearing'"));
    }

    #[test]
    fn readiness_needs_both_halves() {
        let dir = std::env::temp_dir().join(format!("purrch-kit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!ready(&dir));
        std::fs::write(dir.join(CLI_FILE), b"x").unwrap();
        assert!(!ready(&dir), "a transcriber with no model can't transcribe");
        std::fs::write(dir.join(MODEL_FILE), b"x").unwrap();
        assert!(ready(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole point, end to end: fetch the transcriber for real, then make
    /// it read something back. Covers everything the unit tests can't — that
    /// the URLs still resolve, that the checksums still match what's served,
    /// that the archive still unpacks to the shape expected, and that the
    /// program that comes out can actually be run.
    ///
    /// Ignored by default: it pulls ~150 MB. Run it by hand after changing any
    /// of the pins above, which is exactly when it matters.
    #[tokio::test]
    #[ignore = "downloads ~150 MB"]
    async fn the_fetch_really_lands_a_working_transcriber() {
        use tokio::io::{AsyncBufReadExt, BufReader};

        let dir = std::env::temp_dir().join(format!("purrch-fetch-{}", uuid::Uuid::new_v4()));
        let mut child = fetch(&dir).expect("couldn't start the fetch");

        let mut steps = Vec::new();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(step) = parse(&line) {
                if let Fetched::Failed { message } = &step {
                    let _ = std::fs::remove_dir_all(&dir);
                    panic!("the fetch gave up: {message}");
                }
                steps.push(step);
            }
        }
        let _ = child.wait().await;

        assert!(
            steps.iter().any(|s| matches!(s, Fetched::Ready)),
            "the fetch never said it was ready: {steps:?}"
        );
        assert!(ready(&dir), "nothing usable landed in {}", dir.display());

        // Both halves have to be the real thing, not a redirect page that
        // happened to match nothing.
        assert!(dir.join(MODEL_FILE).metadata().unwrap().len() > 100_000_000);
        // And the libraries have to have been flattened next to the program,
        // or Windows won't find them when it runs.
        assert!(dir.join("whisper.dll").is_file());
        assert!(!dir.join("unpack").exists(), "the scratch dir was left behind");
        assert!(!dir.join("engine.zip").exists(), "the archive was left behind");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reads a sentence back through the real invocation — the flags, the
    /// stdout parsing, all of it. The sentence is the one SAPI mangles into
    /// "the right games launch a rank euphoria room", so this also pins down
    /// the reason any of this exists.
    ///
    /// The voice is Windows' own synthesiser, which is a good deal cleaner
    /// than a person in a room; passing here proves the plumbing, not the
    /// accuracy.
    #[tokio::test]
    #[ignore = "downloads ~150 MB"]
    async fn it_reads_back_what_sapi_could_not() {
        use tokio::io::{AsyncBufReadExt, BufReader};

        let dir = std::env::temp_dir().join(format!("purrch-say-{}", uuid::Uuid::new_v4()));
        let mut child = fetch(&dir).expect("couldn't start the fetch");
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        while let Ok(Some(_)) = lines.next_line().await {}
        let _ = child.wait().await;
        assert!(ready(&dir), "nothing to transcribe with");

        let wav = dir.join("spoken.wav");
        let say = format!(
            r#"
Add-Type -AssemblyName System.Speech
$tts = New-Object System.Speech.Synthesis.SpeechSynthesizer
$tts.SetOutputToWaveFile({path})
$tts.Rate = -1
$tts.Speak('Hey Tofu, open the Riot Games launcher and queue for ARAM')
$tts.Dispose()
"#,
            path = shell::quote(&wav.to_string_lossy())
        );
        let spoken = shell::command(&say).output().await.expect("couldn't speak");
        assert!(spoken.status.success(), "the synthesiser refused");

        let said = transcribe(&dir, &wav).await.expect("transcribe failed");
        let _ = std::fs::remove_dir_all(&dir);

        let flat = said.to_lowercase();
        assert!(flat.contains("riot games"), "got: {said}");
        assert!(flat.contains("launcher"), "got: {said}");
        assert!(flat.contains("aram"), "got: {said}");
    }

    #[test]
    fn lines_of_the_fetch_become_steps() {
        assert!(matches!(parse(r#"{"kind":"ready"}"#), Some(Fetched::Ready)));
        assert!(matches!(
            parse(r#"{"kind":"fetching","what":"the language model"}"#),
            Some(Fetched::Fetching { .. })
        ));
        assert!(parse("").is_none());
        assert!(parse("Invoke-WebRequest : boom").is_none());
    }
}
