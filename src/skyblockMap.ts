/**
 * Classic default Skyblock starter island block map.
 * Coordinates: y-up, unit = 1 block.
 */
export type BlockId =
  | "grass"
  | "dirt"
  | "log"
  | "leaves"
  | "chest"
  | "planks"
  | "sand"
  | "cobble"
  | "water"
  | "lava"
  | "bedrock";

export type Block = Readonly<{ x: number; y: number; z: number; id: BlockId }>;

const SURFACE: ReadonlyArray<readonly [number, number]> = [
  [-1, -2],
  [0, -2],
  [1, -2],
  [-2, -1],
  [-1, -1],
  [0, -1],
  [1, -1],
  [2, -1],
  [-2, 0],
  [-1, 0],
  [0, 0],
  [1, 0],
  [2, 0],
  [-2, 1],
  [-1, 1],
  [0, 1],
  [1, 1],
  [2, 1],
  [-1, 2],
  [0, 2],
  [1, 2],
];

const LEAVES: ReadonlyArray<readonly [number, number, number]> = [
  [-1, 4, -1],
  [0, 4, -1],
  [1, 4, -1],
  [-1, 4, 0],
  [1, 4, 0],
  [-1, 4, 1],
  [0, 4, 1],
  [1, 4, 1],
  [-1, 5, -1],
  [0, 5, -1],
  [1, 5, -1],
  [-1, 5, 0],
  [0, 5, 0],
  [1, 5, 0],
  [-1, 5, 1],
  [0, 5, 1],
  [1, 5, 1],
  [0, 6, 0],
  [-1, 6, 0],
  [1, 6, 0],
  [0, 6, -1],
  [0, 6, 1],
];

const GEN: ReadonlyArray<readonly [number, number, number, BlockId]> = [
  [3, 0, -1, "sand"],
  [4, 0, -1, "sand"],
  [5, 0, -1, "sand"],
  [3, 0, 0, "sand"],
  [4, 0, 0, "cobble"],
  [5, 0, 0, "sand"],
  [3, 0, 1, "sand"],
  [4, 0, 1, "sand"],
  [5, 0, 1, "sand"],
  [3, -1, -1, "dirt"],
  [4, -1, 0, "dirt"],
  [5, -1, 1, "dirt"],
  [4, 1, -1, "water"],
  [4, 1, 1, "lava"],
];

function buildBlocks(): Block[] {
  const blocks: Block[] = [];
  for (const [x, z] of SURFACE) {
    blocks.push({ x, y: 0, z, id: "grass" });
    blocks.push({ x, y: -1, z, id: "dirt" });
    if ((x + z) % 2 === 0 || Math.abs(x) + Math.abs(z) <= 2) {
      blocks.push({ x, y: -2, z, id: "dirt" });
    }
  }
  blocks.push({ x: 0, y: -3, z: 0, id: "dirt" });
  blocks.push({ x: 1, y: -3, z: 0, id: "dirt" });
  blocks.push({ x: 0, y: -3, z: 1, id: "dirt" });
  blocks.push({ x: 0, y: -4, z: 0, id: "bedrock" });
  for (let y = 1; y <= 4; y++) blocks.push({ x: 0, y, z: 0, id: "log" });
  for (const [x, y, z] of LEAVES) blocks.push({ x, y, z, id: "leaves" });
  blocks.push({ x: 1, y: 1, z: 1, id: "chest" });
  blocks.push({ x: -1, y: 1, z: 1, id: "planks" });
  for (const [x, y, z, id] of GEN) blocks.push({ x, y, z, id });
  return blocks;
}

/** Precomputed classic skyblock map — never rebuilt at runtime. */
export const CLASSIC_SKYBLOCK: readonly Block[] = Object.freeze(buildBlocks());

export function classicSkyblockBlocks(): readonly Block[] {
  return CLASSIC_SKYBLOCK;
}
