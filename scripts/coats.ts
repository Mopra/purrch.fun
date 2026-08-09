// Renders every coat into one sheet, so the palettes can be judged side by side
// rather than one at a time in the app. Run: node scripts/coats.ts
//
// Order in the sheet is the order in `COATS`, which is also the order the collar
// shows them in — the names are printed on exit rather than drawn, since there
// is no font in here and a mislabelled swatch is worse than an unlabelled one.
import { writeFileSync } from "node:fs";
import {
  renderScene,
  IDLE_POSE,
  SCENE_W,
  SCENE_H,
  type CatPose,
} from "../src/lib/render.ts";
import { COATS } from "../src/lib/coats.ts";
import { encodePng, scaleRgba } from "./png.ts";

/** Two poses per coat: sitting, and mid-cuddle where the trim and blush show. */
const POSES: Partial<CatPose>[] = [
  { tail: "up" },
  { eyes: "happy", bigBlush: true, tail: "mid", groomPaw: "up" },
];

const SCALE = 5;
const cellW = SCENE_W * SCALE;
const cellH = SCENE_H * SCALE;
const sheetW = cellW * POSES.length;
const sheetH = cellH * COATS.length;
const sheet = new Uint8Array(sheetW * sheetH * 4);

// A mid grey behind everything: a coat has to read against the taskbar, and a
// checkerboard would make the light coats look better than they are.
for (let i = 0; i < sheetW * sheetH; i++) {
  sheet[i * 4] = 44;
  sheet[i * 4 + 1] = 46;
  sheet[i * 4 + 2] = 52;
  sheet[i * 4 + 3] = 255;
}

COATS.forEach((coat, row) => {
  POSES.forEach((partial, col) => {
    const pose: CatPose = { ...IDLE_POSE, ...partial };
    const scaled = scaleRgba(
      renderScene(pose, [], null, coat),
      SCENE_W,
      SCENE_H,
      cellW,
      cellH,
    );
    const gx = col * cellW;
    const gy = row * cellH;
    for (let y = 0; y < cellH; y++) {
      for (let x = 0; x < cellW; x++) {
        const si = (y * cellW + x) * 4;
        const a = scaled[si + 3] / 255;
        if (a === 0) continue;
        const di = ((gy + y) * sheetW + (gx + x)) * 4;
        for (let c = 0; c < 3; c++) {
          sheet[di + c] = scaled[si + c] * a + sheet[di + c] * (1 - a);
        }
      }
    }
  });
});

writeFileSync("coats.png", encodePng(sheetW, sheetH, sheet));
console.log(
  `wrote coats.png (${sheetW}x${sheetH}), top to bottom: ${COATS.map((c) => c.label).join(", ")}`,
);
