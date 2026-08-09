// The chore board, from the window's side.
//
// A chore is a standing job you hand to this cat; one execution of it is a
// *hunt*, and what the cat comes back with is a *gift*. The board itself and
// the clock that reads it both live in Rust (`chores.rs`, `hunt.rs`) — this is
// the typed client, plus the two event streams the panel watches.
//
// Hunts are deliberately on their own channel, separate from `bridge.ts`'s.
// A chore firing at 09:00 is not something you said, and it must never end up
// appended to the conversation you were having.
//
// As everywhere else, a plain browser (`npm run dev`) has no board behind it,
// so every call degrades to an empty answer and the cat just lives in the
// moment.

import type { BridgeEvent } from "./bridge.ts";

/** A standing job. Mirrors `chores::Chore`. */
export interface Chore {
  id: string;
  cat: string;
  name: string;
  prompt: string;
  cwd: string | null;
  /** How often it goes and looks. Never less than {@link MIN_EVERY_MS}. */
  everyMs: number;
  enabled: boolean;
  /** Whether a slot missed while the PC was off runs late or is let go. */
  catchUp: boolean;
  nextDue: number;
  lastRun: number;
  runs: number;
  session: string | null;
}

/**
 * One tool the cat picked up while it was out. Mirrors `chores::Step`.
 *
 * The turn you ask for streams its tools into the panel where you watch them
 * go by — that visible feed is the whole answer to "it follows whatever it
 * reads". A chore firing at 09:00 has nobody watching, so its feed is kept
 * instead, and this is it.
 */
export interface Step {
  tool: string;
  detail: string;
  /** null if the hunt died while this one was still running. */
  ok: boolean | null;
}

/** What the cat brought back. Mirrors `chores::Gift`. */
export interface Gift {
  id: string;
  cat: string;
  chore: string;
  choreName: string;
  at: number;
  ok: boolean;
  text: string;
  /** Can exceed `trail.length` — that's how you know it was cut off. */
  tools: number;
  trail: Step[];
  read: boolean;
}

/** A chore on its way to the board, before it has an id or a slot. */
export interface Draft {
  name: string;
  prompt: string;
  cwd?: string | null;
  everyMs: number;
  catchUp: boolean;
}

/** Which hunt a stream of events belongs to. Mirrors `session::Hunt`. */
export interface HuntTag {
  id: string;
  chore: string;
  name: string;
}

/** What the cat is out doing right now. Mirrors `hunt::Live`. */
export interface Live {
  hunt: string;
  chore: string;
  name: string;
  since: number;
}

export interface Status {
  live: Live | null;
  waiting: number;
}

/**
 * The floor on how often a chore may fire, mirrored from `chores.rs`.
 *
 * Every hunt spends the user's subscription in the background. Typing less
 * than this isn't refused — it's rounded up, on both sides of the bridge.
 */
export const MIN_EVERY_MS = 5 * 60 * 1000;

/**
 * The intervals the board offers.
 *
 * Lazy on purpose. "Every X" is the whole mental model, and the honest default
 * for most chores is *far* less often than it feels like it should be: a cat
 * that wanders over to look at the inbox four times an hour is spending your
 * afternoon's tokens on finding nothing.
 */
export const EVERY: { label: string; ms: number }[] = [
  { label: "15 min", ms: 15 * 60 * 1000 },
  { label: "30 min", ms: 30 * 60 * 1000 },
  { label: "hourly", ms: 60 * 60 * 1000 },
  { label: "3 hours", ms: 3 * 60 * 60 * 1000 },
  { label: "6 hours", ms: 6 * 60 * 60 * 1000 },
  { label: "daily", ms: 24 * 60 * 60 * 1000 },
];

/** How an interval reads on the board, whatever number is behind it. */
export function everyLabel(ms: number): string {
  const known = EVERY.find((e) => e.ms === ms);
  if (known) return known.label;
  const mins = Math.round(ms / 60000);
  if (mins < 60) return `${mins} min`;
  const hours = mins / 60;
  if (hours < 24) return `${+hours.toFixed(1)} hours`;
  return `${+(hours / 24).toFixed(1)} days`;
}

/** Rough, cat-sized time. "just now", "in 40 min", "2 days ago". */
export function when(at: number, now = Date.now()): string {
  const delta = at - now;
  const mins = Math.round(Math.abs(delta) / 60000);
  if (mins < 1) return "just now";
  const say =
    mins < 60
      ? `${mins} min`
      : mins < 1440
        ? `${Math.round(mins / 60)}h`
        : `${Math.round(mins / 1440)}d`;
  return delta > 0 ? `in ${say}` : `${say} ago`;
}

async function tauri() {
  if (!("__TAURI_INTERNALS__" in window)) return null;
  return await import("@tauri-apps/api/core");
}

/** Everything on this cat's board. */
export async function list(): Promise<Chore[]> {
  const api = await tauri();
  if (!api) return [];
  return await api.invoke<Chore[]>("chores_list");
}

export async function add(draft: Draft): Promise<Chore | null> {
  const api = await tauri();
  if (!api) return null;
  return await api.invoke<Chore>("chores_add", { draft });
}

/** Shallow, field by field — the board only ever sends what it changed. */
export async function update(
  id: string,
  patch: Partial<Chore>,
): Promise<Chore | null> {
  const api = await tauri();
  if (!api) return null;
  return await api.invoke<Chore>("chores_update", { id, patch });
}

export async function remove(id: string): Promise<void> {
  const api = await tauri();
  await api?.invoke("chores_remove", { id });
}

/**
 * Go now. Still queues behind whatever the cat is already doing.
 *
 * Rejects when the cat has spent its day, or when the colony hasn't been let
 * loose — somebody is watching this one, so a button that silently did nothing
 * would be worse than an answer.
 */
export async function runNow(id: string): Promise<void> {
  const api = await tauri();
  await api?.invoke("chores_run_now", { id });
}

/**
 * What this cat is out doing. Read on mount, because a reloaded window has
 * missed the events that would otherwise have told it.
 */
export async function status(): Promise<Status> {
  const api = await tauri();
  if (!api) return { live: null, waiting: 0 };
  return await api.invoke<Status>("hunt_status");
}

/** This cat's pile, newest first. */
export async function gifts(): Promise<Gift[]> {
  const api = await tauri();
  if (!api) return [];
  return await api.invoke<Gift[]>("gifts_list");
}

/** Marks gifts as looked at. No ids means the whole pile. */
export async function read(ids: string[] = []): Promise<void> {
  const api = await tauri();
  await api?.invoke("gifts_read", { ids });
}

export async function clear(): Promise<void> {
  const api = await tauri();
  await api?.invoke("gifts_clear");
}

const HUNT_EVENT = "purrch://hunt";
const GIFT_EVENT = "purrch://gift";

/**
 * Live progress from a hunt: the same events a turn produces, wrapped in the
 * chore they belong to. This is what the check-in line is built from.
 */
export async function onHunt(
  handler: (hunt: HuntTag, event: BridgeEvent) => void,
): Promise<() => void> {
  if (!("__TAURI_INTERNALS__" in window)) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return await listen<{ hunt: HuntTag; event: BridgeEvent }>(HUNT_EVENT, (e) =>
    handler(e.payload.hunt, e.payload.event),
  );
}

/** A hunt is over and there's something by the door. */
export async function onGift(
  handler: (gift: Gift) => void,
): Promise<() => void> {
  if (!("__TAURI_INTERNALS__" in window)) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return await listen<{ hunt: string; gift: Gift }>(GIFT_EVENT, (e) =>
    handler(e.payload.gift),
  );
}
