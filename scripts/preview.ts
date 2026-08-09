// Renders key animation frames into one sprite-sheet PNG so the art can be
// reviewed visually. Run: node scripts/preview.ts
import { writeFileSync } from "node:fs";
import {
  renderScene,
  validateArt,
  IDLE_POSE,
  SCENE_W,
  SCENE_H,
  CAT_X,
  CAT_Y,
  type CatPose,
} from "../src/lib/render.ts";
import {
  HEART,
  HEART_SMALL,
  PUFF,
  PUFF_SMALL,
  ZZ_BIG,
  ZZ_SMALL,
  EAR_WAVE,
  EAR_WAVE_SMALL,
  MOUSE,
  PILE,
  YARN_FRAMES,
  YARN_H,
} from "../src/lib/catArt.ts";
import { encodePng, scaleRgba } from "./png.ts";

/** Yarn y when it is resting on the ground, as in Cat.svelte. */
const FLOOR = SCENE_H - YARN_H;

const problems = validateArt();
if (problems.length) {
  console.error("ART PROBLEMS:");
  for (const p of problems) console.error("  " + p);
  process.exit(1);
}

type Sprite = { grid: string[]; x: number; y: number };

const frames: {
  name: string;
  pose: Partial<CatPose>;
  particles?: Sprite[];
  yarn?: Sprite;
}[] = [
  { name: "idle-tail-down", pose: {} },
  { name: "idle-tail-mid", pose: { tail: "mid" } },
  { name: "idle-tail-up", pose: { tail: "up" } },
  { name: "blink", pose: { eyes: "closed" } },
  { name: "gaze-left", pose: { gaze: -1 } },
  {
    name: "pet",
    pose: { eyes: "happy", bigBlush: true, bob: 1 },
    particles: [
      { grid: HEART, x: CAT_X - 4, y: CAT_Y - 7 },
      { grid: HEART_SMALL, x: CAT_X + 23, y: CAT_Y - 4 },
      // one that has drifted up into the fade, on its way out of the frame
      { grid: HEART, x: CAT_X + 8, y: 1 },
    ],
  },
  { name: "groom-up", pose: { groomPaw: "up", eyes: "closed" } },
  { name: "groom-down", pose: { groomPaw: "down", eyes: "closed" } },
  {
    name: "sleep",
    pose: { eyes: "closed", bob: 1, tail: "down" },
    particles: [
      { grid: ZZ_SMALL, x: CAT_X + 22, y: CAT_Y - 4 },
      { grid: ZZ_BIG, x: CAT_X + 25, y: CAT_Y - 9 },
    ],
  },
  {
    name: "play-watch",
    pose: { gaze: -1, tail: "up", playPaw: "ready", bob: 1 },
    yarn: { grid: YARN_FRAMES[0], x: CAT_X - 2, y: FLOOR },
  },
  {
    name: "play-swat",
    pose: { gaze: -1, tail: "mid", playPaw: "swat" },
    yarn: { grid: YARN_FRAMES[1], x: CAT_X - 3, y: FLOOR - 1 },
  },
  {
    // rolling away, out past the fade — how it looks leaving the frame
    name: "play-chase",
    pose: { gaze: -1, tail: "up" },
    yarn: { grid: YARN_FRAMES[2], x: 1, y: FLOOR - 3 },
  },
  // a file held over the cat — eyes up on it, paw already reaching
  { name: "curious", pose: { tail: "up", playPaw: "ready", bob: 1, gazeY: -1 } },
  // spoken to by name: ears up and turned on you, sound coming in from the left
  {
    name: "listen",
    pose: { earsUp: true, tail: "mid" },
    particles: [
      { grid: EAR_WAVE, x: CAT_X - 8, y: CAT_Y - 3 },
      { grid: EAR_WAVE_SMALL, x: CAT_X - 4, y: CAT_Y - 1 },
    ],
  },
  // back from a chore, sat next to what it caught. One mouse, then a pile —
  // the count has to be readable at a glance from the taskbar, which is the
  // only place anyone will ever see it.
  {
    name: "gift-one",
    pose: { eyes: "happy", tail: "up", bigBlush: true },
    particles: [{ grid: MOUSE, ...PILE[0] }],
  },
  {
    name: "gift-pile",
    pose: { tail: "mid" },
    particles: PILE.map((at) => ({ grid: MOUSE, ...at })),
  },
  // the pick-up, in order: scruffed and still, swung by a hand on the move,
  // dropped and reaching for the floor, then folded into it.
  { name: "hang", pose: { posture: "hang", gazeY: 1 } },
  { name: "hang-swing", pose: { posture: "hang", gazeY: 1, gaze: -1, lean: 4 } },
  { name: "fall", pose: { posture: "fall", tail: "up", gazeY: 1 } },
  {
    name: "land",
    pose: { posture: "land", tail: "up", eyes: "closed" },
    particles: [
      { grid: PUFF, x: CAT_X - 4, y: SCENE_H - 7 },
      { grid: PUFF_SMALL, x: CAT_X + 22, y: SCENE_H - 6 },
    ],
  },
];

const SCALE = 6;
const COLS = 3;
const rows = Math.ceil(frames.length / COLS);
const cellW = SCENE_W * SCALE;
const cellH = SCENE_H * SCALE;
const sheetW = cellW * COLS;
const sheetH = cellH * rows;
const sheet = new Uint8Array(sheetW * sheetH * 4);

// checkerboard background so transparency is visible
for (let y = 0; y < sheetH; y++) {
  for (let x = 0; x < sheetW; x++) {
    const i = (y * sheetW + x) * 4;
    const v = ((x >> 4) + (y >> 4)) % 2 === 0 ? 38 : 50;
    sheet[i] = v;
    sheet[i + 1] = v + 4;
    sheet[i + 2] = v + 10;
    sheet[i + 3] = 255;
  }
}

frames.forEach((f, idx) => {
  const pose: CatPose = { ...IDLE_POSE, ...f.pose };
  const buf = renderScene(pose, f.particles ?? [], f.yarn ?? null);
  const scaled = scaleRgba(buf, SCENE_W, SCENE_H, cellW, cellH);
  const gx = (idx % COLS) * cellW;
  const gy = Math.floor(idx / COLS) * cellH;
  for (let y = 0; y < cellH; y++) {
    for (let x = 0; x < cellW; x++) {
      const si = (y * cellW + x) * 4;
      const a = scaled[si + 3] / 255;
      if (a === 0) continue;
      // blended rather than punched in, so the fade at the scene edges — the
      // whole reason the cat has room around it — is visible in the sheet
      const di = ((gy + y) * sheetW + (gx + x)) * 4;
      for (let c = 0; c < 3; c++) {
        sheet[di + c] = scaled[si + c] * a + sheet[di + c] * (1 - a);
      }
      sheet[di + 3] = 255;
    }
  }
});

writeFileSync("preview.png", encodePng(sheetW, sheetH, sheet));
console.log(`wrote preview.png (${sheetW}x${sheetH}), frames: ${frames.map((f) => f.name).join(", ")}`);
