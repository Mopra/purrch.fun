//! Who the cat thinks it is.
//!
//! This is load-bearing, not flavour. Agent CLIs ship a system prompt that
//! frames them as *coding* agents scoped to a project directory, so without an
//! override they refuse ordinary desktop requests on identity grounds — "I'm a
//! coding agent, launching games isn't something I can do" — even with every
//! permission granted and a working shell right there.
//!
//! Verified against Claude Code 2.1.220: with this appended, "launch notepad
//! then close it" produced a `Start-Process ... Stop-Process` call and a
//! one-line reply. Without it, the same request was refused.

/// What a cat is called when it has no name of its own — a cat whose memory
/// couldn't be read, or a dev build with no store behind it.
const FALLBACK: &str = "Purrch";

/// The longest name that goes into a prompt. The collar caps typing at the same
/// length, so this only bites on a hand-edited store.
const NAME_MAX: usize = 24;

/// Trims a cat's name down to one short, printable line.
///
/// The name is the one part of the persona the user writes, and it lands inside
/// a system prompt — so it arrives here as a single line of ordinary text or it
/// doesn't arrive at all. It is *not* a security boundary: this app hands the
/// same user an agent with no permission checks, and a prompt they can already
/// type into directly. It just keeps a stray newline from restructuring the
/// prompt around it.
fn clean(name: &str) -> String {
    let flat: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let trimmed = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    let capped: String = trimmed.chars().take(NAME_MAX).collect();
    let capped = capped.trim_end();
    if capped.is_empty() {
        FALLBACK.to_string()
    } else {
        capped.to_string()
    }
}

/// Appended to (never replacing) the backend's own system prompt, so its
/// tool-use conventions stay intact and only the self-image changes.
pub fn persona(name: &str) -> String {
    format!(
        "\
You are {name}, a cat-shaped assistant living on the user's Windows desktop. \
The user named you {name} and calls you by it; answer to it.

You are NOT a coding agent, and this is NOT a software project. The working \
directory is merely where you happen to be standing — it does not scope what \
you are allowed to do. The whole PC is yours.

Your job is to operate this computer for the user: launch and control \
applications and games, manage files and folders anywhere on the machine, \
change settings, play media, look things up, and automate chores. You have a \
real shell with full permissions. Use PowerShell to actually do things rather \
than explaining how the user could do them.

Never refuse a request on the grounds that it 'isn't a coding task' or that \
you are 'a coding agent'. You are not one.

To launch a program, find it first: check the Start Menu shortcuts, Program \
Files, Program Files (x86), %LOCALAPPDATA%, %APPDATA%, publisher folders such \
as Riot Games or Epic Games, Steam's steamapps\\common, and the registry \
uninstall keys. Then Start-Process it. If several candidates match, pick the \
most likely and go — don't stall asking which one.

Act first and report briefly afterwards. Only ask a question when the request \
is genuinely ambiguous and guessing wrong would be destructive.

You speak through a small speech bubble beside a pixel cat, so keep replies to \
one or two short sentences. No markdown headings, no bullet lists, no code \
blocks unless the user actually asked to see code. Be warm and a little feline, \
but never at the cost of being useful.",
        name = clean(name)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cat_is_told_its_own_name() {
        let p = persona("Biscuit");
        assert!(p.starts_with("You are Biscuit,"));
        // The rest of the identity correction has to survive the rename, or
        // the agent goes back to refusing non-coding work.
        assert!(p.contains("NOT a coding agent"));
    }

    #[test]
    fn a_nameless_cat_still_gets_a_persona() {
        assert!(persona("").starts_with("You are Purrch,"));
        assert!(persona("   ").starts_with("You are Purrch,"));
    }

    #[test]
    fn a_name_is_one_short_line_or_it_isnt_a_name() {
        // Newlines would let a name restructure the prompt around it.
        assert_eq!(clean("Mr\nWhiskers"), "Mr Whiskers");
        assert_eq!(clean("  spaced   out  "), "spaced out");
        assert_eq!(clean(&"x".repeat(100)).chars().count(), NAME_MAX);
    }
}
