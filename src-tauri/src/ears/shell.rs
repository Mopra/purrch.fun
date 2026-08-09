//! Running PowerShell, which is how Purrch reaches the parts of Windows it
//! doesn't want to link against.
//!
//! Shared by the two halves of the ear: the listener that holds the microphone
//! ([`sapi`](super::sapi)) and the fetch that goes and gets a better
//! transcriber ([`whisper`](super::whisper)).

use std::process::Stdio;
use tokio::process::Command;

/// Windows PowerShell specifically, and never `pwsh`: `System.Speech` is a
/// .NET Framework assembly, which PowerShell 7 cannot `Add-Type`.
pub fn powershell() -> String {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let path = format!("{root}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe");
    if std::path::Path::new(&path).is_file() {
        path
    } else {
        "powershell.exe".to_string()
    }
}

/// `-EncodedCommand` takes base64 of UTF-16LE, which sidesteps every layer of
/// quoting between here and the script.
pub fn encode(script: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let utf16: Vec<u8> = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();

    let mut out = String::with_capacity(utf16.len().div_ceil(3) * 4);
    for chunk in utf16.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((*chunk.get(1).unwrap_or(&0) as u32) << 8)
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// A PowerShell array literal of single-quoted strings.
///
/// The one place user-written text — a cat's name — reaches a script, so the
/// quote doubling is what stops a name closing its own string and running as
/// code.
pub fn array(items: &[String]) -> String {
    if items.is_empty() {
        return "@()".to_string();
    }
    let quoted: Vec<String> = items.iter().map(|s| quote(s)).collect();
    format!("@({})", quoted.join(","))
}

/// One single-quoted PowerShell string literal.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// A hidden, profile-free PowerShell running `script`, with both pipes open.
pub fn command(script: &str) -> Command {
    let mut cmd = Command::new(powershell());
    cmd.arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-EncodedCommand")
        .arg(encode(script))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    crate::bridge::detect::hide_console(cmd.as_std_mut());
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_encoding_powershell_expects() {
        // UTF-16LE of "hi" is 68 00 69 00.
        assert_eq!(encode("hi"), "aABpAA==");
        assert_eq!(encode("a"), "YQA=");
        assert_eq!(encode(""), "");
    }

    #[test]
    fn a_quote_cant_break_out_of_a_literal() {
        assert_eq!(quote("o'malley"), "'o''malley'");
        assert_eq!(array(&["o'malley".to_string()]), "@('o''malley')");
        assert_eq!(array(&[]), "@()");
    }

    #[test]
    fn paths_with_quotes_in_them_survive() {
        // Windows allows an apostrophe in a folder name, and the user's own
        // profile is the likeliest place to find one.
        let path = r"C:\Users\o'malley\AppData\Local";
        assert_eq!(quote(path), r"'C:\Users\o''malley\AppData\Local'");
    }
}
