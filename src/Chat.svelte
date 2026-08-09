<script lang="ts">
  import type { Backend } from "./lib/bridge.ts";
  import type { Entry } from "./lib/entry.ts";

  let {
    backends,
    backendId = $bindable(),
    entries,
    busy,
    agreed,
    name,
    /** Bound so a file drop can write into the composer from outside. */
    draft = $bindable(""),
    onsend,
    oncancel,
    onagree,
    oncollar,
    onhush,
    onsavekey,
    onusesubscription,
  }: {
    backends: Backend[];
    backendId: string;
    entries: Entry[];
    busy: boolean;
    /** Until the user accepts, the composer stays locked. */
    agreed: boolean;
    /** Whose panel this is — the cat you're talking to, not the app. */
    name: string;
    draft?: string;
    onsend: (text: string) => void;
    oncancel: () => void;
    onagree: () => void;
    oncollar: () => void;
    /** Shut the panel again and leave just the cat. */
    onhush: () => void;
    onsavekey: (key: string) => void;
    onusesubscription: () => void;
  } = $props();

  let log: HTMLDivElement | undefined = $state();

  const active = $derived(backends.find((b) => b.id === backendId));

  /** The billing drawer, and the key on its way into it. */
  let authOpen = $state(false);
  let keyDraft = $state("");

  /** Only some CLIs take a key; the rest own their own credentials. */
  const takesKey = $derived(!!active?.keyEnv);

  /**
   * What this cat is actually spending. Worth stating outright rather than
   * implying — one of these costs money per turn and the other doesn't.
   */
  const paying = $derived.by(() => {
    if (!active) return "";
    if (active.auth === "key" && active.hasKey) return "spending your API key";
    return `runs on your ${active.subscription}`;
  });

  function saveKey() {
    const key = keyDraft.trim();
    if (!key) return;
    keyDraft = "";
    authOpen = false;
    onsavekey(key);
  }

  function useSubscription() {
    keyDraft = "";
    authOpen = false;
    onusesubscription();
  }

  // Pin to the newest entry as the turn streams in.
  $effect(() => {
    entries.length;
    if (log) log.scrollTop = log.scrollHeight;
  });

  function submit(e: Event) {
    e.preventDefault();
    const text = draft.trim();
    if (!text || busy || !agreed) return;
    draft = "";
    onsend(text);
  }

  function onKeydown(e: KeyboardEvent) {
    // Enter sends, Shift+Enter makes a newline.
    if (e.key === "Enter" && !e.shiftKey) submit(e);
    // Escape puts the cat back on its own — the draft is kept for next time.
    else if (e.key === "Escape") onhush();
  }

</script>

<div class="panel">
  <header>
    <!-- The name doubles as the way in to the collar: click who you're talking
         to when you want to change who they are. -->
    <button class="who" onclick={oncollar} title="name and colour">{name}</button>
    <select bind:value={backendId} disabled={busy} title="which subscription to spend">
      {#each backends as b (b.id)}
        <option value={b.id}>{b.label}{b.signedIn ? "" : " (not signed in)"}</option>
      {/each}
      {#if backends.length === 0}
        <option value="">no agent CLI found</option>
      {/if}
    </select>
    <!-- Getting in is a double-click on the cat; getting out shouldn't be
         harder than the thing sitting right there in the corner. -->
    <button class="x" onclick={onhush} title="hush (esc)">&#x2715;</button>
  </header>

  {#if active && takesKey}
    <button
      class="sub as-button"
      class:metered={active.auth === "key" && active.hasKey}
      onclick={() => (authOpen = !authOpen)}
      title="how this backend is paid for"
    >
      {paying}<span class="caret">{authOpen ? "▾" : "▸"}</span>
    </button>
  {:else if active}
    <p class="sub">runs on your {active.subscription}</p>
  {/if}

  {#if active && takesKey && authOpen}
    <div class="auth">
      {#if active.auth === "key" && active.hasKey}
        <p>
          Turns are billed to your key, not your subscription. It's kept in
          Windows Credential Manager — Purrch passes it to {active.label} and
          never sends it anywhere itself.
        </p>
        <button class="link" onclick={useSubscription}>
          go back to your {active.subscription}
        </button>
      {:else}
        <p>
          Rather pay per token than spend your subscription? Paste a key and
          this cat will use it instead.
        </p>
      {/if}
      <div class="row">
        <!-- svelte-ignore a11y_autofocus -->
        <input
          type="password"
          bind:value={keyDraft}
          placeholder={active.keyEnv}
          spellcheck="false"
          autocomplete="off"
          autofocus
          onkeydown={(e) => e.key === "Enter" && saveKey()}
        />
        <button onclick={saveKey} disabled={!keyDraft.trim()}>save</button>
      </div>
    </div>
  {/if}

  {#if !agreed}
    <div class="gate">
      <p class="title">this cat has no leash</p>
      <p>
        Purrch runs your agent with every permission check turned off. It can
        run any command, read or delete any file, and reach the network — on
        your machine, as you, without asking first.
      </p>
      <p>
        It follows whatever it reads, so text in a web page or a file can tell
        it what to do. Watch the tool feed.
      </p>
      <p>
        It also listens. Your microphone stays open for its name — "{name}, open
        my mail" — and nothing else is acted on. The audio never leaves this
        machine; Windows does the hearing. Right-click the cat to stop it.
      </p>
      <button class="agree" onclick={onagree}>I know. let it loose.</button>
    </div>
  {/if}

  <div class="log" class:hidden={!agreed} bind:this={log}>
    {#if entries.length === 0}
      <p class="empty">
        {#if backends.length === 0}
          No agent CLI found. Install Claude Code or Codex, sign in once, and
          Purrch will pick it up — your subscription, your machine.
        {:else}
          Ask me to do something on your PC — here, or out loud: "{name}, open
          my mail".
        {/if}
      </p>
    {/if}
    {#each entries as e, i (i)}
      {#if e.role === "tool"}
        <div class="tool" class:failed={e.ok === false}>
          <span class="name">{e.tool}</span><span class="detail">{e.text}</span>
        </div>
      {:else}
        <div class="msg {e.role}">{e.text}</div>
      {/if}
    {/each}
  </div>

  <form onsubmit={submit} class:hidden={!agreed}>
    <textarea
      bind:value={draft}
      onkeydown={onKeydown}
      rows="2"
      placeholder={busy ? "working…" : "ask the cat"}
      disabled={busy}
    ></textarea>
    {#if busy}
      <button type="button" class="stop" onclick={oncancel}>stop</button>
    {:else}
      <button type="submit" disabled={!draft.trim() || backends.length === 0}>go</button>
    {/if}
  </form>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: #3a2434;
    border: 2px solid var(--accent, #f5a05c);
    border-radius: 6px;
    font-family: var(--mono);
    font-size: 13px;
    color: #fff6e8;
    overflow: hidden;
  }

  header {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px;
    border-bottom: 1px solid #5a3b52;
  }

  .who {
    all: unset;
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: inherit;
    font-size: 14px;
    font-weight: bold;
    color: var(--accent, #f5a05c);
    cursor: pointer;
  }

  .who:hover {
    text-decoration: underline;
  }

  select {
    flex: 1;
    min-width: 0;
    background: #2c1a28;
    color: #fff6e8;
    border: 1px solid #5a3b52;
    border-radius: 3px;
    font-family: inherit;
    font-size: 12px;
    padding: 3px 4px;
  }

  /* Quieter than the panel's orange buttons — it closes, it doesn't act. */
  .x {
    all: unset;
    flex: 0 0 auto;
    padding: 0 3px;
    font-family: inherit;
    font-size: 13px;
    color: #d9b3cc;
    cursor: pointer;
  }

  .x:hover {
    color: #fff6e8;
  }

  .hidden {
    display: none;
  }

  .gate {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 7px;
    line-height: 1.5;
    color: #e8d5e0;
  }

  .gate p {
    margin: 0;
  }

  .gate .title {
    color: #ff7d88;
    font-weight: bold;
    font-size: 14px;
  }

  .gate .agree {
    margin-top: auto;
    padding: 7px;
  }

  .sub {
    margin: 0;
    padding: 4px 7px;
    font-size: 12px;
    color: #d9b3cc;
    border-bottom: 1px solid #5a3b52;
  }

  /* The same line, but it opens the billing drawer. Styled back down from the
     panel's chunky orange button so it still reads as a caption. */
  .sub.as-button {
    all: unset;
    display: block;
    box-sizing: border-box;
    width: 100%;
    padding: 4px 7px;
    font-family: inherit;
    font-size: 12px;
    color: #d9b3cc;
    border-bottom: 1px solid #5a3b52;
    cursor: pointer;
  }

  .sub.as-button:hover {
    color: #fff6e8;
  }

  /* Money is leaving per turn — say so in a colour that isn't the quiet one. */
  .sub.metered {
    color: #f5a05c;
  }

  .caret {
    margin-left: 4px;
    opacity: 0.7;
  }

  .auth {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 7px;
    background: #2c1a28;
    border-bottom: 1px solid #5a3b52;
    font-size: 12px;
    line-height: 1.5;
    color: #d9b3cc;
  }

  .auth p {
    margin: 0;
  }

  .auth .row {
    display: flex;
    gap: 4px;
  }

  .auth input {
    flex: 1;
    min-width: 0;
    background: #3a2434;
    color: #fff6e8;
    border: 1px solid #5a3b52;
    border-radius: 3px;
    font-family: inherit;
    font-size: 13px;
    padding: 5px;
  }

  .auth input:focus {
    outline: none;
    border-color: #f5a05c;
  }

  .auth .link {
    all: unset;
    align-self: flex-start;
    font-family: inherit;
    font-size: 12px;
    color: #b6cfe8;
    text-decoration: underline;
    cursor: pointer;
  }

  .auth .link:hover {
    color: #fff6e8;
  }

  .log {
    flex: 1;
    overflow-y: auto;
    padding: 6px 7px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .empty {
    margin: 0;
    color: #d9b3cc;
    line-height: 1.5;
  }

  .msg {
    white-space: pre-wrap;
    word-break: break-word;
    line-height: 1.45;
  }

  .msg.you {
    color: var(--accent, #f5a05c);
  }

  .msg.you::before {
    content: "> ";
  }

  .msg.error {
    color: #ff7d88;
  }

  /* The tool feed is the thing you're told to watch, so it stays legible
     rather than being dimmed into the background. */
  .tool {
    display: flex;
    gap: 5px;
    font-size: 12px;
    color: #b6cfe8;
  }

  .tool.failed {
    color: #ff7d88;
  }

  .tool .name {
    flex: 0 0 auto;
    font-weight: bold;
  }

  .tool .detail {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  form {
    display: flex;
    gap: 4px;
    padding: 5px;
    border-top: 1px solid #5a3b52;
  }

  textarea {
    flex: 1;
    min-width: 0;
    resize: none;
    background: #2c1a28;
    color: #fff6e8;
    border: 1px solid #5a3b52;
    border-radius: 3px;
    font-family: inherit;
    font-size: 13px;
    padding: 5px;
  }

  textarea:focus {
    outline: none;
    border-color: var(--accent, #f5a05c);
  }

  button {
    all: unset;
    flex: 0 0 auto;
    padding: 0 10px;
    background: var(--accent, #f5a05c);
    color: #3a2434;
    font-family: inherit;
    font-size: 13px;
    font-weight: bold;
    border-radius: 3px;
    cursor: pointer;
    text-align: center;
  }

  button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  button.stop {
    background: #e8616e;
    color: #fff6e8;
  }
</style>
