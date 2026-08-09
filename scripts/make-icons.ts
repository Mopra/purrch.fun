// Generates src-tauri/icons/icon.ico and icon.png from the cat artwork.
// Run: node scripts/make-icons.ts
import { writeFileSync, mkdirSync } from "node:fs";
import {
  renderScene,
  IDLE_POSE,
  SCENE_W,
  CAT_X,
  CAT_Y,
} from "../src/lib/render.ts";
import { CAT_W, CAT_H } from "../src/lib/catArt.ts";
import { encodePng, scaleRgba } from "./png.ts";

// crop the scene to the cat and pad to a square
const PAD = 1;
const SQ = Math.max(CAT_W, CAT_H) + PAD * 2; // 28
const scene = renderScene({ ...IDLE_POSE, tail: "up" });
const square = new Uint8Array(SQ * SQ * 4);
const offX = Math.floor((SQ - CAT_W) / 2);
const offY = Math.floor((SQ - CAT_H) / 2);
for (let y = 0; y < CAT_H; y++) {
  for (let x = 0; x < CAT_W; x++) {
    const si = ((CAT_Y + y) * SCENE_W + CAT_X + x) * 4;
    const di = ((offY + y) * SQ + (offX + x)) * 4;
    for (let c = 0; c < 4; c++) square[di + c] = scene[si + c];
  }
}

function bmpEntry(size: number): Buffer {
  const px = scaleRgba(square, SQ, SQ, size, size);
  const header = Buffer.alloc(40);
  header.writeUInt32LE(40, 0); // biSize
  header.writeInt32LE(size, 4); // biWidth
  header.writeInt32LE(size * 2, 8); // biHeight (XOR + AND masks)
  header.writeUInt16LE(1, 12); // biPlanes
  header.writeUInt16LE(32, 14); // biBitCount
  header.writeUInt32LE(0, 16); // biCompression
  header.writeUInt32LE(size * size * 4, 20); // biSizeImage

  // BGRA, bottom-up
  const xor = Buffer.alloc(size * size * 4);
  for (let y = 0; y < size; y++) {
    const srcRow = size - 1 - y;
    for (let x = 0; x < size; x++) {
      const si = (srcRow * size + x) * 4;
      const di = (y * size + x) * 4;
      xor[di] = px[si + 2];
      xor[di + 1] = px[si + 1];
      xor[di + 2] = px[si];
      xor[di + 3] = px[si + 3];
    }
  }
  // 1bpp AND mask, all zero (alpha channel governs transparency)
  const andStride = Math.ceil(size / 32) * 4;
  const and = Buffer.alloc(andStride * size);
  return Buffer.concat([header, xor, and]);
}

const sizes = [16, 24, 32, 48, 64, 128, 256];
const entries = sizes.map(bmpEntry);

const dir = Buffer.alloc(6 + sizes.length * 16);
dir.writeUInt16LE(0, 0);
dir.writeUInt16LE(1, 2); // ICO
dir.writeUInt16LE(sizes.length, 4);
let offset = dir.length;
sizes.forEach((size, i) => {
  const e = 6 + i * 16;
  dir[e] = size >= 256 ? 0 : size;
  dir[e + 1] = size >= 256 ? 0 : size;
  dir[e + 2] = 0; // palette
  dir[e + 3] = 0; // reserved
  dir.writeUInt16LE(1, e + 4); // planes
  dir.writeUInt16LE(32, e + 6); // bpp
  dir.writeUInt32LE(entries[i].length, e + 8);
  dir.writeUInt32LE(offset, e + 12);
  offset += entries[i].length;
});

mkdirSync("src-tauri/icons", { recursive: true });
writeFileSync("src-tauri/icons/icon.ico", Buffer.concat([dir, ...entries]));

const png512 = scaleRgba(square, SQ, SQ, 512, 512);
writeFileSync("src-tauri/icons/icon.png", encodePng(512, 512, png512));

console.log("wrote src-tauri/icons/icon.ico and icon.png");
