<script lang="ts">
  import { onMount } from "svelte";
  import Cat from "./Cat.svelte";
  import Chat from "./Chat.svelte";
  import Collar from "./Collar.svelte";
  import Chores from "./Chores.svelte";
  import Gifts from "./Gifts.svelte";
  import type { Entry } from "./lib/entry.ts";
  import * as identity from "./lib/identity.ts";
  import { coatById } from "./lib/coats.ts";
  import * as bridge from "./lib/bridge.ts";
  import * as chores from "./lib/chores.ts";
  import * as ears from "./lib/ears.ts";
  import { recall, remember, flush, type CatMemory } from "./lib/memory.ts";

  let cat: Cat;
  let menuOpen = $state(false);
  let menuEl = $state<HTMLDivElement | undefined>();
  /** Whatever had the keyboard before the menu took it, to hand it back. */
  let refocus: HTMLElement | null = null;

  /**
   * The window is what clips the menu — nothing drawn past its edge survives —
   * so the menu is placed against the viewport rather than against the cursor,
   * and the window is grown to fit whatever the menu turns out to measure.
   */
  const MENU_PAD = 4;
  let menuW = $state(0);
  let menuH = $state(0);
  let winW = $state(0);
  let winH = $state(0);
  /** Set while the window is being made big enough; the menu waits it out. */
  let fitting = $state(false);
  /** Whether the window is currently carrying height it borrowed for the menu. */
  let grown = false;

  /**
   * Where the menu was asked for, held as a distance from the window's right
   * and bottom edges rather than from its top-left.
   *
   * Making room for the menu grows the window up and to the left — the cat is
   * pinned to the bottom-right corner, with the taskbar directly under its
   * feet — so those two edges are the ones that stay put across the resize.
   * Anchoring to them leaves the menu under the cursor either way; anchoring
   * to the top-left would strand it where the smaller window used to be.
   */
  let menuRight = $state(0);
  let menuBottom = $state(0);
  const menuX = $derived(winW - menuRight);
  const menuY = $derived(winH - menuBottom);

  let menuPos = $derived.by(() => {
    // Prefer down-right of the cursor, flip to the other side when that
    // would overflow, then clamp in case neither side fits.
    const fit = (v: number, limit: number) =>
      Math.max(MENU_PAD, Math.min(v, limit - MENU_PAD));
    const x = menuX + menuW > winW - MENU_PAD ? menuX - menuW : menuX;
    const y = menuY + menuH > winH - MENU_PAD ? menuY - menuH : menuY;
    return { x: fit(x, winW - menuW), y: fit(y, winH - menuH) };
  });

  /**
   * The cat's own window is 200 logical pixels tall and the menu is taller
   * than that, so it has to borrow the height before it can be shown whole.
   * Measured from `scrollHeight`, which is what the menu wants rather than
   * what the window is currently letting it have.
   */
  $effect(() => {
    // Re-runs on every remeasure, and again once the window has resized.
    if (!menuOpen || !menuEl || !menuH) return;
    const w = menuEl.scrollWidth + MENU_PAD * 2;
    const h = menuEl.scrollHeight + MENU_PAD * 2;
    if (w <= winW && h <= winH) return;
    fitting = true;
    grown = true;
    // If the screen itself is too short the window comes back short too, and
    // the menu is shown clipped rather than never shown at all.
    void bridge.setMenu(w, h).finally(() => (fitting = false));
  });

  let panelOpen = $state(false);

  /**
   * Which of the cat's panels has the room. They're mutually exclusive by
   * design: the window is 380 pixels wide, and every one of these wants all
   * of it.
   */
  type View = "chat" | "collar" | "chores" | "gifts";
  let view = $state<View>("chat");

  // --- chores, hunts and gifts ---
  //
  // What this cat does when you aren't asking it anything. The board and the
  // clock behind it are in Rust; this side is the three ways you find out
  // about it, which are the whole point of the feature:
  //
  //  - ambient — the cat gets up and paces the taskbar mid-hunt, exactly as it
  //    does for a turn you asked for, and a gift lands as a mouse by its paws
  //  - check-in — hover the cat and it says what it's doing, in one line
  //  - gifts — the pile by the door, which waits until you look
  let board = $state<chores.Chore[]>([]);
  let pile = $state<chores.Gift[]>([]);
  /** The chore this cat is out on, or null if it's home. */
  let live = $state<chores.Live | null>(null);
  /** The check-in line: what it's doing right now, in as many words. */
  let doing = $state("");
  let unread = $derived(pile.filter((g) => !g.read).length);
  let backends = $state<bridge.Backend[]>([]);
  let backendId = $state("");
  let entries = $state<Entry[]>([]);
  let busy = $state(false);

  /** One-time agreement that the cat runs with no permission checks at all. */
  const AGREED_KEY = "purrch.unleashed";
  let agreed = $state(false);

  function agree() {
    agreed = true;
    try {
      localStorage.setItem(AGREED_KEY, "1");
    } catch {
      // private mode / storage disabled — it'll just ask again next launch
    }
  }

  /** Continues the conversation across turns; reset when the cat forgets. */
  let session: string | null = null;
  let cwd = $state<string | null>(null);
  /** Whether this turn already streamed prose, so the final result isn't echoed. */
  let sawText = false;

  // --- memory ---
  // Everything the cat is still carrying from the last time Purrch was open:
  // the conversation it was in the middle of, where it was working, and the
  // tally of what the two of you have got through together. Nothing is written
  // back until that's been read, or an empty panel would overwrite it.
  let memory = $state<CatMemory | null>(null);
  let loaded = false;
  let turns = 0;
  let tools = 0;

  /**
   * The remembered session may be gone: agent CLIs prune old transcripts, and
   * they're stored per working directory, so a folder that moved takes its
   * conversation with it. Rather than showing the user a CLI error they can do
   * nothing about, the first turn after a restart quietly starts over.
   */
  let stale = false;
  let lastPrompt = "";

  // --- who this cat is ---
  // Its name and its coat, kept per window so a colony is a colony rather than
  // one cat shown several times. Both are only defaults until you open the
  // collar; nothing here is guessed twice, since `load` writes what it mints.
  let label = "main";
  let who = $state<identity.Identity>({
    name: "Purrch",
    coat: "marmalade",
    ears: true,
  });
  let coat = $derived(coatById(who.coat));
  /**
   * Set the moment this cat knows what it's actually called, so the ear is
   * never opened for the placeholder above — the colony would spend a beat
   * listening for a name nobody uses.
   */
  let named = $state(false);

  function dress(next: Partial<identity.Identity>) {
    who = { ...who, ...next };
    identity.save(label, who);
    // The agent runs as this cat, by name, and that happens in Rust — so the
    // name has to reach the memory store as well as the collar.
    remember({ name: who.name });
  }

  let draft = $state("");

  /** Quote a path only if it needs it, so prompts stay readable. */
  function quote(path: string): string {
    return /\s/.test(path) ? `"${path}"` : path;
  }

  /**
   * A folder re-scopes the session; files just get named in the composer.
   *
   * Re-scoping has to start a fresh conversation: agent CLIs store sessions
   * per working directory, so resuming an old id from a new cwd would either
   * fail or silently reopen the wrong transcript.
   */
  async function drop(paths: string[]) {
    if (paths.length === 0) return;
    const { dirs, files } = await bridge.classifyPaths(paths);
    cat?.pet();

    if (dirs.length > 0) {
      cwd = dirs[0];
      session = null;
      stale = false;
      remember({ cwd, session: null });
      entries = [
        ...entries,
        { role: "cat", text: `now working in ${dirs[0]} (fresh start)` },
      ];
    }

    if (files.length > 0) {
      const named = files.map(quote).join(" ");
      draft = draft.trim() ? `${draft.trim()} ${named}` : `${named} `;
    }

    if (!panelOpen) await togglePanel(true);
  }

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let unlistenDrop: (() => void) | undefined;
    let unlistenFocus: (() => void) | undefined;
    let unlistenEar: (() => void) | undefined;
    let unlistenHunt: (() => void) | undefined;
    let unlistenGift: (() => void) | undefined;

    try {
      agreed = localStorage.getItem(AGREED_KEY) === "1";
    } catch {
      agreed = false;
    }

    (async () => {
      label = await bridge.catLabel();
      who = identity.load(label);
      named = true;
      backends = await bridge.detect();
      const past = await recall();
      memory = past;

      // The backend it was last thinking with, as long as that CLI is still
      // installed. Otherwise one that has some credential behind it — a login
      // or a key the user saved, either will do.
      const before = backends.find((b) => b.id === past?.backend);
      backendId =
        (before ?? backends.find(bridge.usable) ?? backends[0])?.id ?? "";

      // Picks the conversation back up where it was left: same folder, same
      // agent session, same transcript in the panel.
      cwd = past?.cwd ?? (await bridge.homeDir());
      if (past) {
        session = past.session;
        stale = !!past.session;
        entries = past.entries ?? [];
        turns = past.turns;
        tools = past.tools;
      }
      loaded = true;
      // A cat named before it had a memory file — or renamed in another
      // window — still has to answer to that name in Rust.
      if (past && past.name !== who.name) remember({ name: who.name });

      unlisten = await bridge.onEvent(handle);
      // A file over the cat is answered by the cat, not by the window: it
      // looks up at what you're holding, so you can tell it will be caught.
      unlistenDrop = await bridge.onDrop(drop, (h, x) => cat?.curious(h, x));
      // Most of the screen isn't this window, so most ways of walking away
      // from an open menu are answered here rather than by a click on it.
      unlistenFocus = await bridge.onFocusChange((focused) => {
        if (!focused) closeMenu();
      });
      unlistenEar = await ears.onEar(listened);

      // The cat may already be out when this window opens — chores fire
      // whether or not anything is watching, and a reload lands mid-hunt.
      // Subscribed before the state is read so nothing slips through the gap.
      unlistenHunt = await chores.onHunt(huntEvent);
      unlistenGift = await chores.onGift(gift);
      board = await chores.list();
      pile = await chores.gifts();
      const out = await chores.status();
      if (out.live) {
        live = out.live;
        doing ||= `out on ${out.live.name}`;
        cat?.work();
      }
    })();

    return () => {
      unlisten?.();
      unlistenDrop?.();
      unlistenFocus?.();
      unlistenEar?.();
      unlistenHunt?.();
      unlistenGift?.();
    };
  });

  function handle(e: bridge.BridgeEvent) {
    switch (e.kind) {
      case "started":
        session = e.session;
        // The agent answered, so whatever it was handed was good.
        stale = false;
        remember({ session: e.session });
        break;

      case "thinking":
        cat?.think();
        break;

      case "text":
        sawText = true;
        entries = [...entries, { role: "cat", text: e.text }];
        cat?.think();
        break;

      case "toolStart":
        entries = [...entries, { role: "tool", tool: e.tool, text: e.detail }];
        remember({ tools: ++tools });
        cat?.work();
        cat?.spark();
        break;

      case "toolEnd": {
        // The id in the event doesn't map back to a name, so close out the
        // most recent tool entry that's still pending.
        for (let i = entries.length - 1; i >= 0; i--) {
          const entry = entries[i];
          if (entry.role !== "tool" || entry.ok !== undefined) continue;
          const next = [...entries];
          next[i] = { ...entry, ok: e.ok };
          entries = next;
          break;
        }
        break;
      }

      case "finished":
        busy = false;
        if (e.text && !sawText) {
          entries = [...entries, { role: "cat", text: e.text }];
        }
        remember({ turns: ++turns });
        if (e.ok) cat?.done();
        else cat?.rest();
        break;

      case "failed":
        // The agent never even started on the first turn after a restart: the
        // conversation it was told to resume isn't there any more. Start a
        // fresh one and try again rather than blaming the user for it.
        if (stale && lastPrompt) {
          stale = false;
          session = null;
          remember({ session: null });
          void send(lastPrompt, true);
          break;
        }
        busy = false;
        entries = [...entries, { role: "error", text: e.message }];
        cat?.oops();
        break;
    }
  }

  /** `again` is the silent retry of a turn whose resumed session was gone. */
  async function send(text: string, again = false) {
    if (!again) entries = [...entries, { role: "you", text }];
    lastPrompt = text;
    busy = true;
    sawText = false;
    cat?.think();
    try {
      await bridge.send({
        backend: backendId,
        prompt: text,
        resume: session,
        cwd,
        name: who.name,
      });
    } catch (err) {
      busy = false;
      entries = [...entries, { role: "error", text: String(err) }];
      cat?.oops();
    }
  }

  function cancel() {
    bridge.cancel();
  }

  // --- being spoken to ---
  //
  // The whole point of talking to a cat is not having to open anything, so
  // everything here has to be legible from the taskbar alone: the cat's ears
  // for "I'm listening", one line above its head for "this is what I heard",
  // and then the ordinary working animation for the rest. The panel is only
  // ever where you go afterwards to read back what happened.

  /** Why the colony can't hear you, if it can't. */
  let deaf = $state<string | null>(null);

  /** One line above the cat, for a few seconds. */
  let caption = $state<{ text: string; muted: boolean } | null>(null);
  let captionFor = 0;

  /** Whether the pointer is over the cat's window right now. */
  let peeking = $state(false);

  /**
   * Looking over the cat's shoulder — the check-in layer.
   *
   * Costs nothing and shows nothing until you actually look: the desktop stays
   * quiet, and one hover answers "what's that one up to" without opening a
   * panel or scrolling a transcript. Shares the caption's line above the cat,
   * and yields to it — speech is rarer and it's gone in three seconds.
   */
  let peek = $derived.by(() => {
    if (!peeking || panelOpen) return "";
    if (doing) return doing;
    if (live) return `out on ${live.name}`;
    if (unread > 0) return `${unread} gift${unread === 1 ? "" : "s"} by the door`;
    return "";
  });

  const CAPTION_MS = 3400;

  function say(text: string, muted = false) {
    caption = { text, muted };
    clearTimeout(captionFor);
    captionFor = window.setTimeout(() => (caption = null), CAPTION_MS);
  }

  /**
   * The ear is colony-wide, so this is only ever *this* cat's half of it: its
   * own name, and whether it wants to answer to it.
   *
   * Gated on the agreement as well as the switch. Voice is the one way into
   * this app that doesn't pass through the composer, so it must not be the way
   * somebody skips the thing the composer is locked behind.
   */
  $effect(() => {
    if (!named) return;
    void ears.tune(who.name, who.ears && agreed);
  });

  function listened(e: ears.EarEvent) {
    switch (e.kind) {
      case "listening":
        deaf = null;
        break;

      case "deaf":
        deaf = e.message;
        // Worth saying out loud once: a cat that silently never hears you is
        // indistinguishable from a broken one.
        if (who.ears && agreed) say(e.message, true);
        break;

      case "learning":
        // A one-off, the first time a cat opens its ears. Until it finishes,
        // voice runs on Windows' own recogniser, which mishears anything with
        // a name in it — so silence here would read as the cat being stupid
        // rather than as it still getting ready.
        if (who.ears && agreed) say(e.message, true);
        break;

      case "perked":
        cat?.listen();
        break;

      case "missed":
        cat?.unheard();
        break;

      case "heard":
        obey(e.text);
        break;
    }
  }

  /** Something was said to this cat, by name. */
  function obey(text: string) {
    const command = text.trim();

    // Called and nothing else. That's being greeted, not tasked.
    if (!command) {
      cat?.pet();
      return;
    }

    // "Tofu, stop" has to work while it's working — that's the whole reason
    // it's a spoken command rather than the button in the panel.
    if (ears.isStop(command)) {
      if (busy) {
        say("stopping");
        cancel();
      } else {
        cat?.unheard();
      }
      return;
    }

    if (busy) {
      say('busy — say "stop" first', true);
      cat?.unheard();
      return;
    }

    if (backends.length === 0) {
      say("no agent CLI to ask", true);
      cat?.unheard();
      return;
    }

    say(command);
    void send(command);
  }

  function toggleEars() {
    closeMenu();
    dress({ ears: !who.ears });
  }

  /** Resolves once the frame that was just built has been put on screen. */
  function painted(): Promise<void> {
    return new Promise((done) =>
      requestAnimationFrame(() => requestAnimationFrame(() => done())),
    );
  }

  /**
   * Waits for the webview to take up the window's new size.
   *
   * The resize reaches it as its own message from the OS, so it hasn't happened
   * yet when `setPanel` returns — drawing any earlier would put a panel laid
   * out for the old window inside the new one.
   */
  function resized(): Promise<void> {
    return new Promise((done) => {
      let timer = 0;
      const settle = () => {
        clearTimeout(timer);
        window.removeEventListener("resize", settle);
        requestAnimationFrame(() => done());
      };
      // Nothing resizes in a plain browser, so never wait longer than a beat.
      timer = window.setTimeout(settle, 120);
      window.addEventListener("resize", settle);
    });
  }

  /** Blank while the window is changing shape around the panel. */
  let settling = $state(false);

  async function togglePanel(open = !panelOpen) {
    // It resizes the window itself, a few lines down, and to the size the menu
    // was borrowing from — so there's nothing to hand back first.
    closeMenu(false);
    // Shutting the panel puts it away entirely: reopening it lands you back in
    // the conversation, not in whichever drawer you were last rummaging in.
    if (!open) view = "chat";

    // Making room for the panel moves the whole window up and to the left, and
    // the window takes the last frame the webview drew along with it: for a
    // frame or two the old picture of the cat is left sitting at the window's
    // new corner, which reads as a second cat in the space the panel is about
    // to fill. An empty window has nothing to carry, so the cat steps off the
    // screen for the length of the resize and comes back in the new shape.
    settling = true;
    panelOpen = open;
    await painted();
    await bridge.setPanel(open);
    await resized();
    settling = false;

    // the window just changed size, so the ground line moved with it
    await cat?.resync();
  }

  /** Every drawer borrows the chat panel's room; this is how one is opened. */
  async function show(next: View) {
    view = next;
    if (!panelOpen) await togglePanel(true);
    else closeMenu();
  }

  async function collar() {
    await show("collar");
    // The collar is where you read your cat's life back, so the numbers on it
    // have to be the ones it's living with, not the ones it opened with.
    await flush();
    memory = await recall(true);
  }

  /** The board, and whatever the cat has left by the door. */
  async function openChores() {
    await show("chores");
    board = await chores.list();
  }

  async function openGifts() {
    await show("gifts");
    pile = await chores.gifts();
    // Looking at the pile is what makes it read: the mice by the cat's paws go
    // away because you came and looked, which is the whole point of them.
    if (pile.some((g) => !g.read)) {
      await chores.read();
      pile = pile.map((g) => ({ ...g, read: true }));
    }
  }

  /** One line, whatever it started as — the bubble is about 30 characters. */
  function oneLine(text: string, max = 58): string {
    const flat = text.replace(/\s+/g, " ").trim();
    return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
  }

  /**
   * A hunt, streaming.
   *
   * Deliberately drives the cat and nothing else: none of this touches
   * `entries`. A chore that fired at 09:00 must not turn up in the middle of
   * the conversation you were having, and the transcript you come back to has
   * to be the one you left.
   */
  function huntEvent(tag: chores.HuntTag, e: bridge.BridgeEvent) {
    // The events themselves are what tell us a hunt has started: the window
    // may have been reloaded, or this may be the first one since launch.
    if (live?.hunt !== tag.id) {
      live = {
        hunt: tag.id,
        chore: tag.chore,
        name: tag.name,
        since: Date.now(),
      };
    }

    switch (e.kind) {
      case "thinking":
        doing ||= "having a think";
        cat?.think();
        break;

      case "text":
        // The cat's own words beat anything we could summarise from a tool
        // call — this is the "reading your inbox" line, in its voice.
        doing = oneLine(e.text);
        cat?.think();
        break;

      case "toolStart":
        doing = oneLine(`${e.tool} · ${e.detail}`);
        cat?.work();
        cat?.spark();
        break;

      case "finished":
      case "failed":
        live = null;
        doing = "";
        if (e.kind === "finished" && e.ok) cat?.done();
        else cat?.rest();
        break;
    }
  }

  /** A hunt is over and there's something on the doormat. */
  function gift(g: chores.Gift) {
    pile = [g, ...pile.filter((old) => old.id !== g.id)];
    // Open on the pile already: you're looking straight at it, so it isn't
    // news waiting to be found.
    if (view === "gifts" && panelOpen) {
      void chores.read([g.id]);
      pile = pile.map((old) => (old.id === g.id ? { ...old, read: true } : old));
    }
  }

  // --- the board ---
  // Every one of these re-reads rather than patching what's on screen: the
  // interval got rounded up, the next slot moved, a run happened while you
  // were typing. What the board says has to be what Rust will actually do.

  async function addChore(draft: chores.Draft) {
    await chores.add(draft);
    board = await chores.list();
  }

  async function saveChore(id: string, patch: Partial<chores.Chore>) {
    await chores.update(id, patch);
    board = await chores.list();
  }

  async function removeChore(id: string) {
    await chores.remove(id);
    board = await chores.list();
  }

  async function toggleChore(id: string, enabled: boolean) {
    await chores.update(id, { enabled });
    board = await chores.list();
  }

  async function runChore(id: string) {
    await chores.runNow(id);
    board = await chores.list();
  }

  async function sweep() {
    await chores.clear();
    pile = [];
  }

  function forget() {
    closeMenu();
    session = null;
    stale = false;
    entries = [];
    remember({ session: null, entries: [] });
  }

  // The panel picks up mid-conversation next launch, so the transcript and the
  // backend behind it are written down as they change. `$state.snapshot` since
  // what crosses to Rust has to be plain data, not a reactive proxy.
  $effect(() => {
    const transcript = $state.snapshot(entries);
    if (loaded) remember({ entries: transcript });
  });

  $effect(() => {
    const backend = backendId;
    if (loaded && backend) remember({ backend });
  });

  /**
   * Changes how the current backend is paid for.
   *
   * Re-detects rather than patching `backends` by hand, so what the panel says
   * about the user's money always comes from the store that Rust will actually
   * read at spawn time — never from an optimistic guess here.
   */
  async function repay(change: () => Promise<void>, note: string) {
    try {
      await change();
      backends = await bridge.detect();
      entries = [...entries, { role: "cat", text: note }];
    } catch (err) {
      entries = [...entries, { role: "error", text: String(err) }];
    }
  }

  function saveKey(key: string) {
    const backend = backends.find((b) => b.id === backendId);
    repay(
      () => bridge.setKey(backendId, key),
      `got it — I'll spend your API key from now on, not your ${
        backend?.subscription ?? "subscription"
      }.`,
    );
  }

  function useSubscription() {
    const backend = backends.find((b) => b.id === backendId);
    repay(
      () => bridge.useSubscription(backendId),
      `back on your ${backend?.subscription ?? "subscription"} — key forgotten.`,
    );
  }

  function onContextMenu(e: MouseEvent) {
    e.preventDefault();
    menuRight = winW - e.clientX;
    menuBottom = winH - e.clientY;
    // Right-clicking again while it's up moves the menu rather than stacking a
    // second one, so only the first open is worth remembering focus for.
    if (!menuOpen) refocus = document.activeElement as HTMLElement | null;
    // A menu that moved is a new menu: nothing carries over highlighted.
    else menuEl?.focus();
    menuOpen = true;
  }

  /**
   * Puts the menu away and hands the keyboard back to wherever it came from,
   * so a stray right-click doesn't cost the composer its caret.
   *
   * `restore` is for callers that are about to size the window themselves:
   * shrinking it back here first would show as a stutter on the way.
   */
  function closeMenu(restore = true) {
    if (!menuOpen) return;
    menuOpen = false;
    if (grown) {
      grown = false;
      if (restore) void bridge.setPanel(panelOpen);
    }
    refocus?.focus();
    refocus = null;
  }

  /** A press anywhere else dismisses the menu, and goes no further. */
  function dismiss(e: PointerEvent) {
    // Except the right button: that one belongs to `onContextMenu`, which
    // moves the menu here instead of closing it and opening it again.
    if (e.button === 2) return;
    e.preventDefault();
    closeMenu();
  }

  /** Arrow keys walk the menu, Enter picks — the buttons handle that part. */
  function onMenuKeydown(e: KeyboardEvent) {
    const items = [...(menuEl?.querySelectorAll("button") ?? [])];
    if (items.length === 0) return;
    // -1 while the menu itself holds focus: nothing is highlighted yet, so the
    // first press lands on whichever end you reached for.
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    const to = (i: number) => {
      e.preventDefault();
      items[i].focus();
    };
    const wrap = (n: number) => (n + items.length) % items.length;

    if (e.key === "ArrowDown") to(at < 0 ? 0 : wrap(at + 1));
    else if (e.key === "ArrowUp") to(at < 0 ? items.length - 1 : wrap(at - 1));
    else if (e.key === "Home") to(0);
    else if (e.key === "End") to(items.length - 1);
  }

  /** The menu takes the keyboard as it appears, the way a Windows one does. */
  function grab(node: HTMLElement) {
    node.focus();
  }

  function play() {
    closeMenu();
    cat?.play();
  }

  function nap() {
    closeMenu();
    cat?.nap();
  }

  /** Another cat, with its own agent session — not a second view of this one. */
  async function anotherCat() {
    closeMenu();
    try {
      await bridge.spawnCat();
    } catch (err) {
      entries = [...entries, { role: "error", text: String(err) }];
    }
  }

  async function quit() {
    closeMenu();
    // Anything still sitting in the debounce would be lost with the window.
    await flush();
    // Closes this cat, or the whole app if it's the last one standing.
    await bridge.dismissCat();
  }

  let debugInfo = $state("");
  $effect(() => {
    if (!panelOpen) return;
    const id = setInterval(() => {
      const m = document.querySelector("main") as HTMLElement;
      const ps = document.querySelector(".panel-slot") as HTMLElement;
      const cs = document.querySelector(".cat-slot") as HTMLElement;
      const p = document.querySelector(".panel") as HTMLElement;
      const hd = document.querySelector("header") as HTMLElement;
      const sel = document.querySelector("select") as HTMLElement;
      const log = document.querySelector(".log") as HTMLElement;
      // min-content width of a clone, measured out of flow
      const probe = document.createElement("div");
      probe.style.cssText =
        "position:absolute;left:-9999px;top:0;width:min-content;visibility:hidden;";
      document.body.appendChild(probe);
      const mc = (e: HTMLElement | null) => {
        if (!e) return "-";
        probe.replaceChildren(e.cloneNode(true));
        const w = Math.round(
          (probe.firstElementChild as HTMLElement).getBoundingClientRect().width,
        );
        return String(w);
      };
      const parts = [`mc pnl ${mc(p)} hdr ${mc(hd)} log ${mc(log)}`];
      const kids = [...(hd?.children ?? [])] as HTMLElement[];
      parts.push(
        "hdrkids " + kids.map((k) => `${k.tagName}:${mc(k)}`).join(" "),
      );
      const logKids = [...(log?.children ?? [])] as HTMLElement[];
      parts.push(
        "logmax " + Math.max(0, ...logKids.map((k) => Number(mc(k)))),
      );
      const form = document.querySelector("form") as HTMLElement;
      parts.push(`form ${mc(form)} sub ${mc(document.querySelector(".sub"))}`);
      probe.remove();
      const box = (e: HTMLElement | null) =>
        e ? `${Math.round(e.getBoundingClientRect().left)}+${Math.round(e.getBoundingClientRect().width)}/${e.scrollWidth}` : "-";
      debugInfo =
        `vp ${window.innerWidth} main ${box(m)} slot ${box(ps)}\n` +
        `cat ${box(cs)} pnl ${box(p)}\n` +
        parts.join("\n");
    }, 300);
    return () => clearInterval(id);
  });
</script>

{#if panelOpen}
  <div id="dbg" style="position:fixed;left:0;top:0;z-index:99;background:#000;color:#0f0;font:9px monospace;padding:2px;white-space:pre;pointer-events:none;">
    {debugInfo}
  </div>
{/if}

<svelte:window
  onkeydown={(e) => e.key === "Escape" && closeMenu()}
  onblur={closeMenu}
  bind:innerWidth={winW}
  bind:innerHeight={winH}
/>

<!-- The cat's own colour tints its panel, so in a colony you can tell at a
     glance whose window you're typing into. -->
<!-- The pointer being anywhere in this window counts as looking at the cat:
     with the panel shut the window *is* the cat, give or take the desktop the
     scene leaves around it for the yarn. -->
<main
  oncontextmenu={onContextMenu}
  onpointerenter={() => (peeking = true)}
  onpointerleave={() => (peeking = false)}
  class:open={panelOpen}
  class:settling
  style="--accent: {coat.accent};"
>
  {#if panelOpen}
    <div class="panel-slot">
      {#if view === "collar"}
        <Collar
          name={who.name}
          coat={who.coat}
          life={memory}
          onname={(name) => dress({ name })}
          oncoat={(coat) => dress({ coat })}
          onclose={() => (view = "chat")}
        />
      {:else if view === "chores"}
        <Chores
          {board}
          {live}
          {cwd}
          onadd={addChore}
          onsave={saveChore}
          onremove={removeChore}
          ontoggle={toggleChore}
          onrun={runChore}
          onclose={() => (view = "chat")}
        />
      {:else if view === "gifts"}
        <Gifts {pile} onclear={sweep} onclose={() => (view = "chat")} />
      {:else}
        <Chat
          {backends}
          bind:backendId
          {entries}
          {busy}
          {agreed}
          name={who.name}
          bind:draft
          onsend={send}
          oncancel={cancel}
          onagree={agree}
          oncollar={collar}
          onhush={() => togglePanel(false)}
          onsavekey={saveKey}
          onusesubscription={useSubscription}
        />
      {/if}
    </div>
  {/if}

  <div class="cat-slot">
    <!-- Sits in the empty desktop the scene leaves above the cat for hearts to
         drift into. Speech has to be readable somewhere or a misheard command
         is just a cat doing something inexplicable. -->
    {#if caption}
      <p class="caption" class:muted={caption.muted}>{caption.text}</p>
    {:else if peek}
      <p class="caption peek">{peek}</p>
    {/if}
    <Cat
      bind:this={cat}
      {panelOpen}
      {coat}
      gifts={unread}
      ondblclick={() => togglePanel()}
    />
  </div>

  {#if menuOpen}
    <!-- Everything that isn't the menu is a way out of it, and the press that
         takes it away is swallowed here rather than reaching the cat — the
         same bargain a Windows menu makes with the window behind it. -->
    <div class="scrim" onpointerdown={dismiss} aria-hidden="true"></div>

    <!-- Kept invisible until it has been measured and the window has been
         grown to hold it, so it never paints half off-window. -->
    <div
      class="menu"
      role="menu"
      tabindex="-1"
      use:grab
      onkeydown={onMenuKeydown}
      onpointerover={(e) =>
        (e.target as HTMLElement).closest("button")?.focus()}
      oncontextmenu={(e) => {
        // Right-clicking the menu itself does nothing, rather than tearing it
        // down and rebuilding it under the cursor.
        e.preventDefault();
        e.stopPropagation();
      }}
      style="left: {menuPos.x}px; top: {menuPos.y}px; visibility: {menuW &&
      !fitting
        ? 'visible'
        : 'hidden'};"
      bind:this={menuEl}
      bind:clientWidth={menuW}
      bind:clientHeight={menuH}
    >
      <button role="menuitem" onclick={() => togglePanel()}>
        &#x1F4AC; {panelOpen ? "hush" : "chat"}
      </button>
      <button role="menuitem" onclick={collar}>&#x1F3F7; collar</button>
      <button role="menuitem" onclick={openChores}>
        &#x1F4CB; chores{board.length > 0 ? ` (${board.length})` : ""}
      </button>
      <!-- The count is the point: a cat that's been out is worth going to look
           at, and this is the same news the pile of mice is telling you. -->
      <button role="menuitem" onclick={openGifts} class:fresh={unread > 0}>
        &#x1F381; gifts{unread > 0 ? ` (${unread})` : ""}
      </button>
      <button role="menuitem" onclick={forget}>&#x1F9F6; forget</button>
      <button role="menuitem" onclick={anotherCat}>&#x1F431; another cat</button>
      <button role="menuitem" onclick={toggleEars}>
        {#if !who.ears}
          &#x1F442; listen
        {:else if deaf}
          &#x1F649; stop listening (no mic)
        {:else}
          &#x1F649; stop listening
        {/if}
      </button>
      <button role="menuitem" onclick={play}>&#x1F43E; play</button>
      <button role="menuitem" onclick={nap}>&#x1F4A4; nap</button>
      <button role="menuitem" onclick={quit}>&#x2715; bye</button>
    </div>
  {/if}
</main>

<style>
  main {
    position: fixed;
    inset: 0;
    /* The cat lives in the bottom-right corner; the panel grows above it. */
    display: grid;
    grid-template-rows: 1fr auto;
    justify-items: end;
  }

  /* Pinned by the corner the cat stands in, not by the window's origin.
     Opening the menu grows the window up and to the left around that same
     corner, so anchoring here instead leaves the cat on the taskbar and puts
     the borrowed room where the menu needs it — above the cat's head. Anchored
     top-left, the cat rode the window's top edge upwards and hung in mid-air. */
  main:not(.open) {
    width: fit-content;
    height: fit-content;
    inset: auto 0 0 auto;
  }

  /* Mid-resize: drawn but invisible, so the frame the window carries into its
     new shape is an empty one. Opacity rather than visibility, so anything the
     panel focuses as it opens still takes the keyboard. */
  main.settling {
    opacity: 0;
  }

  .panel-slot {
    width: 100%;
    min-height: 0;
    padding: 4px 4px 0;
    box-sizing: border-box;
  }

  .panel-slot :global(> *) {
    height: 100%;
  }

  .cat-slot {
    line-height: 0;
    position: relative;
  }

  /* Pinned into the top of the cat's own box rather than the window's, so it
     lands above the cat's head whether or not the chat panel is open. */
  .caption {
    position: absolute;
    left: 4px;
    right: 4px;
    top: 4px;
    z-index: 1;
    margin: 0;
    padding: 4px 6px;
    box-sizing: border-box;
    background: #3a2434;
    border: 1px solid var(--accent, #f5a05c);
    border-radius: 4px;
    font-family: var(--mono);
    font-size: 11px;
    line-height: 1.35;
    text-align: left;
    color: #fff6e8;
    /* Two lines of a long command, then it gives up — the panel has the rest. */
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    /* The cat underneath is the drag handle, and this is only ever a label. */
    pointer-events: none;
  }

  .caption.muted {
    color: #c99ab8;
    border-color: #5a3b52;
  }

  /* Looking over the cat's shoulder. Quieter than speech: this one is here
     because you went looking, not because the cat had something to say. */
  .caption.peek {
    color: #c99ab8;
    border-color: #5a3b52;
    border-style: dashed;
  }

  /* Invisible, but it owns every pixel that isn't the menu — which is what
     makes clicking anywhere else close it and do nothing else. */
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 9;
  }

  .menu {
    /* Fixed, not absolute: `menuPos` is worked out against the window, and
       with the panel shut `main` is only as big as the cat and sits in the
       corner — so it's the wrong thing to measure from. */
    position: fixed;
    display: flex;
    flex-direction: column;
    background: #3a2434;
    border: 2px solid var(--accent, #f5a05c);
    border-radius: 4px;
    /* Last line of defence. The window is grown to fit the menu, but on a
       screen too short to grow into, scrolling the last item into reach beats
       having it cut in half by the taskbar. `scrollHeight` is what asks for
       the room, so capping the height here doesn't stop the window growing. */
    max-height: calc(100vh - 8px);
    overflow-x: hidden;
    overflow-y: auto;
    z-index: 10;
  }

  .menu button {
    all: unset;
    font-family: var(--mono);
    font-size: 13px;
    /* Explicit, so the emoji — which come from a different font with a taller
       line box than the text — can't quietly change how tall the menu is. */
    line-height: 1.35;
    font-weight: bold;
    color: #fff6e8;
    padding: 5px 12px;
    cursor: pointer;
    white-space: nowrap;
  }

  .menu:focus {
    outline: none;
  }

  /* Something is waiting on the doormat. */
  .menu button.fresh {
    color: var(--accent, #f5a05c);
  }

  /* The pointer moves focus as it passes, so the highlight is always the one
     thing Enter would pick — mouse and keyboard never disagree. */
  .menu button:hover,
  .menu button:focus {
    background: var(--accent, #f5a05c);
    color: #3a2434;
  }
</style>
