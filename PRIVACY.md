# Privacy

Short version: Purrch has no servers, no account and no telemetry. It never
sends anything anywhere. The two things that *do* leave your machine leave it
through software you installed and logged into yourself, and this page is about
exactly what those are.

## What Purrch itself sends

Nothing. There is no analytics, no crash reporting, no update ping carrying
anything about you, and no "anonymous usage data". Purrch makes exactly two
kinds of request of its own, both to fixed GitHub URLs and neither carrying
anything about you:

- a check for a newer version, only when you ask for one from the tray or the
  panel;
- a one-time download of the local speech engine, only if you use voice. See
  *Your microphone* below.

## What your agent CLI sends

Everything you type at a cat, everything a chore is told to do, and whatever
files or command output the agent decides to read on the way, go to the model
provider behind whichever CLI you picked — Anthropic for Claude Code, OpenAI for
Codex CLI. That is the same thing that would happen if you ran the CLI in a
terminal yourself, under the same account and the same policy.

Purrch does not proxy it, log it, or see the responses beyond what it draws in
the panel. If you want to know what is retained, that is a question for your
provider's policy, not for this app.

Two Purrch-specific details worth knowing:

- The cat's name is put into the system prompt of every turn, so the provider
  sees it.
- A chore's prompt is sent on a schedule, with nobody watching, as long as
  Purrch is running.

## Your microphone

Only if you leave a cat's ears on, and only after you have let the colony
loose — a cat that has never been agreed to has never had the microphone open.

Speech recognition is local. No audio is streamed anywhere, and no recording is
sent off the machine.

Two engines are involved, and both run on your computer:

- **Windows' own recogniser holds the microphone.** It is given one fixed shape
  of sentence — a cat's name, then some words — so speech that doesn't open with
  a cat's name is rejected by the engine before Purrch is told anything about
  it. A conversation your cats weren't addressed in never becomes text.
- **whisper.cpp reads back what was said**, because Windows' own dictation
  mangles proper nouns. It only ever sees an utterance that already matched a
  cat's name.

That second step means a short **WAV of the matched utterance is written to a
scratch folder** in Purrch's local app data, handed to the transcriber, and
deleted as soon as the turn it belongs to is done — including when the turn
fails. Nothing is kept, but it does briefly touch the disk, so it isn't nothing.

whisper.cpp is not bundled: on first use Purrch downloads a pinned release and
model (~150 MB) from GitHub, checks it against a hard-coded checksum, and throws
away anything that doesn't match. That download is the one file Purrch fetches
that isn't an update of itself. Until it arrives, voice still works on the
Windows recogniser alone.

What is heard after a cat's name is then sent to your agent CLI as though you
had typed it — and from there to that CLI's provider, as above.

Right-click a cat → **stop listening** to turn it off for that cat.

## Your API key, if you give one

Optional, and off by default; the normal path is that Purrch holds no
credential at all and just drives your CLI's own login.

If you do save one, it goes to **Windows Credential Manager** under the service
name `fun.purrch.keys`. It is never written into Purrch's JSON files, never put
on a command line where other local processes could read it, and never included
in a log or an event. The only thing that ever sees it again is the environment
of the CLI process Purrch spawns.

If Credential Manager cannot be reached, the key falls back to a file in
Purrch's config folder, locked to your user account — and the panel tells you
that has happened, because a secret silently kept somewhere it wasn't promised
to be is worse than one you know about.

## What is on your disk

In `%APPDATA%\fun.purrch.pet\`:

- `cats.json` — each cat's name, position, mood, tallies, and the last ~80 lines
  of your conversation with it, so the panel can pick up where it left off.
- `chores.json` — your chores, and the gifts they produced, including the list
  of tools each hunt used.
- `colony.json` — whether you agreed, when, your daily cap, and the timestamps
  of recent hunts.
- `auth.json` — which purse each backend spends. Never the key itself.
- `logs\` — what Purrch did, capped at a couple of megabytes and rotated.

And in `%LOCALAPPDATA%un.purrch.pet\`:

- `hearing\` — the downloaded speech engine and model, plus the scratch folder
  the short recordings pass through. Local rather than roaming because 150 MB
  has no business following you onto another machine.

All of it is plain text you can read, apart from the engine. The uninstaller
deletes both folders.
Nothing in it is uploaded by Purrch, including the log — if you want to attach a
log to a bug report, you do that yourself, and you should skim it first, because
it records the tools your cats ran.

## Children

Purrch is not for children, and there is nothing in it aimed at them. It hands
an unsupervised agent full control of a computer.

## Changes

This file is versioned with the app. If it changes, the change is in the git
history alongside the code that made it necessary.
