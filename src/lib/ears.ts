// Typed client for the colony's hearing.
//
// Same shape as `bridge.ts`: a thin wrapper over Tauri IPC that degrades to a
// no-op in a plain browser, so `npm run dev` still runs the cat.
//
// The switch is here rather than in Rust because it isn't only "does the user
// want voice" — it's also whether they have agreed to let a cat act without
// asking, which is the frontend's to know. A cat that hasn't been let loose
// must not have its microphone open.

export type EarEvent =
  /** The mic is open and the colony is being listened for. */
  | { kind: "listening" }
  /** It isn't, and here's something sayable about why. */
  | { kind: "deaf"; message: string }
  /**
   * Off fetching the half of the ear that can spell. Worth saying: it's a
   * large download, and until it lands the cats are noticeably worse at
   * understanding you rather than broken.
   */
  | { kind: "learning"; message: string }
  /** This cat's name is being said right now. */
  | { kind: "perked" }
  /** What was said after the name. Empty means it was only called. */
  | { kind: "heard"; text: string }
  /** It was called, then the rest didn't come out as anything. */
  | { kind: "missed" };

const EVENT = "purrch://ears";

async function tauri() {
  if (!("__TAURI_INTERNALS__" in window)) return null;
  return await import("@tauri-apps/api/core");
}

/**
 * Tells the ear what this cat is called and whether it wants to be heard.
 *
 * Cheap to call and safe to call often — the microphone is only reopened when
 * the colony as a whole sounds different from the one being listened for.
 */
export async function tune(name: string, listening: boolean): Promise<void> {
  const api = await tauri();
  await api?.invoke("ears_tune", { name, listening });
}

/** Subscribes to the ear. Returns an unlisten function. */
export async function onEar(
  handler: (event: EarEvent) => void,
): Promise<() => void> {
  if (!("__TAURI_INTERNALS__" in window)) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return await listen<EarEvent>(EVENT, (e) => handler(e.payload));
}

/**
 * Things you say to a cat that's already working, meaning "drop it".
 *
 * Spoken commands are otherwise ignored mid-turn, the same way the composer is
 * disabled — but there has to be a way to call one off without reaching for the
 * panel, or a cat that misheard you is a cat you have to go and stop by hand.
 */
const STOP: string[] = [
  "stop",
  "stop it",
  "stop that",
  "cancel",
  "cancel that",
  "never mind",
  "nevermind",
  "forget it",
  "leave it",
];

export function isStop(text: string): boolean {
  const flat = text
    .toLowerCase()
    .replace(/[^a-z0-9' ]+/g, " ")
    .split(/\s+/)
    .filter(Boolean)
    .join(" ");
  return STOP.includes(flat);
}
