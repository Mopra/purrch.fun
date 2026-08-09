<script lang="ts">
  import { COATS, coatById, type Coat } from "./lib/coats.ts";
  import { NAME_MAX } from "./lib/identity.ts";
  import {
    renderScene,
    IDLE_POSE,
    SCENE_W,
    SCENE_H,
    CAT_X,
    CAT_Y,
  } from "./lib/render.ts";
  import { CAT_W, CAT_H } from "./lib/catArt.ts";
  import type { CatMemory } from "./lib/memory.ts";

  let {
    name,
    coat,
    life = null,
    onname,
    oncoat,
    onclose,
  }: {
    name: string;
    coat: string;
    /** What this cat has been through, or null outside the app shell. */
    life?: CatMemory | null;
    onname: (name: string) => void;
    oncoat: (coat: string) => void;
    onclose: () => void;
  } = $props();

  /**
   * How long you've had this cat. Days once it's had a night, hours after an
   * afternoon — never "0 days", which reads like it hasn't happened yet.
   */
  function age(bornAt: number): string {
    const plural = (n: number, unit: string) =>
      `${n} ${unit}${n === 1 ? "" : "s"} old`;
    const mins = Math.floor((Date.now() - bornAt) / 60000);
    if (mins >= 1440) return plural(Math.floor(mins / 1440), "day");
    if (mins >= 60) return plural(Math.floor(mins / 60), "hour");
    return plural(Math.max(1, mins), "minute");
  }

  /** Live text, committed to the cat on blur or Enter. */
  let typed = $state("");

  // Starts on whatever the cat is called now, and follows a rename that
  // happened anywhere else. Typing doesn't re-run this — only `name` does.
  $effect(() => {
    typed = name;
  });

  const SWATCH = 3; // scene pixels per swatch pixel

  /**
   * Draws one coat's cat into a swatch.
   *
   * The scene is wider than the cat — it has to be, for the yarn to roll in
   * from off-body — so the buffer is cropped back to the cat's own box, or
   * every swatch would be mostly empty space.
   */
  function preview(canvas: HTMLCanvasElement, c: Coat) {
    const ctx = canvas.getContext("2d")!;
    const off = document.createElement("canvas");
    off.width = SCENE_W;
    off.height = SCENE_H;
    const offCtx = off.getContext("2d")!;

    const paint = (c: Coat) => {
      const pose = { ...IDLE_POSE, tail: "up" as const };
      const buf = renderScene(pose, [], null, c);
      offCtx.putImageData(new ImageData(buf, SCENE_W, SCENE_H), 0, 0);
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(
        off,
        CAT_X,
        CAT_Y,
        CAT_W,
        CAT_H,
        0,
        0,
        canvas.width,
        canvas.height,
      );
    };

    paint(c);
    return { update: paint };
  }

  function commit() {
    const next = typed.trim();
    // An empty box means "I changed my mind", not "this cat has no name".
    if (!next) {
      typed = name;
      return;
    }
    if (next !== name) onname(next);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      commit();
      (e.currentTarget as HTMLInputElement).blur();
    } else if (e.key === "Escape") {
      typed = name;
      (e.currentTarget as HTMLInputElement).blur();
    }
  }
</script>

<div class="panel">
  <header>
    <span class="title">collar</span>
    <button class="x" onclick={onclose} title="back to the chat">&#x2715;</button>
  </header>

  <div class="body">
    <label class="field">
      <span>name</span>
      <input
        bind:value={typed}
        onblur={commit}
        onkeydown={onKeydown}
        maxlength={NAME_MAX}
        spellcheck="false"
        placeholder="name your cat"
      />
    </label>

    <p class="hint">The cat answers to this, and knows it as its own name.</p>

    {#if life}
      <!-- Everything this cat has been through, kept across restarts. It is
           the proof it's the same cat and not a new one wearing its collar. -->
      <div class="life">
        <span class="age">{age(life.bornAt)}</span>
        <span>&#x2665; {life.pets} pets</span>
        <span>&#x1F4A4; {life.naps} naps</span>
        <span>&#x1F9F6; {life.plays} games</span>
        <span>&#x1F43E; {life.turns} jobs</span>
        <span>&#x1F527; {life.tools} tools</span>
      </div>
    {/if}

    <span class="field-label">coat</span>
    <div class="coats">
      {#each COATS as c (c.id)}
        <button
          class="swatch"
          class:on={c.id === coat}
          style="--swatch: {c.accent};"
          onclick={() => oncoat(c.id)}
        >
          <canvas
            use:preview={c}
            width={CAT_W * SWATCH}
            height={CAT_H * SWATCH}
          ></canvas>
          <span>{c.label}</span>
        </button>
      {/each}
    </div>
  </div>

  <footer>
    <button class="done" onclick={onclose}>done</button>
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
    font-size: 13px;
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
    color: #d9b3cc;
    cursor: pointer;
  }

  .x:hover {
    color: #fff6e8;
  }

  .body {
    flex: 1;
    overflow-y: auto;
    padding: 8px 7px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .field span,
  .field-label {
    flex: 0 0 auto;
    color: #d9b3cc;
  }

  .field-label {
    margin-top: 4px;
  }

  input {
    flex: 1;
    min-width: 0;
    background: #2c1a28;
    color: #fff6e8;
    border: 1px solid #5a3b52;
    border-radius: 3px;
    font-family: inherit;
    font-size: 13px;
    padding: 5px;
  }

  input:focus {
    outline: none;
    border-color: var(--accent, #f5a05c);
  }

  .hint {
    margin: 0;
    color: #c3a1ba;
    line-height: 1.45;
  }

  .life {
    display: flex;
    flex-wrap: wrap;
    gap: 3px 10px;
    padding: 6px 7px;
    background: #2c1a28;
    border-radius: 4px;
    font-size: 12px;
    color: #d9b3cc;
  }

  .life .age {
    flex: 1 0 100%;
    color: var(--accent, #f5a05c);
    font-weight: bold;
  }

  .coats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 5px;
  }

  .swatch {
    all: unset;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 4px 2px 3px;
    border: 1px solid transparent;
    border-radius: 4px;
    cursor: pointer;
    color: #d9b3cc;
    font-size: 12px;
  }

  .swatch:hover {
    background: #4a2f44;
  }

  .swatch.on {
    border-color: var(--swatch);
    background: #4a2f44;
    color: var(--swatch);
  }

  .swatch canvas {
    display: block;
    image-rendering: pixelated;
  }

  footer {
    padding: 5px;
    border-top: 1px solid #5a3b52;
  }

  .done {
    all: unset;
    display: block;
    padding: 6px;
    background: var(--accent, #f5a05c);
    color: #3a2434;
    font-family: inherit;
    font-size: 13px;
    font-weight: bold;
    border-radius: 3px;
    cursor: pointer;
    text-align: center;
  }
</style>
