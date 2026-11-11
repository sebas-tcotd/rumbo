import type { StateIndex, VehicleGeometry } from '@rumbo/contracts';
import * as THREE from 'three/webgpu';
import { SCENE } from '../palette.ts';

/** Car meshes: the root follows the body pose; wheels steer and spin under it. */
export interface CarRig {
  root: THREE.Group;
  body: THREE.Group;
  wheels: THREE.Group[];
  spinners: THREE.Object3D[];
}

function material(ghost: boolean, color: number, rough = 0.6): THREE.Material {
  if (ghost) {
    return new THREE.MeshStandardMaterial({
      color: SCENE.ghost,
      transparent: true,
      opacity: 0.32,
      depthWrite: false,
      roughness: 0.5,
    });
  }
  return new THREE.MeshStandardMaterial({ color, roughness: rough, metalness: 0.05, flatShading: true });
}

function addLamps(body: THREE.Group, hx: number, hz: number): void {
  const lamp = new THREE.MeshStandardMaterial({ color: 0xfff6dc, emissive: 0xfff2c8, emissiveIntensity: 0.6 });
  const tail = new THREE.MeshStandardMaterial({ color: 0xb3261e, emissive: 0x7a0f0a, emissiveIntensity: 0.5 });
  for (const side of [-1, 1]) {
    const head = new THREE.Mesh(new THREE.BoxGeometry(0.42, 0.14, 0.05), lamp);
    head.position.set(side * hx * 0.62, 0, -hz - 0.01);
    const rear = new THREE.Mesh(new THREE.BoxGeometry(0.42, 0.14, 0.05), tail);
    rear.position.set(side * hx * 0.62, 0.05, hz + 0.01);
    body.add(head, rear);
  }
}

function buildBody(he: [number, number, number], ghost: boolean): THREE.Group {
  const body = new THREE.Group();
  const [hx, hy, hz] = he;
  const lower = new THREE.Mesh(new THREE.BoxGeometry(hx * 2, hy * 1.25, hz * 2), material(ghost, SCENE.car));
  lower.position.y = -hy * 0.2;
  const cabin = new THREE.Mesh(
    new THREE.BoxGeometry(hx * 1.7, hy * 1.3, hz * 1.05),
    material(ghost, SCENE.glass, 0.25),
  );
  cabin.position.set(0, hy * 0.95, hz * 0.12);
  const roof = new THREE.Mesh(new THREE.BoxGeometry(hx * 1.62, hy * 0.18, hz * 0.8), material(ghost, SCENE.car));
  roof.position.set(0, hy * 1.66, hz * 0.16);
  const spoiler = new THREE.Mesh(new THREE.BoxGeometry(hx * 1.9, 0.08, 0.4), material(ghost, SCENE.carDark));
  spoiler.position.set(0, hy * 0.9, hz * 0.95);
  body.add(lower, cabin, roof, spoiler);
  if (!ghost) addLamps(body, hx, hz);
  body.traverse((o) => {
    if (o instanceof THREE.Mesh) o.castShadow = !ghost;
  });
  return body;
}

/** Builds the player car or its translucent ghost from the sim's vehicle geometry. */
export function buildCar(vehicle: VehicleGeometry, ghost: boolean): CarRig {
  const root = new THREE.Group();
  const body = buildBody(vehicle.halfExtents, ghost);
  root.add(body);
  const wheels: THREE.Group[] = [];
  const spinners: THREE.Object3D[] = [];
  const r = vehicle.wheelRadius;
  const tyreGeo = new THREE.CylinderGeometry(r, r, 0.26, 12);
  tyreGeo.rotateZ(Math.PI / 2);
  const rimGeo = new THREE.CylinderGeometry(r * 0.58, r * 0.58, 0.27, 6);
  rimGeo.rotateZ(Math.PI / 2);
  for (let i = 0; i < 4; i++) {
    const steer = new THREE.Group();
    const spin = new THREE.Group();
    spin.add(new THREE.Mesh(tyreGeo, material(ghost, SCENE.tyre, 0.9)), new THREE.Mesh(rimGeo, material(ghost, SCENE.rim, 0.4)));
    spin.traverse((o) => {
      if (o instanceof THREE.Mesh) o.castShadow = !ghost;
    });
    steer.add(spin);
    root.add(steer);
    wheels.push(steer);
    spinners.push(spin);
  }
  return { root, body, wheels, spinners };
}

/** Per-wheel state fields, in the order of the vehicle's hardpoints (FL, FR, RL, RR). */
const CONTACT_FIELDS = ['contactFL', 'contactFR', 'contactRL', 'contactRR'] as const;
const COMPRESSION_FIELDS = ['compFL', 'compFR', 'compRL', 'compRR'] as const;

/** Wheel placement inputs that do not change during a session. */
export interface WheelSetup {
  hardpoints: [number, number, number][];
  wheelRadius: number;
  suspensionRest: number;
}

/**
 * Poses the four wheels from the latest tick: suspension drop, front steering and spin.
 * Returns the accumulated spin angle so the caller keeps it across frames.
 */
export function placeWheels(
  car: CarRig,
  s: Float32Array,
  I: StateIndex,
  setup: WheelSetup,
  spin: number,
  dt: number,
): number {
  const steer = (-s[I.steerDeg] * Math.PI) / 180;
  const nextSpin = spin - ((s[I.speedKmh] / 3.6) * dt) / setup.wheelRadius;
  for (let i = 0; i < 4; i++) {
    const [x, y, z] = setup.hardpoints[i];
    // Each field by name: the layout comes from the worker at runtime and need not be contiguous.
    const contact = s[I[CONTACT_FIELDS[i]]] > 0.5;
    const drop = contact ? setup.suspensionRest - s[I[COMPRESSION_FIELDS[i]]] : setup.suspensionRest;
    // Wheels stick out of the body a little (visual only).
    car.wheels[i].position.set(x + Math.sign(x) * 0.17, y - drop, z);
    car.wheels[i].rotation.set(0, i < 2 ? steer : 0, 0);
    car.spinners[i].rotation.x = nextSpin;
  }
  return nextSpin;
}
