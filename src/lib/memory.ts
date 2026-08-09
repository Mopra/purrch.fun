// What this cat remembers between launches.
//
// The store itself lives in Rust, one file for the whole colony keyed by window
// label (see `memory.rs`). This side is a debounced writer: the cat reports
// every step it takes and every line of chat, and that would be a file write
// per animation frame if the patches weren't coalesced first.
//
// Patches are shallow and merged field by field on the other side, so the two
// writers — the animation loop with the position and mood, the panel with the
// conversation — can each send only what they own without reading the other's
// fields back first.
//
// As with the bridge, a plain browser (`npm run dev`) has nowhere to keep any
// of this, so the cat lives entirely in the moment there.

import type { Entry } from "./entry.ts";

export interface CatMemory {
  /** Given at birth and kept — this is the cat you know, not "the cat". */
  name: string;
  /** ms since the epoch, both of these. */
  bornAt: number;
  lastSeen: number;
  /** Last place it stood, in physical pixels on the virtual desktop. */
  x: number | null;
  y: number | null;
  asleep: boolean;
  pets: number;
  naps: number;
  plays: number;
  /** Turns its agent has finished for you, and tools picked up doing them. */
  turns: number;
  tools: number;
  backend: string | null;
  cwd: string | null;
  /** Agent session, so the conversation resumes instead of restarting. */
  session: string | null;
  entries: Entry[];
}

/** How long the writer will sit on a patch before committing it. */
const DEBOUNCE_MS = 1200;

async function tauri() {
  if (!("__TAURI_INTERNALS__" in window)) return null;
  return await import("@tauri-apps/api/core");
}

let cached: Promise<CatMemory | null> | null = null;

/**
 * Everything this cat remembers. Cached, since it's the same answer for the
 * lifetime of the window — pass `fresh` to re-read it, which is only worth
 * doing to show the user their cat's current stats.
 */
export function recall(fresh = false): Promise<CatMemory | null> {
  if (fresh) cached = null;
  cached ??= (async () => {
    const api = await tauri();
    if (!api) return null;
    try {
      return await api.invoke<CatMemory>("recall");
    } catch {
      // Nothing to remember beats refusing to open.
      return null;
    }
  })();
  return cached;
}

let pending: Partial<CatMemory> | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;

/**
 * Commits part of the cat's memory, eventually. Cheap to call on every tick:
 * patches pile up and go out together at most once per {@link DEBOUNCE_MS}.
 */
export function remember(patch: Partial<CatMemory>): void {
  pending = { ...pending, ...patch };
  // Deliberately not restarting the timer — a cat that walks for a minute
  // straight should still be writing down where it is along the way.
  timer ??= setTimeout(() => void flush(), DEBOUNCE_MS);
}

/** Commits now. Worth awaiting before the window goes away. */
export async function flush(): Promise<void> {
  clearTimeout(timer);
  timer = undefined;
  const patch = pending;
  pending = null;
  if (!patch) return;
  const api = await tauri();
  if (!api) return;
  try {
    await api.invoke("remember", { patch });
  } catch {
    // A cat that can't write its memory down still has to keep living.
  }
}

// Closed by the OS, a reload, a shutdown — anything that isn't the "bye" menu
// item, which flushes on its own and can actually wait for it.
window.addEventListener("pagehide", () => void flush());
