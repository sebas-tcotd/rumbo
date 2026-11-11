import type { MeshData, ReadyMsg } from '@rumbo/contracts';
import * as THREE from 'three/webgpu';
import { SCENE, SURFACE_COLORS } from '../palette.ts';
import { sampleIndexAt } from './sample-index.ts';

/** Builds a flat-shaded mesh coloured per vertex from the sim's surface tags. */
export function meshFrom(data: MeshData, material: THREE.Material): THREE.Mesh {
  const geo = new THREE.BufferGeometry();
  geo.setAttribute('position', new THREE.BufferAttribute(data.positions, 3));
  const colors = new Float32Array(data.surfaces.length * 3);
  const c = new THREE.Color();
  for (let i = 0; i < data.surfaces.length; i++) {
    c.setHex(SURFACE_COLORS[data.surfaces[i]] ?? SCENE.asphalt);
    colors.set([c.r, c.g, c.b], i * 3);
  }
  geo.setAttribute('color', new THREE.BufferAttribute(colors, 3));
  geo.setIndex(new THREE.BufferAttribute(data.indices, 1));
  geo.computeVertexNormals(); // unshared vertices: flat shading, scale-model look
  return new THREE.Mesh(geo, material);
}

/** Road, walls, decals and the ground plane. */
export function addTrackMeshes(scene: THREE.Scene, ready: ReadyMsg): void {
  const surface = new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.95, metalness: 0 });
  const road = meshFrom(ready.meshes.drivable, surface);
  road.receiveShadow = true;
  const walls = meshFrom(ready.meshes.walls, surface);
  walls.receiveShadow = true;
  walls.castShadow = true;
  const decals = meshFrom(
    ready.meshes.decals,
    new THREE.MeshStandardMaterial({ color: SCENE.dash, roughness: 0.9 }),
  );
  decals.receiveShadow = true;
  scene.add(road, walls, decals);

  const ground = new THREE.Mesh(
    new THREE.PlaneGeometry(4000, 4000),
    new THREE.MeshStandardMaterial({ color: SCENE.ground, roughness: 1 }),
  );
  ground.rotation.x = -Math.PI / 2;
  ground.position.y = ready.groundY;
  ground.receiveShadow = true;
  scene.add(ground);
}

/** Position, right vector and road width of the centerline at lap distance `s`. */
export function centerlineAt(ready: ReadyMsg, s: number) {
  const cl = ready.centerline;
  const { closed, spacing } = ready.track;
  const o = sampleIndexAt(s, spacing, cl.length / 10, closed) * 10;
  return {
    pos: new THREE.Vector3(cl[o], cl[o + 1], cl[o + 2]),
    right: new THREE.Vector3(cl[o + 3], cl[o + 4], cl[o + 5]),
    width: cl[o + 9],
  };
}
