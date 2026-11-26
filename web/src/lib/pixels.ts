// Small pixel-art pictures for the node view. Each row is one line of
// pixels: "#" is ink, "o" is the accent colour, "." is empty.

export type Sprite = readonly string[];

/** A server with 2 units and a light on each. */
export const server: Sprite = [
  "###########",
  "#.........#",
  "#.o.#####.#",
  "#.........#",
  "###########",
  "#.........#",
  "#.o.#####.#",
  "#.........#",
  "###########",
  "..#.....#..",
  ".###...###.",
];

/** A laptop: the client. */
export const laptop: Sprite = [
  "..##########..",
  "..#........#..",
  "..#.oo.....#..",
  "..#.oooo...#..",
  "..#.oo.....#..",
  "..#........#..",
  "..##########..",
  ".############.",
  "##############",
];

/** One line in, three lines out: the load balancer. */
export const splitter: Sprite = [
  "......o......",
  ".....ooo.....",
  "......#......",
  "......#......",
  "..#########..",
  "..#...#...#..",
  "..#...#...#..",
  ".###.###.###.",
  ".###.###.###.",
];

export const spriteSize = (s: Sprite) => ({ w: s[0].length, h: s.length });
