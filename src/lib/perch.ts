// Where the cat is allowed to stand.
//
// The cat lives on the strip of screen just above the taskbar: the OS work
// area, which is a monitor minus whatever the taskbar occupies. There's one of
// those per monitor, each with its own taskbar height and its own DPI, so the
// backend hands over all of them and the cat picks whichever is under its feet.
//
// Everything here is in *physical* pixels on the virtual desktop. That's the
// only coordinate space shared by every monitor — a logical pixel is defined by
// one monitor's scale factor, and a 150% laptop beside a 100% external has two
// different ones. Pointer events arrive in CSS pixels, so they get multiplied
// by `devicePixelRatio` on the way in.
//
// As with the bridge, running in a plain browser (`npm run dev`) degrades to
// no-ops — there's no window to move, so the cat just sits in the page.

/** One monitor's work area, and what the cat window measures on it. */
export interface Screen {
  left: number;
  top: number;
  right: number;
  bottom: number;
  /** Window y with the cat's feet on this monitor's taskbar. */
  floor: number;
  /** Highest the window may be lifted here — the top of the work area. */
  ceiling: number;
  /** Window size on this monitor; it changes with the DPI. */
  winW: number;
  winH: number;
  /** Width of the cat itself — less than `winW` while the panel is open. */
  catW: number;
  scale: number;
  /** Index into `Perch.strips` of the run this monitor belongs to. */
  strip: number;
}

/**
 * A run of monitors whose work areas touch, so the cat can walk from one to
 * the next. Monitors stacked vertically, or with a gap between them, are
 * separate strips — the cat can be carried across but won't walk into the void.
 */
export interface Strip {
  left: number;
  right: number;
}

export interface Perch {
  screens: Screen[];
  strips: Strip[];
  /** Index into `screens` of the monitor under the cat's feet. */
  current: number;
  /** Where the window is right now. */
  x: number;
  y: number;
}

/** The limits that apply while the cat is standing on one particular screen. */
export interface Ground {
  /** Index into `Perch.screens`. */
  screen: number;
  /** Left-most x the window may take. */
  minX: number;
  /** Right-most x the window may take (already reduced by the window width). */
  maxX: number;
  /** Window y when the cat's feet are on the taskbar — the ground line. */
  floor: number;
  /** Highest the window may be lifted, i.e. the top of the work area. */
  ceiling: number;
  /** This monitor's DPI scale, for converting speeds and distances. */
  scale: number;
}

async function tauri() {
  if (!("__TAURI_INTERNALS__" in window)) return null;
  return await import("@tauri-apps/api/core");
}

/** Every monitor's ground line, or null outside the app shell. */
export async function read(): Promise<Perch | null> {
  const api = await tauri();
  if (!api) return null;
  try {
    return await api.invoke<Perch>("perch");
  } catch {
    // no monitor reported (locked session, monitor unplugged) — stay put
    return null;
  }
}

export async function moveTo(x: number, y: number): Promise<void> {
  const api = await tauri();
  await api?.invoke("hop", { x, y });
}

/**
 * Which screen a window at `(x, y)` is standing on.
 *
 * The cat is drawn at the bottom-right of its window, so it's the feet that
 * decide — not the middle of a chat panel it happens to have open. `from` is
 * the screen it was last on, used only to size the window, since that size is
 * what varies between monitors.
 *
 * Ranked horizontally first: a cat that has just stepped over a seam belongs to
 * the monitor it stepped onto, even though it's still standing at the old
 * taskbar's height and hasn't dropped to the new one yet. The vertical distance
 * only breaks ties, which is what separates stacked monitors.
 */
export function under(p: Perch, x: number, y: number, from: number): number {
  const held = p.screens[from] ? from : p.current;
  const s = p.screens[held];
  if (!s) return 0;
  const footX = x + s.winW - Math.min(s.catW, s.winW) / 2;
  const footY = y + s.winH;

  // Distance to the work area, zero anywhere inside it. A cat let go over a
  // gap between monitors lands on the closest one rather than nowhere.
  const gap = (c: Screen): [number, number] => [
    Math.max(c.left - footX, footX - c.right, 0),
    Math.max(c.top - footY, footY - c.bottom, 0),
  ];

  // Seeded with the screen it's already on, so standing exactly on a seam —
  // where two monitors are equally close — doesn't flicker between them.
  let best = held;
  let [bestX, bestY] = gap(s);
  p.screens.forEach((c, i) => {
    const [dx, dy] = gap(c);
    if (dx < bestX || (dx === bestX && dy < bestY)) {
      bestX = dx;
      bestY = dy;
      best = i;
    }
  });
  return best;
}

/**
 * Limits for a cat standing on screen `i`. The floor is that monitor's alone,
 * but the cat may straddle the seam between touching monitors, so the
 * horizontal limits span the whole strip.
 */
export function ground(p: Perch, i: number): Ground {
  const s = p.screens[i];
  const strip = p.strips[s.strip] ?? { left: s.left, right: s.right };
  return {
    screen: i,
    minX: strip.left,
    maxX: Math.max(strip.right - s.winW, strip.left),
    floor: s.floor,
    ceiling: s.ceiling,
    scale: s.scale,
  };
}
