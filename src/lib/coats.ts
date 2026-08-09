// The cat's coat — the only part of the palette a user can change.
//
// A coat remaps four entries: the outline, the fur, its shading, and the cream
// markings (muzzle, chest, socks, tail tip). Everything else — eyes, nose,
// blush, hearts, yarn — is shared across every coat, so a ginger cat and a
// black cat are recognisably the same cat wearing a different colour.
//
// `accent` is the one colour that leaves the canvas: it tints this cat's panel,
// menu and buttons, so in a colony you can tell whose window you're looking at.
// It is always light enough to carry the plum text used on those buttons, which
// is why a dark coat like midnight can't just reuse its fur here.

export interface Coat {
  id: string;
  label: string;
  /** Palette 'o' — outline. */
  line: string;
  /** Palette 'b' — the coat itself. */
  fur: string;
  /** Palette 'd' — tabby stripes and shading. */
  shade: string;
  /** Palette 'w' — muzzle, chest, paws, tail tip. */
  trim: string;
  /** UI tint for this cat's panel. Must read against the dark plum panel. */
  accent: string;
}

export const COATS: Coat[] = [
  {
    id: "marmalade",
    label: "marmalade",
    line: "#3a2434",
    fur: "#f5a05c",
    shade: "#dd7e44",
    trim: "#fff6e8",
    accent: "#f5a05c",
  },
  {
    id: "honey",
    label: "honey",
    line: "#4a3520",
    fur: "#ffd68a",
    shade: "#e0ac52",
    trim: "#fff6e8",
    accent: "#ffd68a",
  },
  {
    id: "snow",
    label: "snow",
    line: "#5a4a52",
    fur: "#f4e6d8",
    shade: "#dcc7b4",
    trim: "#fffdf7",
    accent: "#f4e6d8",
  },
  {
    id: "ash",
    label: "ash",
    line: "#2f3340",
    fur: "#9aa3b8",
    shade: "#79839b",
    trim: "#f0f3fa",
    accent: "#b8c1d6",
  },
  {
    id: "midnight",
    label: "midnight",
    line: "#1b1724",
    fur: "#4e4763",
    shade: "#3a3450",
    trim: "#cfc7e4",
    accent: "#9d8fc7",
  },
  {
    id: "mocha",
    label: "mocha",
    line: "#3d2a20",
    fur: "#b08968",
    shade: "#8c6a4e",
    trim: "#fff6e8",
    accent: "#d3ab84",
  },
  {
    id: "mint",
    label: "mint",
    line: "#24443f",
    fur: "#7ecfc0",
    shade: "#4b9e90",
    trim: "#fff6e8",
    accent: "#7ecfc0",
  },
  {
    id: "ocean",
    label: "ocean",
    line: "#1f3350",
    fur: "#7aa8e0",
    shade: "#5480b8",
    trim: "#fff6e8",
    accent: "#7aa8e0",
  },
  {
    id: "bubblegum",
    label: "bubblegum",
    line: "#4a2436",
    // Kept a shade deeper than the inner-ear pink, or the ears vanish.
    fur: "#ff9db0",
    shade: "#e07a92",
    trim: "#fff6e8",
    accent: "#ff9db0",
  },
];

/** What the very first cat wears — the original Purrch ginger. */
export const DEFAULT_COAT = "marmalade";

export function coatById(id: string | null | undefined): Coat {
  return COATS.find((c) => c.id === id) ?? COATS[0];
}
