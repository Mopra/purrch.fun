// Typed client for the Rust subscription bridge.
//
// Everything here is a thin wrapper over Tauri IPC. When Purrch runs in a
// plain browser (`npm run dev`), Tauri isn't present — each call degrades to
// a no-op so the cat still animates while iterating on the art.

/**
 * Whose money a backend's turns spend.
 *
 * - `inherit` — the default: whatever the CLI already does, including any key
 *   the user has exported into their own environment.
 * - `subscription` — the CLI's own login, with ambient keys scrubbed.
 * - `key` — an API key saved in the OS credential store.
 */
export type Auth = "inherit" | "subscription" | "key";

export interface Backend {
  id: string;
  label: string;
  /** Which subscription this backend spends, for the picker's subtitle. */
  subscription: string;
  program: string;
  version: string | null;
  signedIn: boolean;
  /** Env var this CLI takes an API key in, or null if it doesn't take one. */
  keyEnv: string | null;
  auth: Auth;
  /** A key is saved for it. Never the key itself — that stays in Rust. */
  hasKey: boolean;
  /**
   * That key is in a file rather than the OS credential store.
   *
   * Only true when the keychain couldn't be reached. Surfaced because the
   * alternative is the app quietly keeping a secret somewhere it said it
   * wouldn't, and the user having no way to find that out.
   */
  keyInFile: boolean;
  /** This adapter has never been run against the real CLI. */
  experimental: boolean;
}

/** A CLI that's installed but that Purrch can't drive yet. */
export interface Other {
  id: string;
  label: string;
}

/**
 * What detection found, split by whether a turn can actually run on it.
 *
 * The split comes from Rust rather than being a flag the picker has to
 * remember to filter on: a backend with no adapter dies at spawn time, so the
 * shape of the data is what keeps it off the menu. `others` exists so an
 * installed CLI is explained rather than ignored.
 */
export interface Detected {
  backends: Backend[];
  others: Other[];
}

export type BridgeEvent =
  | { kind: "started"; session: string; model: string | null }
  | { kind: "thinking" }
  | { kind: "text"; text: string }
  | { kind: "toolStart"; tool: string; detail: string }
  | { kind: "toolEnd"; tool: string; ok: boolean }
  | { kind: "finished"; ok: boolean; text: string | null; ms: number | null }
  | { kind: "failed"; message: string };

export interface TurnRequest {
  backend: string;
  prompt: string;
  resume?: string | null;
  cwd?: string | null;
  model?: string | null;
  /** What this cat is called. Goes into the persona it runs with. */
  name?: string;
}

const EVENT = "purrch://agent";

async function tauri() {
  // '__TAURI_INTERNALS__' is injected only inside the real app shell.
  if (!("__TAURI_INTERNALS__" in window)) return null;
  return await import("@tauri-apps/api/core");
}

/**
 * Rust sends camelCase now, but older builds sent snake_case for some of these
 * and the cost of accepting both is one `??` per field.
 */
function normalize(raw: any): Backend {
  return {
    id: raw.id,
    label: raw.label,
    subscription: raw.subscription,
    program: raw.program,
    version: raw.version ?? null,
    signedIn: raw.signed_in ?? raw.signedIn ?? false,
    keyEnv: raw.key_env ?? raw.keyEnv ?? null,
    auth: raw.auth ?? "inherit",
    hasKey: raw.has_key ?? raw.hasKey ?? false,
    keyInFile: raw.key_in_file ?? raw.keyInFile ?? false,
    experimental: raw.experimental ?? false,
  };
}

/**
 * Whether a backend has a credential we can actually point to, of either kind.
 *
 * Deliberately not true for `inherit`: that means "we don't know what the CLI
 * will do", which is fine to fall back to but must never win a preference over
 * a backend we know is signed in.
 */
export function usable(b: Backend): boolean {
  return b.signedIn || b.hasKey;
}

/**
 * Hands a backend an API key to spend instead of a subscription.
 *
 * The key crosses this boundary once and is never read back: it goes to the OS
 * credential store, and the only thing that ever sees it again is the CLI's
 * environment. {@link detect} reports whether one exists, never what it is.
 */
export async function setKey(backend: string, key: string): Promise<void> {
  const api = await tauri();
  if (!api) throw new Error("keys can only be saved inside the Purrch app");
  await api.invoke("creds_set_key", { backend, key });
}

/** Back to the CLI's own login, dropping any saved key. */
export async function useSubscription(backend: string): Promise<void> {
  const api = await tauri();
  await api?.invoke("creds_use_subscription", { backend });
}

/** Forgets the choice entirely, back to inheriting the environment. */
export async function clearAuth(backend: string): Promise<void> {
  const api = await tauri();
  await api?.invoke("creds_clear", { backend });
}

export async function detect(): Promise<Detected> {
  const api = await tauri();
  if (!api) return { backends: [], others: [] };
  const raw = await api.invoke<any>("bridge_detect");
  return {
    backends: (raw.backends ?? []).map(normalize),
    others: raw.others ?? [],
  };
}

export async function homeDir(): Promise<string | null> {
  const api = await tauri();
  if (!api) return null;
  return await api.invoke<string>("home_dir");
}

/** Resolves when the turn ends. Progress arrives via {@link onEvent}. */
export async function send(request: TurnRequest): Promise<void> {
  const api = await tauri();
  if (!api) throw new Error("the bridge only runs inside the Purrch app");
  await api.invoke("bridge_send", { request });
}

export async function cancel(): Promise<void> {
  const api = await tauri();
  await api?.invoke("bridge_cancel");
}

export async function setPanel(open: boolean): Promise<void> {
  const api = await tauri();
  await api?.invoke("set_panel", { open });
}

/**
 * Borrows enough window for the right-click menu to be drawn whole.
 *
 * Only ever grows the window; `setPanel` is what gives the room back once the
 * menu is down.
 */
export async function setMenu(w: number, h: number): Promise<void> {
  const api = await tauri();
  await api?.invoke("set_menu", { w, h });
}

/** Dropped paths, split by kind so a folder can re-scope the session. */
export interface Dropped {
  dirs: string[];
  files: string[];
}

export async function classifyPaths(paths: string[]): Promise<Dropped> {
  const api = await tauri();
  if (!api) return { dirs: [], files: [] };
  return await api.invoke<Dropped>("classify_paths", { paths });
}

/**
 * This cat's window label — the key everything per-cat is filed under: its
 * agent session, its memory, and its name and coat. Outside the app there is
 * only ever one cat, so it answers to the main window's label.
 */
export async function catLabel(): Promise<string> {
  if (!("__TAURI_INTERNALS__" in window)) return "main";
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow().label;
}

/** Adds another cat to the colony. Returns its window label. */
export async function spawnCat(): Promise<string | null> {
  const api = await tauri();
  if (!api) return null;
  return await api.invoke<string>("spawn_cat");
}

/** Sends this cat home — or quits, if it's the last one. */
export async function dismissCat(): Promise<void> {
  const api = await tauri();
  await api?.invoke("dismiss_cat");
}

/**
 * Files dragged onto this cat's window. Tauri intercepts the OS drop, so the
 * usual HTML drag events never fire — this is the only way to see them.
 *
 * `onHover` is told where the file is as well as whether it's there at all, in
 * physical pixels from the window's top-left, so the cat can watch it move.
 */
export async function onDrop(
  handler: (paths: string[]) => void,
  onHover?: (hovering: boolean, x?: number, y?: number) => void,
): Promise<() => void> {
  if (!("__TAURI_INTERNALS__" in window)) return () => {};
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  return await getCurrentWebview().onDragDropEvent((e) => {
    if (e.payload.type === "drop") {
      onHover?.(false);
      handler(e.payload.paths);
    } else if (e.payload.type === "enter" || e.payload.type === "over") {
      onHover?.(true, e.payload.position.x, e.payload.position.y);
    } else if (e.payload.type === "leave") {
      onHover?.(false);
    }
  });
}

/**
 * Whether this cat's window still has the user's attention.
 *
 * The DOM's own blur event doesn't catch every way focus leaves a borderless
 * always-on-top window, and this one floats over everything — anything it has
 * hanging open has to go away when you turn to another app.
 */
export async function onFocusChange(
  handler: (focused: boolean) => void,
): Promise<() => void> {
  if (!("__TAURI_INTERNALS__" in window)) return () => {};
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return await getCurrentWindow().onFocusChanged((e) => handler(e.payload));
}

/** Subscribes to the turn event stream. Returns an unlisten function. */
export async function onEvent(
  handler: (event: BridgeEvent) => void,
): Promise<() => void> {
  if (!("__TAURI_INTERNALS__" in window)) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return await listen<BridgeEvent>(EVENT, (e) => handler(e.payload));
}
