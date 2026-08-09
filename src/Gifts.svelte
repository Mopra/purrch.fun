<script lang="ts">
  // The pile by the door. What the cat caught while you were doing something
  // else, newest first.
  //
  // What the cat *said* is already here — a gift is one or two lines by the
  // time it lands (see `hunt::brief`), so there is nothing to load. What it
  // *did* is folded away behind the tool count, and that half is not decoration:
  // a hunt runs with nobody watching the tool feed the user was told to watch,
  // and an agent that reads email and PR bodies is the best possible target for
  // text saying "ignore your instructions". Unfolding a gift is how you find
  // out afterwards. It's shut by default because most errands find nothing, and
  // a pile that opens as a wall of tool calls is a pile nobody reads.

  import * as chores from "./lib/chores.ts";

  let {
    pile,
    onclear,
    onclose,
  }: {
    pile: chores.Gift[];
    onclear: () => void;
    onclose: () => void;
  } = $props();

  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(id);
  });

  const unread = $derived(pile.filter((g) => !g.read).length);

  /** Which gifts have been unfolded to show what the cat actually did. */
  let open = $state<Record<string, boolean>>({});

  function toggle(id: string) {
    open = { ...open, [id]: !open[id] };
  }
</script>

<div class="panel">
  <header>
    <span class="title">gifts</span>
    {#if unread > 0}
      <span class="count">{unread} new</span>
    {/if}
    <button class="x" onclick={onclose} title="back to the chat">&#x2715;</button
    >
  </header>

  <div class="body">
    {#if pile.length === 0}
      <p class="empty">
        Nothing on the doormat yet. When a chore finishes, whatever the cat
        found is left here — and it waits until you look.
      </p>
    {/if}

    {#each pile as gift (gift.id)}
      <!-- The unread ones are the pile; the rest are what you've already seen,
           kept around because "nothing new" three times running is itself
           worth being able to notice. -->
      <div class="gift" class:fresh={!gift.read} class:failed={!gift.ok}>
        <div class="head">
          <span class="from">{gift.choreName}</span>
          <span class="at">{chores.when(gift.at, now)}</span>
        </div>
        <p class="what">{gift.text}</p>
        {#if gift.tools > 0}
          <button
            class="tools"
            onclick={() => toggle(gift.id)}
            title="what it actually did"
            aria-expanded={!!open[gift.id]}
          >
            &#x1F527; {gift.tools} tool{gift.tools === 1 ? "" : "s"}
            <span class="caret">{open[gift.id] ? "▾" : "▸"}</span>
          </button>
        {/if}

        {#if open[gift.id]}
          <ol class="trail">
            <!-- Only the tail is kept for a long hunt, so say so rather than
                 letting the list quietly disagree with the count above it. -->
            {#if gift.tools > gift.trail.length}
              <li class="elided">
                …{gift.tools - gift.trail.length} earlier
              </li>
            {/if}
            {#each gift.trail as step, i (i)}
              <li class:failed={step.ok === false} class:open={step.ok === null}>
                <span class="tool">{step.tool}</span>
                <span class="detail" title={step.detail}>{step.detail}</span>
              </li>
            {/each}
          </ol>
        {/if}
      </div>
    {/each}
  </div>

  <footer>
    <button class="done" onclick={onclear} disabled={pile.length === 0}>
      sweep the pile
    </button>
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
    gap: 5px;
    padding: 5px 7px;
    border-bottom: 1px solid #5a3b52;
  }

  .title {
    flex: 1;
    font-weight: bold;
    color: var(--accent, #f5a05c);
  }

  .count {
    font-size: 10px;
    color: #3a2434;
    background: var(--accent, #f5a05c);
    border-radius: 8px;
    padding: 1px 6px;
    font-weight: bold;
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

  .empty {
    margin: 0;
    color: #8f6f86;
    line-height: 1.5;
  }

  .gift {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 5px 6px;
    background: #2c1a28;
    border-radius: 4px;
    /* The stripe is the "new" mark: quiet enough to read past, obvious enough
       to count at a glance. */
    border-left: 2px solid transparent;
  }

  .gift.fresh {
    border-left-color: var(--accent, #f5a05c);
  }

  .gift.failed {
    border-left-color: #e8616e;
  }

  .head {
    display: flex;
    gap: 6px;
    align-items: baseline;
    font-size: 10px;
  }

  .from {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: bold;
    color: #c99ab8;
  }

  .gift.failed .from {
    color: #e8616e;
  }

  .at {
    flex: 0 0 auto;
    color: #8f6f86;
  }

  .what {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
    line-height: 1.45;
  }

  /* The way in to what the cat actually did. Styled as the quiet label it
     already was, so the pile still reads as a pile rather than as a log. */
  .tools {
    all: unset;
    align-self: flex-start;
    font-family: inherit;
    font-size: 10px;
    color: #9db8d6;
    opacity: 0.85;
    cursor: pointer;
  }

  .tools:hover {
    opacity: 1;
    text-decoration: underline;
  }

  .caret {
    opacity: 0.7;
  }

  .trail {
    margin: 3px 0 0;
    padding: 4px 0 0;
    list-style: none;
    border-top: 1px solid #4a2f44;
    display: flex;
    flex-direction: column;
    gap: 1px;
    font-size: 10px;
    color: #9db8d6;
  }

  .trail li {
    display: flex;
    gap: 5px;
    min-width: 0;
  }

  /* A tool that came back an error, and one the hunt died still holding —
     different things, and the second is the more interesting of the two. */
  .trail li.failed {
    color: #e8616e;
  }

  .trail li.open {
    color: #e8c07d;
  }

  .trail .tool {
    flex: 0 0 auto;
    font-weight: bold;
  }

  .trail .detail {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .trail .elided {
    color: #8f6f86;
    font-style: italic;
  }

  footer {
    padding: 5px;
    border-top: 1px solid #5a3b52;
  }

  .done {
    all: unset;
    display: block;
    box-sizing: border-box;
    width: 100%;
    padding: 6px;
    background: var(--accent, #f5a05c);
    color: #3a2434;
    font-family: inherit;
    font-size: 11px;
    font-weight: bold;
    border-radius: 3px;
    cursor: pointer;
    text-align: center;
  }

  .done:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
