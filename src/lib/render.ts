// Pure pixel compositor — no DOM access, shared by the app and the
// Node-based art preview / icon scripts.

import {
  PALETTE,
  CAT_W,
  CAT_H,
  BODY_SIT,
  BODY_HANG,
  BODY_FALL,
  BODY_LAND,
  HANG_PIVOT,
  EARS_PERKED,
  EYES_OPEN,
  EYES_CLOSED,
  EYES_HAPPY,
  FACE,
  BLUSH_BIG,
  PAW_UP,
  PAW_DOWN,
  PAW_READY,
  PAW_SWAT,
  TAIL_DOWN,
  TAIL_MID,
  TAIL_UP,
  YARN_FRAMES,
  MOUSE,
  MOUSE_W,
  MOUSE_H,
  PILE,
  type Grid,
} from "./catArt.ts";
import { coatById, type Coat } from "./coats.ts";

// The scene is deliberately wider and taller than the cat: the yarn needs a
// runway to roll down and the hearts need sky to drift into. The cat sits
// flush with the bottom (CAT_Y + CAT_H === SCENE_H), because that edge is the
// taskbar it stands on — every other edge is empty desktop.
export const SCENE_W = 48;
export const SCENE_H = 40;
export const CAT_X = 16;
export const CAT_Y = 14;

export type EyeState = "open" | "closed" | "happy";
export type TailState = "down" | "mid" | "up";

/**
 * Which body the cat is wearing. `sit` is the one it spends its life in;
 * the other three are the arc of being picked up and put down again —
 * dangling from a scruff, reaching for the floor, and folding into it.
 */
export type Posture = "sit" | "hang" | "fall" | "land";

export interface CatPose {
  eyes: EyeState;
  tail: TailState;
  posture: Posture;
  gaze: number; // -1 | 0 | 1 — shifts the pupils sideways
  gazeY: number; // and up/down, so a dangled cat can look at the drop
  lean: number; // scene px the dangling half swings sideways (hang only)
  bob: number; // vertical offset of whole cat (sleep breathing / pet squash)
  lift: number; // scene px the cat is off the ground mid-jump
  /**
   * Radians the whole cat is turned by, about the middle of its body.
   *
   * The one thing in here that isn't a layer swap. Whirl the cat around on the
   * end of the pointer and the dangle turns into a tumble, and a tumble is the
   * one pose that can't be drawn: it isn't a shape a cat holds, it's every
   * shape it already has, seen from a different angle. So the art is drawn
   * flat and the finished cat is turned as one piece, which keeps the face on
   * the head and the paws on the legs all the way round.
   */
  spin: number;
  groomPaw: "none" | "up" | "down";
  playPaw: "none" | "ready" | "swat"; // front-left leg while chasing the yarn
  bigBlush: boolean;
  /** Ears up and pointed at you — the cat is being spoken to. */
  earsUp: boolean;
}

const BODIES: Record<Posture, Grid> = {
  sit: BODY_SIT,
  hang: BODY_HANG,
  fall: BODY_FALL,
  land: BODY_LAND,
};

/**
 * The tail layers are drawn against the sitting body, so they only belong to
 * the postures whose back end is still roughly where the sitting cat's is.
 * The hanging body carries its own drooped tail instead.
 */
const WEARS_TAIL: Record<Posture, boolean> = {
  sit: true,
  hang: false,
  fall: true,
  land: true,
};

export interface Particle {
  grid: Grid;
  x: number;
  y: number;
  /** 0–1, so a sprite can dissolve instead of winking out. */
  alpha?: number;
}

const TAILS: Record<TailState, Grid> = {
  down: TAIL_DOWN,
  mid: TAIL_MID,
  up: TAIL_UP,
};

function hexToRgb(hex: string): [number, number, number] {
  return [
    parseInt(hex.slice(1, 3), 16),
    parseInt(hex.slice(3, 5), 16),
    parseInt(hex.slice(5, 7), 16),
  ];
}

type Rgb = Record<string, [number, number, number]>;

/**
 * The palette a given coat draws with: the shared colours, with the four the
 * coat owns swapped in. Cached per coat, since the tick loop asks for one on
 * every frame and the answer only changes when the collar does.
 */
const PALETTES = new Map<string, Rgb>();

function rgbFor(coat: Coat): Rgb {
  const hit = PALETTES.get(coat.id);
  if (hit) return hit;
  const rgb: Rgb = {};
  const colors = {
    ...PALETTE,
    o: coat.line,
    b: coat.fur,
    d: coat.shade,
    w: coat.trim,
  };
  for (const [ch, hex] of Object.entries(colors)) rgb[ch] = hexToRgb(hex);
  PALETTES.set(coat.id, rgb);
  return rgb;
}

/**
 * How far in from the left, right and top edges a pixel is faded out, in scene
 * pixels.
 *
 * The window has no frame, so a sprite cut off mid-pixel by its boundary is the
 * one thing that gives away there is a window there at all — the yarn stops
 * reading as a ball rolling across the desktop and starts reading as a ball in
 * a box. Dissolving over the last few pixels means nothing ever ends at a line.
 *
 * The bottom edge is exempt: that one isn't invisible, it's the taskbar the cat
 * and the yarn are both resting on.
 */
const EDGE_FADE = 6;

function edgeAlpha(x: number, y: number): number {
  const d = Math.min(x + 1, SCENE_W - x, y + 1);
  return d >= EDGE_FADE ? 1 : Math.max(d, 0) / EDGE_FADE;
}

/**
 * How far row `y` of a grid slides sideways for a given `lean`.
 *
 * Rows above the pivot are the ones held by the scruff, so they don't move at
 * all; below it each row goes a little further than the one above, squared so
 * the bend starts gently at the neck and ends with the back paws swinging
 * widest. That reads as a body hanging off a fixed point rather than a sprite
 * sheared in half.
 */
function bend(y: number, height: number, pivot: number, lean: number): number {
  if (lean === 0 || y <= pivot) return 0;
  const span = height - 1 - pivot;
  if (span <= 0) return 0;
  const t = (y - pivot) / span;
  return Math.round(lean * t * t);
}

/**
 * Lays one colour onto one pixel at opacity `a`, source-over — so a half-faded
 * heart passing the cat thins itself rather than punching a hole in the coat.
 */
function paint(
  buf: Uint8ClampedArray,
  idx: number,
  r: number,
  g: number,
  b: number,
  a: number,
): void {
  if (a >= 1) {
    buf[idx] = r;
    buf[idx + 1] = g;
    buf[idx + 2] = b;
    buf[idx + 3] = 255;
    return;
  }
  const da = buf[idx + 3] / 255;
  const oa = a + da * (1 - a);
  buf[idx] = (r * a + buf[idx] * da * (1 - a)) / oa;
  buf[idx + 1] = (g * a + buf[idx + 1] * da * (1 - a)) / oa;
  buf[idx + 2] = (b * a + buf[idx + 2] * da * (1 - a)) / oa;
  buf[idx + 3] = oa * 255;
}

function stamp(
  buf: Uint8ClampedArray,
  RGB: Rgb,
  grid: Grid,
  ox: number,
  oy: number,
  alpha = 1,
  lean = 0,
  pivot = 0,
): void {
  for (let y = 0; y < grid.length; y++) {
    const row = grid[y];
    const dx = bend(y, grid.length, pivot, lean);
    for (let x = 0; x < row.length; x++) {
      const ch = row[x];
      if (ch === ".") continue;
      const px = ox + x + dx;
      const py = oy + y;
      if (px < 0 || px >= SCENE_W || py < 0 || py >= SCENE_H) continue;
      const rgb = RGB[ch];
      if (!rgb) continue;
      const a = alpha * edgeAlpha(px, py);
      if (a <= 0) continue;
      paint(buf, (py * SCENE_W + px) * 4, rgb[0], rgb[1], rgb[2], a);
    }
  }
}

/**
 * The point a turning cat turns about: the middle of the box its body fills.
 *
 * Measured off the body grid alone rather than off the finished frame, so the
 * pivot belongs to the posture and to nothing else. Taking it from what's been
 * drawn would let a tail swishing from `down` to `up` shift the centre a pixel
 * and turn a steady spin into a wobble. Cached — there are four bodies and the
 * tumble asks for one on every frame.
 */
const PIVOTS = new Map<Grid, [number, number]>();

function pivotOf(grid: Grid): [number, number] {
  const hit = PIVOTS.get(grid);
  if (hit) return hit;
  let x0 = Infinity;
  let x1 = -Infinity;
  let y0 = Infinity;
  let y1 = -Infinity;
  for (let y = 0; y < grid.length; y++) {
    const row = grid[y];
    for (let x = 0; x < row.length; x++) {
      if (row[x] === ".") continue;
      if (x < x0) x0 = x;
      if (x > x1) x1 = x;
      if (y < y0) y0 = y;
      if (y > y1) y1 = y;
    }
  }
  const at: [number, number] = x1 < x0 ? [0, 0] : [(x0 + x1) / 2, (y0 + y1) / 2];
  PIVOTS.set(grid, at);
  return at;
}

/**
 * Turns the flat cat in `src` onto `dst`, `angle` radians about (px, py).
 *
 * Walks the destination and reads back, which is the only way round that
 * leaves no gaps: going forwards, a turned pixel lands wherever it lands and
 * the ones it misses stay holes. Nearest neighbour, deliberately — the cat is a
 * pixel drawing, and a cat whose outline blurs at 40° stops being drawn and
 * starts being a photograph of itself.
 */
function turn(
  dst: Uint8ClampedArray,
  src: Uint8ClampedArray,
  px: number,
  py: number,
  angle: number,
): void {
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  for (let y = 0; y < SCENE_H; y++) {
    for (let x = 0; x < SCENE_W; x++) {
      const dx = x - px;
      const dy = y - py;
      const sx = Math.round(px + dx * cos + dy * sin);
      const sy = Math.round(py - dx * sin + dy * cos);
      if (sx < 0 || sx >= SCENE_W || sy < 0 || sy >= SCENE_H) continue;
      const si = (sy * SCENE_W + sx) * 4;
      // The fade is applied here rather than carried over from the flat cat:
      // what matters is how close to the window edge a pixel ends up, not where
      // it started, and a leg swinging out wide ends up somewhere else entirely.
      const a = (src[si + 3] / 255) * edgeAlpha(x, y);
      if (a <= 0) continue;
      paint(dst, (y * SCENE_W + x) * 4, src[si], src[si + 1], src[si + 2], a);
    }
  }
}

/**
 * Scratch the cat is drawn flat into before being turned. Reused between
 * frames: a tumble asks for one on every animation frame, and a fresh 7KB of
 * rubbish sixty times a second buys nothing.
 */
let flat: Uint8ClampedArray<ArrayBuffer> | null = null;

/**
 * Renders the full scene into an RGBA buffer (SCENE_W x SCENE_H).
 * `yarn` sits in its own layer between the body and the paws, so the ball
 * rolls in front of the cat but a swatting paw lands on top of it.
 *
 * `coat` recolours the cat only — the yarn, hearts and sleepy z's belong to the
 * scene rather than to the animal, so they look the same on every cat.
 */
export function renderScene(
  pose: CatPose,
  particles: Particle[] = [],
  yarn: Particle | null = null,
  coat: Coat = coatById(null),
  // `ArrayBuffer` rather than the default `ArrayBufferLike`: the callers hand
  // this straight to `new ImageData(...)`, which will not take a buffer that
  // might turn out to be shared.
): Uint8ClampedArray<ArrayBuffer> {
  const buf = new Uint8ClampedArray(SCENE_W * SCENE_H * 4);
  const rgb = rgbFor(coat);
  const body = BODIES[pose.posture];
  const cx = CAT_X;
  // Every body is bottom-aligned to the scene, whatever its height: the sitting
  // cat's feet rest on the last row because that row is the taskbar, and the
  // taller airborne bodies hang from further up the window rather than spilling
  // out of the bottom of it. `lift` is a jump drawn inside the scene rather than
  // by moving the window, so the ground — and the yarn on it — stays put.
  const cy = SCENE_H - body.length + pose.bob - pose.lift;
  // Only the body swings; the head is the bit being held.
  const lean = pose.posture === "hang" ? pose.lean : 0;

  // A turning cat is drawn into a layer of its own and turned as one piece at
  // the end, so the tail, face and paws all come round together. Everything
  // that isn't the animal stays where it is: the yarn belongs to the floor, so
  // it goes down first and the cat turns in front of it. Nothing is lost by
  // dropping it behind the paws — a cat mid-whirl isn't swatting at anything.
  const turning = pose.spin !== 0;
  let cat = buf;
  if (turning) {
    if (!flat) flat = new Uint8ClampedArray(SCENE_W * SCENE_H * 4);
    else flat.fill(0);
    cat = flat;
    if (yarn) stamp(buf, rgb, yarn.grid, Math.round(yarn.x), Math.round(yarn.y), yarn.alpha);
  }

  // tail first so the body overlaps its root
  if (WEARS_TAIL[pose.posture]) stamp(cat, rgb, TAILS[pose.tail], cx, cy);
  stamp(cat, rgb, body, cx, cy, 1, lean, HANG_PIVOT);

  // Only the sitting cat can prick its ears: the airborne bodies are drawn
  // from a different height, and a cat dangling from a hand has other things
  // on its mind than what you just said.
  if (pose.earsUp && pose.posture === "sit") {
    stamp(cat, rgb, EARS_PERKED.grid, cx + EARS_PERKED.ox, cy + EARS_PERKED.oy);
  }

  stamp(cat, rgb, FACE.grid, cx + FACE.ox, cy + FACE.oy);
  if (pose.bigBlush) stamp(cat, rgb, BLUSH_BIG.grid, cx + BLUSH_BIG.ox, cy + BLUSH_BIG.oy);

  const eyes =
    pose.eyes === "open" ? EYES_OPEN : pose.eyes === "closed" ? EYES_CLOSED : EYES_HAPPY;
  stamp(cat, rgb, eyes.grid, cx + eyes.ox + pose.gaze, cy + eyes.oy + pose.gazeY);

  if (yarn && !turning) {
    stamp(cat, rgb, yarn.grid, Math.round(yarn.x), Math.round(yarn.y), yarn.alpha);
  }

  if (pose.groomPaw === "up") stamp(cat, rgb, PAW_UP.grid, cx + PAW_UP.ox, cy + PAW_UP.oy);
  if (pose.groomPaw === "down") stamp(cat, rgb, PAW_DOWN.grid, cx + PAW_DOWN.ox, cy + PAW_DOWN.oy);

  if (pose.playPaw === "ready") stamp(cat, rgb, PAW_READY.grid, cx + PAW_READY.ox, cy + PAW_READY.oy);
  if (pose.playPaw === "swat") stamp(cat, rgb, PAW_SWAT.grid, cx + PAW_SWAT.ox, cy + PAW_SWAT.oy);

  if (turning) {
    const [ox, oy] = pivotOf(body);
    turn(buf, cat, cx + ox, cy + oy, pose.spin);
  }

  for (const p of particles) {
    stamp(buf, rgb, p.grid, Math.round(p.x), Math.round(p.y), p.alpha);
  }

  return buf;
}

export const IDLE_POSE: CatPose = {
  eyes: "open",
  tail: "down",
  posture: "sit",
  gaze: 0,
  gazeY: 0,
  lean: 0,
  bob: 0,
  lift: 0,
  spin: 0,
  groomPaw: "none",
  playPaw: "none",
  bigBlush: false,
  earsUp: false,
};

/** Validates that hand-authored grids are rectangular and only use palette chars. */
export function validateArt(): string[] {
  const problems: string[] = [];
  const check = (name: string, grid: Grid) => {
    const w = grid[0]?.length ?? 0;
    grid.forEach((row, i) => {
      if (row.length !== w) {
        problems.push(`${name} row ${i}: length ${row.length}, expected ${w}`);
      }
      for (const ch of row) {
        if (ch !== "." && !PALETTE[ch]) {
          problems.push(`${name} row ${i}: unknown char '${ch}'`);
        }
      }
    });
  };
  check("BODY_SIT", BODY_SIT);
  check("BODY_HANG", BODY_HANG);
  check("BODY_FALL", BODY_FALL);
  check("BODY_LAND", BODY_LAND);
  // The airborne bodies reuse the sitting head so the eyes, face and blush keep
  // their offsets — if one drifts, the cat's face slides off it. Only the rows
  // those layers land on are pinned; the ears above them are free to react.
  for (let i = 6; i < 16; i++) {
    for (const [name, grid] of [
      ["BODY_HANG", BODY_HANG],
      ["BODY_FALL", BODY_FALL],
      ["BODY_LAND", BODY_LAND],
    ] as const) {
      if (grid[i] !== BODY_SIT[i]) {
        problems.push(`${name} row ${i}: head differs from BODY_SIT`);
      }
    }
  }
  // ...and they must reach the bottom of the scene, since that is where the
  // ground is and `renderScene` bottom-aligns every posture to it.
  for (const [name, grid] of [
    ["BODY_HANG", BODY_HANG],
    ["BODY_FALL", BODY_FALL],
    ["BODY_LAND", BODY_LAND],
  ] as const) {
    if (grid.length > SCENE_H) problems.push(`${name}: ${grid.length} rows, taller than the scene`);
  }
  // The perked ears are drawn over the sitting cat's flat ones rather than
  // instead of them, so they have to be at least as wide or the old ear shows
  // through underneath as a second outline.
  if (EARS_PERKED.grid[0]?.length !== BODY_SIT[0]?.length) {
    problems.push("EARS_PERKED: not as wide as the cat it covers");
  }
  check("EARS_PERKED", EARS_PERKED.grid);
  check("EYES_OPEN", EYES_OPEN.grid);
  check("EYES_CLOSED", EYES_CLOSED.grid);
  check("EYES_HAPPY", EYES_HAPPY.grid);
  check("FACE", FACE.grid);
  check("PAW_UP", PAW_UP.grid);
  check("PAW_DOWN", PAW_DOWN.grid);
  check("PAW_READY", PAW_READY.grid);
  check("PAW_SWAT", PAW_SWAT.grid);
  check("MOUSE", MOUSE);
  YARN_FRAMES.forEach((g, i) => check(`YARN_FRAMES[${i}]`, g));

  // The pile is stacked on the floor beside the cat, and `catArt.ts` can't say
  // where either of those is without importing this file back. So the coupling
  // is checked here: a mouse sunk through the taskbar, or laid across the cat's
  // paws, is still a rectangular grid of legal characters.
  PILE.forEach((at, i) => {
    if (at.y + MOUSE_H > SCENE_H) {
      problems.push(`PILE[${i}]: sunk ${at.y + MOUSE_H - SCENE_H}px through the floor`);
    }
    if (at.x + MOUSE_W > CAT_X + 2) {
      problems.push(`PILE[${i}]: overlaps the cat's paws`);
    }
  });
  // ...and the bottom one has to be *on* the floor, or the whole pile floats.
  if (PILE[0] && PILE[0].y + MOUSE_H !== SCENE_H) {
    problems.push("PILE[0]: the pile rests on this one, so it rests on the floor");
  }
  return problems;
}
