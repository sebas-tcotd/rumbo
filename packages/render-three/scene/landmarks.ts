import type { ReadyMsg } from '@rumbo/contracts';
import * as THREE from 'three/webgpu';
import { SCENE } from '../palette.ts';
import { centerlineAt } from '../track/track-meshes.ts';

function addGantry(scene: THREE.Scene, ready: ReadyMsg, s: number, beamMat: THREE.Material, postMat: THREE.Material) {
  const g = centerlineAt(ready, s);
  const span = g.width + 6;
  for (const side of [-1, 1]) {
    const post = new THREE.Mesh(new THREE.BoxGeometry(0.5, 7, 0.5), postMat);
    post.position.copy(g.pos).addScaledVector(g.right, (span / 2) * side).add(new THREE.Vector3(0, 3.5, 0));
    post.castShadow = true;
    scene.add(post);
  }
  const beam = new THREE.Mesh(new THREE.BoxGeometry(span + 0.5, 0.9, 0.6), beamMat);
  beam.position.copy(g.pos).add(new THREE.Vector3(0, 7, 0));
  beam.quaternion.setFromUnitVectors(new THREE.Vector3(1, 0, 0), new THREE.Vector3(g.right.x, 0, g.right.z).normalize());
  beam.castShadow = true;
  scene.add(beam);
}

/** Gantries (finish on circuits; start and finish on sprints) and amber poles at checkpoints. */
export function addLandmarks(scene: THREE.Scene, ready: ReadyMsg): void {
  const { closed, startS, finishS } = ready.track;
  const dark = new THREE.MeshStandardMaterial({ color: SCENE.gantry, roughness: 0.8 });
  const accent = new THREE.MeshStandardMaterial({ color: SCENE.checkpoint, roughness: 0.6 });
  if (closed) {
    addGantry(scene, ready, 0, accent, dark);
  } else {
    addGantry(scene, ready, startS, accent, dark);
    addGantry(scene, ready, finishS, dark, dark);
  }
  const mids = closed ? ready.checkpoints.slice(1) : ready.checkpoints.slice(1, -1);
  for (const s of mids) {
    const cp = centerlineAt(ready, s);
    for (const side of [-1, 1]) {
      const pole = new THREE.Mesh(new THREE.BoxGeometry(0.35, 2.4, 0.35), accent);
      pole.position.copy(cp.pos).addScaledVector(cp.right, (cp.width / 2 + 0.6) * side).add(new THREE.Vector3(0, 1.2, 0));
      pole.castShadow = true;
      scene.add(pole);
    }
  }
}
