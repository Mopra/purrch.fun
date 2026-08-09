// Purrch — pixel cat artwork.
// All art is authored as character grids; each char maps to a palette color.
// '.' is transparent. The cat is 24x26 pixels, drawn chibi style: big round
// head, tiny body, oversized eyes.

export const PALETTE: Record<string, string> = {
  o: "#3a2434", // soft dark-plum outline
  b: "#f5a05c", // orange coat
  d: "#dd7e44", // darker orange (tabby stripes / shading)
  w: "#fff6e8", // cream white (muzzle, chest, paws, tail tip)
  p: "#ffb3c1", // inner ear pink
  n: "#e8616e", // nose / heart pink-red
  k: "#33222f", // eye
  i: "#ffffff", // eye shine
  r: "#f9a8a2", // blush
  z: "#9db8d6", // sleepy-z blue-grey
  y: "#7ecfc0", // yarn mint
  t: "#4b9e90", // yarn thread / shading
};

export type Grid = string[];

export interface Layer {
  grid: Grid;
  ox: number; // offset within the cat's 24x26 box
  oy: number;
}

export const CAT_W = 24;
export const CAT_H = 26;

// ---------------------------------------------------------------------------
// Base sitting body (no eyes, no tail — those are separate layers)
// ---------------------------------------------------------------------------
export const BODY_SIT: Grid = [
  "...oo..........oo.......",
  "..obbo........obbo......",
  "..obpbo.ooooo.obpbo.....",
  ".obppboobbbbboobppbo....",
  ".obbbbbbdbdbdbbbbbbo....",
  ".obbbbbbbbdbbbbbbbbo....",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbwwwwbbbbbbbbo..",
  "obbbbbbbwwwwwwbbbbbbbo..",
  ".obbbbbbwwwwwwbbbbbbo...",
  ".obbbbbbbwwwwbbbbbbbo...",
  "..obbbbbbbbbbbbbbbbo....",
  "...oobbbbbbbbbbbboo.....",
  "....obbbbbbbbbbbbo......",
  "...obbbwwwwwwbbbbo......",
  "...obbwwwwwwwwbbbbo.....",
  "..obbwwwwwwwwwwbbbbo....",
  "..obbwwwwwwwwwwbbbbo....",
  "..obbwwwwwwwwwwbbbdo....",
  "..obbwwwwwwwwwwbbbbo....",
  "..obwwbobwwbobbbbbbo....",
  "...oo...oo....oooo......",
];

// ---------------------------------------------------------------------------
// Scruffed — hanging from an invisible hand by the loose skin at the nape.
//
// Rows 6–15 are pixel-for-pixel the sitting head so the eye, face and blush
// layers keep their offsets; everything else is redrawn. What sells it is all
// the things a sitting cat isn't doing: a tuft of loose skin pinched up between
// the ears, shoulders hanging off it, forelegs down long and slack with the
// paws splayed limp at the ends, the hind end curled up out of the way, and a
// tail with nothing to do but droop. Taller than the sitting cat, which is why
// the airborne poses hang from higher up the window (see `render.ts`).
//
// Row 16 is the pinch point: `HANG_PIVOT` in the renderer bends everything
// below it sideways as the cat swings.
// ---------------------------------------------------------------------------
export const BODY_HANG: Grid = [
  "...oo....oooo..oo.......", // the scruff, bunched up between the ears
  "..obbo...obbo.obbo......",
  "..obpbo.obbbboobpbo.....",
  ".obppboobbbbboobppbo....",
  ".obbbbbbdbdbdbbbbbbo....",
  ".obbbbbbbbdbbbbbbbbo....",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbwwwwbbbbbbbbo..",
  "obbbbbbbwwwwwwbbbbbbbo..",
  ".obbbbbbwwwwwwbbbbbbo...",
  ".obbbbbbbwwwwbbbbbbbo...",
  "..obbbbbbbbbbbbbbbbo....",
  "...oobbbbbbbbbbbboo.....", // 16 — the hinge everything below swings on
  "....obbbbbbbbbbbbo......", // neck pulled thin...
  "...obbbbbbbbbbbbbbo.....", // ...with the shoulders slung under it
  "...obbbowwwwwwobbbo.....", // forelegs peel off the chest...
  "...obbbowwwwwwobbbo.....",
  "...obbbowwwwwwobbbo.oo..", // tail slips out past the shoulder
  "...obbbowwwwwwobbboobbo.",
  "...obbboowwwwoobbboobbo.",
  "...obbbo.oooo.obbboobbo.", // hind end curled up, well short of the paws
  "...obbbo......obbboobbo.", // ...leaving the forelegs hanging alone
  "...obbbo......obbboobbo.",
  "..obwwwo......obbboowwo.", // paws hanging slack, toes turned out, and
  "..owwwwo......owwwboooo.", // never level — nothing a cat stands on is
  "..oooooo......owwwwo....", // holding either of them up
  "..............oooooo....",
];

/** Row of `BODY_HANG` the scruff grips — nothing above it swings. */
export const HANG_PIVOT = 16;

// ---------------------------------------------------------------------------
// Dropped — the cat has twisted upright and is reaching for the floor. Same
// height as the hanging pose so letting go doesn't jog the art vertically;
// the difference is all in the legs, which swing out wide and stiff with the
// toes spread for the landing.
// ---------------------------------------------------------------------------
export const BODY_FALL: Grid = [
  "...oo..........oo.......",
  "..obbo........obbo......",
  "..obpbo.ooooo.obpbo.....",
  ".obppboobbbbboobppbo....",
  ".obbbbbbdbdbdbbbbbbo....",
  ".obbbbbbbbdbbbbbbbbo....",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbwwwwbbbbbbbbo..",
  "obbbbbbbwwwwwwbbbbbbbo..",
  ".obbbbbbwwwwwwbbbbbbo...",
  ".obbbbbbbwwwwbbbbbbbo...",
  "..obbbbbbbbbbbbbbbbo....",
  "...oobbbbbbbbbbbboo.....",
  "...obbbbbbbbbbbbbbo.....",
  "...obbbwwwwwwwwbbbo.....",
  "...obbbwwwwwwwwbbbo.....",
  "...obbbwwwwwwwwbbbo.....",
  "...obbwwwwwwwwwwbbo.....",
  "...obbwwwwwwwwwwbbo.....",
  "..obbbwwwwwwwwwwbbbo....",
  "..obbbbbbbbbbbbbbbbo....",
  "..obbbbboooooobbbbbo....", // legs part
  ".obbbbbo......obbbbbo...", // ...and brace outwards
  ".obbbbbo......obbbbbo...",
  ".owwwwwo......owwwwwo...",
  ".owowowo......owowowo...", // toes spread for the impact
  ".ooooooo......ooooooo...",
];

// ---------------------------------------------------------------------------
// Touchdown — everything compresses into the floor for a couple of frames.
// Five rows shorter than the sitting cat: the legs fold away entirely, the head
// drops with them, and what was a body spreads out into a pressed-flat pile
// with the paws squeezed out either side of it.
// ---------------------------------------------------------------------------
export const BODY_LAND: Grid = [
  "...oo..........oo.......",
  "..obbo........obbo......",
  "..obpbo.ooooo.obpbo.....",
  ".obppboobbbbboobppbo....",
  ".obbbbbbdbdbdbbbbbbo....",
  ".obbbbbbbbdbbbbbbbbo....",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbbbbbbbwwwwbbbbbbbbo..",
  "obbbbbbbwwwwwwbbbbbbbo..",
  ".obbbbbbwwwwwwbbbbbbo...",
  ".obbbbbbbwwwwbbbbbbbo...",
  "..obbbbbbbbbbbbbbbbo....",
  "..obbbbbbbbbbbbbbbbo....",
  "obbbbbbbbbbbbbbbbbbbbo..",
  "obbwwwwwwwwwwwwwwwwbbo..",
  "owwwobwwwwwwwwwwbowwwo..", // paws squeezed out to the sides
  "oooooooooooooooooooooo..",
];

// ---------------------------------------------------------------------------
// Perked ears — swapped in while the cat is being spoken to.
//
// An overlay rather than a fourth body: the only thing that changes when a cat
// hears its name is the ears, and a whole 26-row copy of the sitting cat to
// move two pixels would be a copy that quietly drifts out of step with the one
// everything else is drawn against.
//
// It sits two rows above the cat and redraws the ears from the tip down, so the
// pixels underneath — the flat ears of `BODY_SIT` — end up entirely covered
// rather than showing through as a second pair. The bottom row it paints is the
// widest, which is what makes the taller ear read as one shape instead of a
// pinched one; the row below that is already the right width in the body, so
// it's left alone.
// ---------------------------------------------------------------------------
export const EARS_PERKED: Layer = {
  ox: 0,
  oy: -2,
  grid: [
    "...oo..........oo.......",
    "..obbo........obbo......",
    "..obpbo.......obpbo.....",
    "..obpbo.......obpbo.....",
    ".obppbo.......obppbo....",
  ],
};

// ---------------------------------------------------------------------------
// Eyes (positioned in cat coords; ox lets us shift the gaze left/right)
// ---------------------------------------------------------------------------
export const EYES_OPEN: Layer = {
  ox: 5,
  oy: 7,
  grid: [
    "ik........ik",
    "kk........kk",
    "kk........kk",
  ],
};

export const EYES_CLOSED: Layer = {
  ox: 5,
  oy: 9,
  grid: ["oo........oo"],
};

// happy ^ ^ arcs
export const EYES_HAPPY: Layer = {
  ox: 4,
  oy: 7,
  grid: [
    ".oo.......oo.",
    "o..o.....o..o",
  ],
};

// ---------------------------------------------------------------------------
// Face (nose, mouth, blush) — static
// ---------------------------------------------------------------------------
export const FACE: Layer = {
  ox: 3,
  oy: 11,
  grid: [
    "rr.....nn.....rr",
    "......o..o......",
    ".......oo.......",
  ],
};

// extra blush when petted
export const BLUSH_BIG: Layer = {
  ox: 2,
  oy: 10,
  grid: [
    "rr.............rr",
    "rrr...........rrr",
  ],
};

// ---------------------------------------------------------------------------
// Grooming paw (two frames: raised to cheek / lowered to mouth)
// ---------------------------------------------------------------------------
export const PAW_UP: Layer = {
  ox: 14,
  oy: 11,
  grid: [
    ".oo.",
    "owwo",
    "owwo",
    ".oo.",
  ],
};

export const PAW_DOWN: Layer = {
  ox: 13,
  oy: 13,
  grid: [
    ".oo.",
    "owwo",
    "owwo",
    ".oo.",
  ],
};

// ---------------------------------------------------------------------------
// Play paws — front-left leg, cocked against the chest then swiped out low
// at the yarn ball. Both connect back to the body edge so the arm reads.
// ---------------------------------------------------------------------------
export const PAW_READY: Layer = {
  ox: 1,
  oy: 16,
  grid: [
    ".oo.",
    "owwo",
    "owwo",
    ".oo.",
  ],
};

export const PAW_SWAT: Layer = {
  ox: -2,
  oy: 21,
  grid: [
    ".oo..",
    "owwo.",
    "owwbb", // right edge stays coat-coloured so the arm merges into the body
    ".oobb",
    "..ooo",
  ],
};

// ---------------------------------------------------------------------------
// Tail — generated from a quadratic curve so the swish animates smoothly.
// Anchored behind the body on the right, tip dipped in cream.
// ---------------------------------------------------------------------------
function emptyGrid(w: number, h: number): string[][] {
  return Array.from({ length: h }, () => Array(w).fill("."));
}

function plot(cells: string[][], x: number, y: number, ch: string) {
  if (y >= 0 && y < cells.length && x >= 0 && x < cells[0].length) {
    cells[y][x] = ch;
  }
}

function makeTail(tipX: number, tipY: number, ctrlX: number, ctrlY: number): Grid {
  const cells = emptyGrid(CAT_W, CAT_H);
  const x0 = 17.5;
  const y0 = 23.5; // root, tucked behind the body
  const steps = 40;
  for (let s = 0; s <= steps; s++) {
    const t = s / steps;
    const mt = 1 - t;
    const x = mt * mt * x0 + 2 * mt * t * ctrlX + t * t * tipX;
    const y = mt * mt * y0 + 2 * mt * t * ctrlY + t * t * tipY;
    const ch = t > 0.78 ? "w" : "b";
    // 2px-thick stroke
    plot(cells, Math.round(x), Math.round(y), ch);
    plot(cells, Math.round(x + 1), Math.round(y), ch);
  }
  // auto-outline: any empty cell touching tail pixels becomes outline
  const out = cells.map((row) => [...row]);
  for (let y = 0; y < CAT_H; y++) {
    for (let x = 0; x < CAT_W; x++) {
      if (cells[y][x] !== ".") continue;
      let touch = false;
      for (let dy = -1; dy <= 1 && !touch; dy++) {
        for (let dx = -1; dx <= 1 && !touch; dx++) {
          const ny = y + dy;
          const nx = x + dx;
          if (
            ny >= 0 &&
            ny < CAT_H &&
            nx >= 0 &&
            nx < CAT_W &&
            (cells[ny][nx] === "b" || cells[ny][nx] === "w")
          ) {
            touch = true;
          }
        }
      }
      if (touch) out[y][x] = "o";
    }
  }
  return out.map((row) => row.join(""));
}

export const TAIL_DOWN: Grid = makeTail(22, 24, 23, 25);
export const TAIL_MID: Grid = makeTail(23, 19, 24, 23);
export const TAIL_UP: Grid = makeTail(22, 14, 25, 20);

// ---------------------------------------------------------------------------
// Particles (drawn in scene coords, above/around the cat)
// ---------------------------------------------------------------------------
export const HEART: Grid = [
  ".n.n.",
  "nnnnn",
  "nnnnn",
  ".nnn.",
  "..n..",
];

export const HEART_SMALL: Grid = [
  "n.n",
  "nnn",
  ".n.",
];

export const ZZ_BIG: Grid = [
  "zzzz",
  "..z.",
  ".z..",
  "zzzz",
];

export const ZZ_SMALL: Grid = [
  "zzz",
  ".z.",
  "zzz",
];

// ---------------------------------------------------------------------------
// Agent-state particles — the cat's tell for what the bridged model is doing.
// ---------------------------------------------------------------------------

/** Thought dot: drifts up while the model is reasoning. */
export const THINK_DOT: Grid = [
  ".z.",
  "zzz",
  ".z.",
];

/**
 * Sound reaching the ear. Drifts *towards* the cat rather than away from it,
 * which is the whole difference between a cat that is listening to you and a
 * cat that is saying something.
 */
export const EAR_WAVE: Grid = [
  "z.",
  ".z",
  ".z",
  "z.",
];

export const EAR_WAVE_SMALL: Grid = [
  "z.",
  ".z",
  "z.",
];

/** Spark: pops when a tool call lands. */
export const SPARK: Grid = [
  ".i.",
  "iii",
  ".i.",
];

/** Dust knocked out from under the paws on a hard landing. */
export const PUFF: Grid = [
  ".oo.",
  "owwo",
  ".oo.",
];

export const PUFF_SMALL: Grid = [
  ".o.",
  "owo",
  ".o.",
];

/** Something went wrong. */
export const OOPS: Grid = [
  "n.n",
  ".n.",
  "n.n",
];

// ---------------------------------------------------------------------------
// The gift — a mouse, laid on the floor beside the cat's paws.
//
// This is the whole "come back to the desk and there's a pile by the door"
// layer, and it has to work with nobody reading anything: a cat sitting next
// to nothing has found nothing, and a cat sitting next to two mice has been
// out twice. It's drawn nose-left with the tail trailing away from the cat,
// because a mouse pointed at the cat reads as alive.
// ---------------------------------------------------------------------------
// The ear is what makes it a mouse rather than a bread roll: at this size the
// eye is one pixel and the tail is two, so the round ear on top is carrying
// the whole silhouette. Everything else is a lump.
export const MOUSE: Grid = [
  ".....ooo...",
  "....opppo..",
  ".oowwwwwoo.",
  "onwwwwwwwo.", // nose at the front, so it's pointed away from the cat
  ".owwwwwwwoo",
  "..ooooooopp", // and the tail trails off behind it
];

export const MOUSE_W = 11;
export const MOUSE_H = 6;

/**
 * Where each mouse in the pile lies, in scene pixels.
 *
 * Stacked rather than laid out in a row, because there is nowhere to lay them
 * in a row: the cat starts at x=16 and the scene dissolves its own left edge
 * (see `EDGE_FADE`), which leaves about one mouse's width of floor. So they go
 * on top of each other, slightly askew, which is what a pile is.
 *
 * Three is as many as get drawn. Past that it stops reading as a count and
 * starts reading as clutter, and the number is on the panel anyway.
 */
export const PILE: { x: number; y: number }[] = [
  { x: 6, y: 34 },
  { x: 4, y: 29 },
  { x: 7, y: 24 },
];

// ---------------------------------------------------------------------------
// Yarn ball — one round 7x7 body with the wound thread drawn as diagonals.
// The three frames step the diagonals across by one pixel, so cycling them
// forwards/backwards makes the ball look like it is rolling that way.
// ---------------------------------------------------------------------------
export const YARN_FRAMES: Grid[] = [
  [
    "..ooo..",
    ".oyyto.",
    "oytyyto",
    "oyytyyo",
    "otyytyo",
    ".otyyo.",
    "..ooo..",
  ],
  [
    "..ooo..",
    ".otyyo.",
    "oyytyyo",
    "otyytyo",
    "oytyyto",
    ".oytyo.",
    "..ooo..",
  ],
  [
    "..ooo..",
    ".oytyo.",
    "otyytyo",
    "oytyyto",
    "oyytyyo",
    ".oyyto.",
    "..ooo..",
  ],
];

export const YARN_W = 7;
export const YARN_H = 7;
