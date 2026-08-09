# Purrch

A pixel cat lives on your taskbar and runs your PC for you.

Ask it out loud or in a little panel — "open my mail", "launch the game",
"tidy the downloads folder" — and it does it, with a real shell, as you. Give
it chores and it goes off on a schedule while you're doing something else, then
leaves what it found by the door.

It has no model of its own. Purrch drives an agent CLI you have already
installed and signed in to, so the work is billed to *your* subscription by
*their* first-party tool. Nothing is sent anywhere by Purrch itself.

## Requirements

- **Windows 10 or 11.** The cat's persona, its speech recognition and the
  installer are all Windows-specific today. Nothing else is supported, and the
  parts of the code that look cross-platform are not tested anywhere else.
- **Optional, for voice:** nothing to install. The first time you talk to a cat,
  Purrch downloads a pinned, checksummed build of whisper.cpp (~150 MB) to
  transcribe what you said. Until it lands, voice runs on Windows' own
  recogniser — worse, but working.
- **An agent CLI, installed and logged in:**
  - [Claude Code](https://claude.com/claude-code) — verified, and the one to use.
  - [Codex CLI](https://developers.openai.com/codex/cli) — **untested.** The
    adapter was written from documentation, not from a captured run. The panel
    marks it as untested for exactly this reason.

  Gemini CLI and opencode are detected but cannot be driven yet; the panel says
  so rather than pretending they aren't there.

## This cat has no leash

Purrch runs your agent with **every permission check turned off**. That is the
product, not an oversight — a pet that stops to ask permission before opening a
folder is not a pet, it is a dialog box with fur. But it means exactly what it
says:

- It can run any command, read or delete any file, and reach the network, on
  your machine, as you, without asking first.
- **It follows whatever it reads.** A web page, an email or a file can contain
  text that tells it what to do, and it may well do it. This is the real risk,
  and it is sharpest for chores, which run with nobody watching.
- What stands in for a permission prompt is *visibility*. Every tool call
  streams into the panel as it happens, and every chore keeps its own list of
  what it did — open a gift and unfold the tool count to see it.

You agree to this once, on first run, and the composer, the microphone and the
chore board stay locked until you do. The agreement is kept by the app itself,
not by the web layer, so nothing routes around it.

## Chores, hunts and gifts

- A **chore** is a standing job you hand to one cat: a prompt, a folder, and
  how often to go and look.
- One execution of it is a **hunt**. The cat gets up and paces the taskbar
  while it works, whether or not you're watching.
- What it comes back with is a **gift**, left in a pile by the door until you
  look.

Chores only fire while Purrch is running, so the tray menu has **start with
Windows** — the board nudges you about it if you've written chores and left it
off.

### What this costs

Every hunt is a turn against your subscription, whether it finds anything or
not. Three things bound it:

- nothing may fire more often than every five minutes;
- each cat has a **daily cap** on how many errands it runs, shown as a bar at
  the top of the chore board and adjustable there (40/day by default, over a
  rolling 24 hours rather than a calendar day);
- a slot missed while the PC was off is dropped, not replayed, so a weekend
  away doesn't fire sixty runs at breakfast.

What still isn't solved: Purrch has no idea how close *your own* session is to
a rate limit. The cap is a blunt instrument, and on a busy board you should set
it low.

## A colony

Right-click a cat → **another cat** and you get a second one: its own name,
coat, agent session, memory, chore board and budget. Cats are assignments —
this one watches the repo, that one watches the inbox, that one is just there
to look nice.

Sending a cat home keeps its life; the slot it stood in remembers it. The last
cat you dismiss quits Purrch.

## Where your things are kept

- `%APPDATA%\fun.purrch.pet\` — cats, chores, gifts, the agreement, the budget.
  All plain JSON; read it whenever you like. The uninstaller removes it.
- **Windows Credential Manager**, under `fun.purrch.keys` — an API key, if you
  gave one. Never in the JSON, never in a log, never on a command line. The
  uninstaller deliberately leaves it alone; "go back to your subscription" in
  the panel is what forgets it.
- `%APPDATA%\fun.purrch.pet\logs\` — what Purrch did, for when it misbehaves.
  Reachable from the tray. Nothing is ever uploaded; see [PRIVACY.md](PRIVACY.md).
- `%LOCALAPPDATA%\fun.purrch.pet\hearing\` — the downloaded speech engine and
  model. Local rather than roaming, because 150 MB shouldn't follow you between
  machines. The uninstaller removes this too.

By default Purrch holds no credential at all and simply drives the CLI's own
login. A key is opt-in, for people who would rather spend metered tokens than
their subscription allowance.

## Building it

```sh
npm install
npm run tauri dev     # the real thing
npm run dev           # just the cat, in a browser, for art work
```

Checks:

```sh
npm run check                     # tsc + svelte-check
cargo test --manifest-path src-tauri/Cargo.toml
```

Two test suites are ignored by default because they need hardware CI doesn't
have — a microphone, and the OS keychain. Run them by hand on anything you
intend to ship from:

```sh
cargo test --manifest-path src-tauri/Cargo.toml -- --ignored
```

Releasing, signing and the updater are in [RELEASING.md](RELEASING.md).

## Layout

| Path | What's in it |
| --- | --- |
| `src-tauri/src/lib.rs` | Window geometry, the taskbar the cat stands on, and every command |
| `src-tauri/src/bridge/` | Finding agent CLIs, driving them, and translating their output |
| `src-tauri/src/chores.rs` | The board, the calendar, and the pile of gifts |
| `src-tauri/src/hunt.rs` | The clock, the per-cat queue, and one errand at a time |
| `src-tauri/src/colony.rs` | The agreement, and the daily budget |
| `src-tauri/src/ears/` | Listening for a cat's name |
| `src/lib/render.ts` | The pixel compositor — no DOM, shared with the art scripts |
| `src/Cat.svelte` | Everything the cat does when you aren't asking it anything |

[IDEAS.md](IDEAS.md) is the board: what's being built, what was decided, and
what's still open.

## Licence

MIT. See [LICENSE](LICENSE).
