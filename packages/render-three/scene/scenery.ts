import type { ReadyMsg } from '@rumbo/contracts';
import * as THREE from 'three/webgpu';
import { SCENE } from '../palette.ts';

const Y = new THREE.Vector3(0, 1, 0);
/** Size (m) of the grid cells marked as drivable so no vegetation lands on any street. */
const CELL = 6;
const PAD = 220;

/** Seeded PRNG for decoration only. Never the simulation's PCG: it must not affect the race. */
export function mulberry32(seed: number): () => number {
  let state = seed;
  return () => {
    state |= 0;
    state = (state + 0x6d2b79f5) | 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Cells occupied by the drivable surface (side streets and junctions of a network included). */
function drivableCells(ready: ReadyMsg): (x: number, z: number) => boolean {
  const occupied = new Set<string>();
  const dp = ready.meshes.drivable.positions;
  for (let i = 0; i < dp.length; i += 3) {
    const cx = Math.floor(dp[i] / CELL);
    const cz = Math.floor(dp[i + 2] / CELL);
    for (let dx = -1; dx <= 1; dx++) for (let dz = -1; dz <= 1; dz++) occupied.add(`${cx + dx},${cz + dz}`);
  }
  return (x, z) => occupied.has(`${Math.floor(x / CELL)},${Math.floor(z / CELL)}`);
}

function centerlinePoints(ready: ReadyMsg): THREE.Vector2[] {
  const cl = ready.centerline;
  const pts: THREE.Vector2[] = [];
  for (let i = 0; i < cl.length / 10; i += 4) pts.push(new THREE.Vector2(cl[i * 10], cl[i * 10 + 2]));
  return pts;
}

function bounds(pts: THREE.Vector2[]) {
  const box = new THREE.Box2().setFromPoints(pts);
  box.min.subScalar(PAD);
  box.max.addScalar(PAD);
  return box;
}

interface Placements {
  trees: THREE.Matrix4[];
  trunks: THREE.Matrix4[];
  rocks: THREE.Matrix4[];
}

/** Scatters trees and rocks off the track with a fixed seed (same layout on every load). */
function place(ready: ReadyMsg): Placements {
  const rand = mulberry32(20261103);
  const pts = centerlinePoints(ready);
  const box = bounds(pts);
  const onRoad = drivableCells(ready);
  const clearance = (x: number, z: number) => {
    let best = Infinity;
    for (const p of pts) best = Math.min(best, (p.x - x) ** 2 + (p.y - z) ** 2);
    return Math.sqrt(best);
  };
  const out: Placements = { trees: [], trunks: [], rocks: [] };
  const m = new THREE.Matrix4();
  const size = box.getSize(new THREE.Vector2());
  // Even density: more attempts on bigger tracks.
  const attempts = Math.min(7000, Math.max(900, Math.round((size.x * size.y) / 2600)));
  for (let k = 0; k < attempts; k++) {
    const x = box.min.x + rand() * size.x;
    const z = box.min.y + rand() * size.y;
    if (clearance(x, z) < 26 || onRoad(x, z)) continue;
    const y = ready.groundY;
    if (rand() < 0.72) {
      const s = 1.6 + rand() * 2.4;
      const turn = new THREE.Quaternion().setFromAxisAngle(Y, rand() * 6.28);
      m.compose(new THREE.Vector3(x, y + s * 1.2 + 1.4, z), turn, new THREE.Vector3(s, s * 1.25, s));
      out.trees.push(m.clone());
      m.compose(new THREE.Vector3(x, y + 1.0, z), new THREE.Quaternion(), new THREE.Vector3(1, 2.0, 1));
      out.trunks.push(m.clone());
    } else {
      const s = 0.6 + rand() * 1.8;
      const tilt = new THREE.Quaternion().setFromEuler(new THREE.Euler(rand(), rand() * 6.28, rand()));
      m.compose(new THREE.Vector3(x, y + s * 0.4, z), tilt, new THREE.Vector3(s * 1.3, s * 0.8, s));
      out.rocks.push(m.clone());
    }
  }
  return out;
}

function instanced(scene: THREE.Scene, geo: THREE.BufferGeometry, mat: THREE.Material, list: THREE.Matrix4[]) {
  const mesh = new THREE.InstancedMesh(geo, mat, list.length);
  list.forEach((mm, i) => mesh.setMatrixAt(i, mm));
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  scene.add(mesh);
}

/** Trees and rocks off the track. */
export function addScenery(scene: THREE.Scene, ready: ReadyMsg): void {
  const { trees, trunks, rocks } = place(ready);
  const crown = new THREE.IcosahedronGeometry(1, 0);
  const crownMats = [SCENE.tree, SCENE.treeDark].map(
    (c) => new THREE.MeshStandardMaterial({ color: c, roughness: 1, flatShading: true }),
  );
  const half = Math.floor(trees.length / 2);
  instanced(scene, crown, crownMats[0], trees.slice(0, half));
  instanced(scene, crown, crownMats[1], trees.slice(half));
  const trunkMat = new THREE.MeshStandardMaterial({ color: SCENE.trunk, roughness: 1 });
  instanced(scene, new THREE.CylinderGeometry(0.18, 0.26, 1, 5), trunkMat, trunks);
  const rockMat = new THREE.MeshStandardMaterial({ color: SCENE.rock, roughness: 1, flatShading: true });
  instanced(scene, new THREE.DodecahedronGeometry(1, 0), rockMat, rocks);
}
