// Who a particular cat is: its name and its coat.
//
// Identity is per *window*, not per app — every cat in the colony is its own
// creature with its own agent session, so it gets its own name and colour and
// keeps them across restarts.
//
// This is deliberately not part of the Rust memory store, which is per-window
// too but private to that window. Every cat shares one localStorage, and that
// is the whole point: a newly spawned cat can see what its siblings are already
// called and what they're already wearing, and pick something else.
//
// Window labels are reused once a cat is sent home (see `spawn_cat`), and the
// memory store keeps a departed cat's life for exactly that reason — so an
// identity outliving its window is the intended behaviour, not a leak. The slot
// on the taskbar keeps its cat.

import { COATS, DEFAULT_COAT } from "./coats.ts";

export interface Identity {
  name: string;
  /** A {@link COATS} id. Unknown ids fall back to the default coat. */
  coat: string;
  /**
   * Whether this cat answers to its name out loud.
   *
   * Per cat rather than per app, because in a colony it's a way of choosing
   * *which* cat is the one you talk to — the others carry on silently. On by
   * default: a cat you have to switch on before it will listen isn't a pet.
   * The microphone still only opens once the colony has been let loose, which
   * is the app's agreement to make, not this one's.
   */
  ears: boolean;
}

/** Long enough for a proper cat name, short enough for the panel header. */
export const NAME_MAX = 24;

const PREFIX = "purrch.cat.";

/**
 * Names for cats that haven't been named yet. Short, soft, and nothing that
 * reads like a hostname — you should be able to say "ask Biscuit" out loud.
 */
const NAMES = [
  "Miso", "Biscuit", "Pixel", "Mochi", "Tuna", "Waffles", "Bean", "Noodle",
  "Clove", "Pepper", "Gizmo", "Olive", "Sushi", "Widget", "Marzipan", "Pickle",
  "Nutmeg", "Domino", "Bramble", "Tofu", "Comma", "Sprocket", "Cricket", "Fig",
];

function storageKey(label: string): string {
  return PREFIX + label;
}

/** Trim a user-typed name down to something that fits on a collar tag. */
export function cleanName(raw: string): string {
  return Array.from(raw.replace(/\s+/g, " ").trim())
    .slice(0, NAME_MAX)
    .join("");
}

/** Names and coats the rest of the colony is already using. */
function taken(exclude: string): { names: Set<string>; coats: Set<string> } {
  const names = new Set<string>();
  const coats = new Set<string>();
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (!k?.startsWith(PREFIX) || k === storageKey(exclude)) continue;
      const stored = JSON.parse(localStorage.getItem(k) ?? "null");
      if (stored?.name) names.add(String(stored.name).toLowerCase());
      if (stored?.coat) coats.add(String(stored.coat));
    }
  } catch {
    // storage disabled, or somebody hand-edited a value — treat it as an empty
    // colony rather than refusing to name the cat
  }
  return { names, coats };
}

function unused<T>(options: T[], used: Set<unknown>, key: (o: T) => unknown): T {
  const free = options.filter((o) => !used.has(key(o)));
  // More cats than colours is a nice problem to have; repeat rather than fail.
  const from = free.length > 0 ? free : options;
  return from[Math.floor(Math.random() * from.length)];
}

/**
 * An identity for a cat that has never been given one.
 *
 * The first cat keeps the original ginger — it's the one on the icon — while
 * any cat spawned afterwards arrives in a colour nobody else is wearing, so a
 * colony on the taskbar is legible at a glance. Both are only defaults; the
 * collar can change either.
 */
function fresh(label: string): Identity {
  const used = taken(label);
  const name = unused(NAMES, used.names, (n) => n.toLowerCase());
  const coat =
    label === "main" ? DEFAULT_COAT : unused(COATS, used.coats, (c) => c.id).id;
  return { name, coat, ears: true };
}

/** This cat's identity, minting and storing one on first sight. */
export function load(label: string): Identity {
  try {
    const stored = JSON.parse(localStorage.getItem(storageKey(label)) ?? "null");
    const name = cleanName(String(stored?.name ?? ""));
    // A cat stored before it had ears still has them: only having said "stop
    // listening" out loud, in so many words, takes them away.
    if (name) {
      return {
        name,
        coat: String(stored?.coat ?? DEFAULT_COAT),
        ears: stored?.ears !== false,
      };
    }
  } catch {
    // fall through and mint a new one
  }
  const minted = fresh(label);
  save(label, minted);
  return minted;
}

export function save(label: string, identity: Identity): void {
  try {
    localStorage.setItem(storageKey(label), JSON.stringify(identity));
  } catch {
    // private mode / storage disabled — the cat just forgets on restart
  }
}
