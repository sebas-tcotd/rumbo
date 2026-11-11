// Scene palette: Lima's grey sky, coastal earth, kerbs and concrete.
// Surfaces come tagged from the simulation; they are only coloured here.

export const SCENE = {
  sky: 0xc5c9c6,
  ground: 0xb39c77,
  asphalt: 0x3b3e43,
  kerbA: 0xc4402f,
  kerbB: 0xeeeae0,
  shoulder: 0x8d8574,
  wall: 0xa9a69d,
  startLine: 0xf2f0ea,
  dash: 0xe9e4d6,
  tree: 0x56704b,
  treeDark: 0x46603f,
  trunk: 0x6b5a46,
  rock: 0x8f8676,
  car: 0xe3a21a,
  carDark: 0x9c6b0b,
  glass: 0x2f3a40,
  tyre: 0x24282b,
  rim: 0xbfc3c4,
  ghost: 0xf4f5f2,
  gantry: 0x2f3338,
  checkpoint: 0xe3a21a,
} as const;

/** Color por etiqueta de superficie (orden de `Surface` en sim-core). */
export const SURFACE_COLORS = [
  SCENE.asphalt,
  SCENE.kerbA,
  SCENE.kerbB,
  SCENE.shoulder,
  SCENE.wall,
  SCENE.startLine,
];
