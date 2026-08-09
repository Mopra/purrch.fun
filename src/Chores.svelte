<script lang="ts">
  // The chore board. What this cat does when you aren't asking it anything.
  //
  // Borrows the chat panel's room, like the collar does. Everything is one
  // scrolling column: the list, and one form that doubles as add and edit —
  // 380 pixels is not enough for two of anything.

  import * as chores from "./lib/chores.ts";

  let {
    board,
    live = null,
    cwd = null,
    onadd,
    onsave,
    onremove,
    ontoggle,
    onrun,
    onclose,
  }: {
    board: chores.Chore[];
    /** The chore this cat is out on right now, if any. */
    live?: chores.Live | null;
    /**
     * Where the cat is standing, which is where a new chore will stand too.
     * Drop a folder on the cat to change it — a chore that watches a repo has
     * to be written down while the cat is in that repo.
     */
    cwd?: string | null;
    onadd: (draft: chores.Draft) => void;
    onsave: (id: string, patch: Partial<chores.Chore>) => void;
    onremove: (id: string) => void;
    ontoggle: (id: string, enabled: boolean) => void;
    onrun: (id: string) => void;
    onclose: () => void;
  } = $props();

  /** The chore being edited, or `""` for the blank one at the bottom. */
  let editing = $state<string | null>(null);

  let name = $state("");
  let prompt = $state("");
  let everyMs = $state(60 * 60 * 1000);
  let catchUp = $state(false);
  /** Fixed once the chore exists — a chore that moved folders is a new chore. */
  let where = $state<string | null>(null);

  /** Confirm-before-delete, which is one click and one glance rather than a dialog. */
  let confirming = $state<string | null>(null);

  function blank() {
    editing = "";
    name = "";
    prompt = "";
    everyMs = 60 * 60 * 1000;
    catchUp = false;
    where = cwd;
    confirming = null;
  }

  function open(chore: chores.Chore) {
    editing = chore.id;
    name = chore.name;
    prompt = chore.prompt;
    everyMs = chore.everyMs;
    catchUp = chore.catchUp;
    where = chore.cwd;
    confirming = null;
  }

  /** Just the last couple of folders — the panel is 380 pixels wide. */
  function shortPath(path: string): string {
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts.length <= 2 ? path : `…\\${parts.slice(-2).join("\\")}`;
  }

  function cancel() {
    editing = null;
    confirming = null;
  }

  function commit() {
    const job = prompt.trim();
    if (!job) return;
    // A chore with no name is still a chore; it just gets called by what it
    // does, trimmed to something that fits on a row.
    const called = name.trim() || job.split(/\s+/).slice(0, 4).join(" ");

    if (editing) {
      onsave(editing, { name: called, prompt: job, everyMs, catchUp });
    } else {
      onadd({ name: called, prompt: job, everyMs, catchUp, cwd: where });
    }
    editing = null;
  }

  /** Ctrl+Enter commits; the prompt box wants plain Enter for newlines. */
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      commit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancel();
    }
  }

  /** Ticks so "in 12 min" doesn't sit there being wrong. */
  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(id);
  });

  /** What a row says under its name. */
  function subtitle(chore: chores.Chore): string {
    const every = chores.everyLabel(chore.everyMs);
    if (!chore.enabled) return `${every} · paused`;
    if (live?.chore === chore.id) return `${every} · out now`;
    return `${every} · ${chores.when(chore.nextDue, now)}`;
  }
</script>

<div class="panel">
  <header>
    <span class="title">chores</span>
    <button class="x" onclick={onclose} title="back to the chat">&#x2715;</button
    >
  </header>

  <div class="body">
    {#if board.length === 0 && editing === null}
      <p class="empty">
        Nothing on the board. Give this cat something to go and check on while
        you're doing something else — it'll bring back whatever it finds.
      </p>
    {/if}

    {#each board as chore (chore.id)}
      {#if editing === chore.id}
        <!-- The form takes the row's place, so the list doesn't jump. -->
        <div class="form">
          <input
            bind:value={name}
            placeholder="what to call it"
            spellcheck="false"
            onkeydown={onKeydown}
          />
          <textarea
            bind:value={prompt}
            rows="4"
            placeholder="what to do"
            onkeydown={onKeydown}
          ></textarea>
          <div class="row">
            <select bind:value={everyMs}>
              {#each chores.EVERY as e (e.ms)}
                <option value={e.ms}>every {e.label}</option>
              {/each}
            </select>
            <label class="check" title="run a slot the PC slept through">
              <input type="checkbox" bind:checked={catchUp} />
              catch up
            </label>
          </div>
          {#if where}
            <span class="where" title={where}>
              stands in {shortPath(where)}
            </span>
          {/if}
          <div class="row end">
            {#if confirming === chore.id}
              <button class="danger" onclick={() => onremove(chore.id)}>
                really delete
              </button>
            {:else}
              <button class="quiet" onclick={() => (confirming = chore.id)}>
                delete
              </button>
            {/if}
            <button class="quiet" onclick={cancel}>cancel</button>
            <button onclick={commit} disabled={!prompt.trim()}>save</button>
          </div>
        </div>
      {:else}
        <div class="chore" class:off={!chore.enabled}>
          <button
            class="what"
            onclick={() => open(chore)}
            title="edit this chore"
          >
            <span class="name">{chore.name}</span>
            <span class="sub">
              {subtitle(chore)}{chore.runs > 0 ? ` · ${chore.runs} runs` : ""}
            </span>
          </button>
          <button
            class="icon"
            onclick={() => onrun(chore.id)}
            title="go now"
            disabled={live?.chore === chore.id}
          >
            &#x25B6;
          </button>
          <button
            class="icon"
            onclick={() => ontoggle(chore.id, !chore.enabled)}
            title={chore.enabled ? "pause" : "resume"}
          >
            {chore.enabled ? "⏸" : "⏵"}
          </button>
        </div>
      {/if}
    {/each}

    {#if editing === ""}
      <div class="form">
        <!-- svelte-ignore a11y_autofocus -->
        <input
          bind:value={name}
          placeholder="what to call it"
          spellcheck="false"
          autofocus
          onkeydown={onKeydown}
        />
        <textarea
          bind:value={prompt}
          rows="4"
          placeholder="what to do — e.g. check the downloads folder and file anything that's been sitting there a week"
          onkeydown={onKeydown}
        ></textarea>
        <div class="row">
          <select bind:value={everyMs}>
            {#each chores.EVERY as e (e.ms)}
              <option value={e.ms}>every {e.label}</option>
            {/each}
          </select>
          <label class="check" title="run a slot the PC slept through">
            <input type="checkbox" bind:checked={catchUp} />
            catch up
          </label>
        </div>
        {#if where}
          <span class="where" title={where}>stands in {shortPath(where)}</span>
        {/if}
        <div class="row end">
          <button class="quiet" onclick={cancel}>cancel</button>
          <button onclick={commit} disabled={!prompt.trim()}>add</button>
        </div>
      </div>
    {/if}

    <p class="hint">
      Every chore is a turn against your subscription, whether it finds anything
      or not. Slower is cheaper, and most checks come back with nothing.
    </p>
  </div>

  <footer>
    {#if editing === null}
      <button class="done" onclick={blank}>+ new chore</button>
    {:else}
      <button class="done" onclick={cancel}>back to the board</button>
    {/if}
  </footer>
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
    font-size: 11px;
    color: #fff6e8;
    overflow: hidden;
  }

  header {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 7px;
    border-bottom: 1px solid #5a3b52;
  }

  .title {
    flex: 1;
    font-weight: bold;
    color: var(--accent, #f5a05c);
  }

  .x {
    all: unset;
    padding: 0 3px;
    color: #c99ab8;
    cursor: pointer;
  }

  .x:hover {
    color: #fff6e8;
  }

  .body {
    flex: 1;
    overflow-y: auto;
    padding: 7px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .empty,
  .hint {
    margin: 0;
    color: #8f6f86;
    line-height: 1.5;
  }

  .hint {
    margin-top: auto;
    padding-top: 6px;
    font-size: 10px;
  }

  .chore {
    display: flex;
    align-items: stretch;
    gap: 2px;
    background: #2c1a28;
    border-radius: 4px;
    overflow: hidden;
  }

  /* A paused chore is still on the board; it just isn't going anywhere. */
  .chore.off {
    opacity: 0.55;
  }

  .what {
    all: unset;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px 6px;
    cursor: pointer;
  }

  .what:hover {
    background: #4a2f44;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: bold;
  }

  .sub {
    font-size: 10px;
    color: #c99ab8;
  }

  .icon {
    all: unset;
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    padding: 0 7px;
    color: #c99ab8;
    cursor: pointer;
  }

  .icon:hover {
    background: #4a2f44;
    color: var(--accent, #f5a05c);
  }

  .icon:disabled {
    opacity: 0.3;
    cursor: default;
    background: none;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 6px;
    background: #2c1a28;
    border: 1px solid var(--accent, #f5a05c);
    border-radius: 4px;
  }

  .row {
    display: flex;
    gap: 5px;
    align-items: center;
  }

  .row.end {
    justify-content: flex-end;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    color: #c99ab8;
    cursor: pointer;
  }

  .check input {
    margin: 0;
    accent-color: var(--accent, #f5a05c);
  }

  .where {
    font-size: 10px;
    color: #8f6f86;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  input:not([type]),
  textarea,
  select {
    min-width: 0;
    background: #3a2434;
    color: #fff6e8;
    border: 1px solid #5a3b52;
    border-radius: 3px;
    font-family: inherit;
    font-size: 11px;
    padding: 4px;
  }

  textarea {
    resize: none;
    line-height: 1.45;
  }

  select {
    flex: 1;
  }

  input:focus,
  textarea:focus,
  select:focus {
    outline: none;
    border-color: var(--accent, #f5a05c);
  }

  button {
    all: unset;
    flex: 0 0 auto;
    padding: 4px 9px;
    background: var(--accent, #f5a05c);
    color: #3a2434;
    font-family: inherit;
    font-size: 11px;
    font-weight: bold;
    border-radius: 3px;
    cursor: pointer;
    text-align: center;
  }

  button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  button.quiet {
    background: none;
    color: #c99ab8;
    font-weight: normal;
  }

  button.quiet:hover {
    color: #fff6e8;
  }

  button.danger {
    background: #e8616e;
    color: #fff6e8;
  }

  footer {
    padding: 5px;
    border-top: 1px solid #5a3b52;
  }

  .done {
    display: block;
    width: 100%;
    box-sizing: border-box;
    padding: 6px;
  }
</style>
