// The colony as a whole: what it's been allowed to do, and what it's spent
// doing it.
//
// Both of these used to be the frontend's business and shouldn't have been.
// The agreement lived in `localStorage`, which meant it guarded exactly the one
// screen that read it — the composer — while the chore board, the clock and the
// microphone all reached an agent without passing through it. It lives in Rust
// now (`colony.rs`), which is where every one of those routes actually ends.
//
// This file is the read side of that, plus the two switches that belong to the
// app rather than to any one cat: whether Purrch comes back at login, and
// whether there's a newer one.
//
// As everywhere else, a plain browser (`npm run dev`) has none of this behind
// it. The defaults there are deliberately the *permissive* ones — outside the
// app shell there is no bridge to send anything to, so a gate would only be a
// gate across an empty doorway.

async function tauri() {
  if (!("__TAURI_INTERNALS__" in window)) return null;
  return await import("@tauri-apps/api/core");
}

export interface Settings {
  /** The user has agreed that the cats run with no permission checks. */
  unleashed: boolean;
  /** Hunts one cat may run in a rolling day. */
  dailyHunts: number;
}

/** One cat's budget, right now. Mirrors `colony::Spend`. */
export interface Spend {
  today: number;
  cap: number;
  /** When a slot next frees up, if the cat is out of them. */
  nextFree: number | null;
}

const OPEN: Settings = { unleashed: true, dailyHunts: 40 };

export async function settings(): Promise<Settings> {
  const api = await tauri();
  if (!api) return OPEN;
  return await api.invoke<Settings>("colony_settings");
}

/**
 * The user has read the gate and let the cats loose.
 *
 * One way, on purpose — see `Colony::unleash`. There is no "put it back on":
 * the agreement is about what the app *is*, and a switch implying you could
 * recall an agent already running would be a lie.
 */
export async function unleash(): Promise<Settings> {
  const api = await tauri();
  if (!api) return OPEN;
  return await api.invoke<Settings>("colony_unleash");
}

export async function setBudget(hunts: number): Promise<Settings> {
  const api = await tauri();
  if (!api) return { ...OPEN, dailyHunts: hunts };
  return await api.invoke<Settings>("colony_set_budget", { hunts });
}

/** What this cat has spent of that budget in the last 24 hours. */
export async function spend(): Promise<Spend> {
  const api = await tauri();
  if (!api) return { today: 0, cap: OPEN.dailyHunts, nextFree: null };
  return await api.invoke<Spend>("colony_spend");
}

/** The cap choices the board offers, in hunts per rolling day. */
export const BUDGETS: { label: string; hunts: number }[] = [
  { label: "10 a day", hunts: 10 },
  { label: "25 a day", hunts: 25 },
  { label: "40 a day", hunts: 40 },
  { label: "80 a day", hunts: 80 },
  { label: "200 a day", hunts: 200 },
];

// --- coming back on its own ---

/**
 * Whether Purrch launches at login.
 *
 * The chore board leans on this harder than it looks: "your PC is already on,
 * so something may as well be using it" is only true of an app that's running,
 * and nobody opens a pet by hand every morning. Still the user's call — the
 * board just makes the case for it when it's off and there are chores waiting.
 */
export async function autostart(): Promise<boolean> {
  const api = await tauri();
  if (!api) return false;
  return await api.invoke<boolean>("autostart_enabled");
}

export async function setAutostart(on: boolean): Promise<boolean> {
  const api = await tauri();
  if (!api) return false;
  return await api.invoke<boolean>("autostart_set", { on });
}

// --- staying current ---

export interface Available {
  /** null means "looked, nothing newer" — not the same as an error. */
  version: string | null;
  notes: string | null;
  error: string | null;
}

export async function checkForUpdate(): Promise<Available> {
  const api = await tauri();
  if (!api) return { version: null, notes: null, error: null };
  return await api.invoke<Available>("update_check");
}

/** Installs and restarts. Only ever from a button the user pressed. */
export async function installUpdate(): Promise<void> {
  const api = await tauri();
  await api?.invoke("update_install");
}

const UPDATE_EVENT = "purrch://update";

/** The tray's check has no window to answer to, so it broadcasts instead. */
export async function onUpdate(
  handler: (found: Available) => void,
): Promise<() => void> {
  if (!("__TAURI_INTERNALS__" in window)) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return await listen<Available>(UPDATE_EVENT, (e) => handler(e.payload));
}
