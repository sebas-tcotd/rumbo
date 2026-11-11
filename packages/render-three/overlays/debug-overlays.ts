import type { OverlayKind, ReadyMsg } from '@rumbo/contracts';
import * as THREE from 'three/webgpu';
import type { CarRig } from '../car/car-rig.ts';
import { centerlineAt } from '../track/track-meshes.ts';

/** Lifted above the road so the lines are not z-fighting with it. */
const LIFT = 0.15;
const AXIS_COLOR = 0x1f6fd1;
const CHECKPOINT_COLOR = 0xd12f1f;
const COLLISION_COLOR = 0x18a058;

function lineMaterial(color: number): THREE.LineBasicMaterial {
  return new THREE.LineBasicMaterial({ color, depthTest: false, transparent: true });
}

/** Sim centerline as a polyline (closed on circuits). */
function buildAxis(ready: ReadyMsg): THREE.Object3D {
  const cl = ready.centerline;
  const points: number[] = [];
  for (let o = 0; o < cl.length; o += 10) points.push(cl[o], cl[o + 1] + LIFT, cl[o + 2]);
  if (ready.track.closed && points.length >= 3) points.push(points[0], points[1], points[2]);
  const geo = new THREE.BufferGeometry();
  geo.setAttribute('position', new THREE.Float32BufferAttribute(points, 3));
  const line = new THREE.Line(geo, lineMaterial(AXIS_COLOR));
  line.renderOrder = 10;
  return line;
}

/** A line across the road at each checkpoint distance `s`. */
function buildCheckpoints(ready: ReadyMsg): THREE.Object3D {
  const points: number[] = [];
  for (const s of ready.checkpoints) {
    const cp = centerlineAt(ready, s);
    const a = cp.pos.clone().addScaledVector(cp.right, -cp.width / 2);
    const b = cp.pos.clone().addScaledVector(cp.right, cp.width / 2);
    points.push(a.x, a.y + LIFT, a.z, b.x, b.y + LIFT, b.z);
  }
  const geo = new THREE.BufferGeometry();
  geo.setAttribute('position', new THREE.Float32BufferAttribute(points, 3));
  const lines = new THREE.LineSegments(geo, lineMaterial(CHECKPOINT_COLOR));
  lines.renderOrder = 10;
  return lines;
}

/** Wireframe of the walls mesh (world) plus the chassis box, which rides on the car root. */
function buildCollision(ready: ReadyMsg, car: CarRig): { world: THREE.Object3D; onCar: THREE.Object3D } {
  const walls = ready.meshes.walls;
  const geo = new THREE.BufferGeometry();
  geo.setAttribute('position', new THREE.BufferAttribute(walls.positions, 3));
  geo.setIndex(new THREE.BufferAttribute(walls.indices, 1));
  const world = new THREE.LineSegments(new THREE.WireframeGeometry(geo), lineMaterial(COLLISION_COLOR));
  geo.dispose();
  const [hx, hy, hz] = ready.vehicle.halfExtents;
  const box = new THREE.EdgesGeometry(new THREE.BoxGeometry(hx * 2, hy * 2, hz * 2));
  const onCar = new THREE.LineSegments(box, lineMaterial(COLLISION_COLOR));
  onCar.renderOrder = 10;
  car.root.add(onCar);
  return { world, onCar };
}

/** Lazily built debug overlays; nothing is created until an overlay is switched on. */
export class DebugOverlays {
  private readonly scene: THREE.Scene;
  private ready: ReadyMsg | null = null;
  private car: CarRig | null = null;
  private readonly wanted = new Set<OverlayKind>();
  private readonly built = new Map<OverlayKind, THREE.Object3D[]>();

  constructor(scene: THREE.Scene) {
    this.scene = scene;
  }

  /** Called once the track is built; builds any overlay requested before that. */
  attach(ready: ReadyMsg, car: CarRig): void {
    this.ready = ready;
    this.car = car;
    for (const kind of this.wanted) this.show(kind, true);
  }

  /** Switches an overlay on or off. */
  set(kind: OverlayKind, on: boolean): void {
    if (on) this.wanted.add(kind);
    else this.wanted.delete(kind);
    this.show(kind, on);
  }

  /** Whether an overlay is currently requested. */
  isOn(kind: OverlayKind): boolean {
    return this.wanted.has(kind);
  }

  private show(kind: OverlayKind, on: boolean): void {
    const objects = on ? this.ensure(kind) : this.built.get(kind);
    for (const o of objects ?? []) o.visible = on;
  }

  private ensure(kind: OverlayKind): THREE.Object3D[] | undefined {
    const existing = this.built.get(kind);
    if (existing || !this.ready || !this.car) return existing;
    const objects = this.create(kind, this.ready, this.car);
    for (const o of objects) if (!o.parent) this.scene.add(o);
    this.built.set(kind, objects);
    return objects;
  }

  private create(kind: OverlayKind, ready: ReadyMsg, car: CarRig): THREE.Object3D[] {
    if (kind === 'axis') return [buildAxis(ready)];
    if (kind === 'checkpoints') return [buildCheckpoints(ready)];
    const { world, onCar } = buildCollision(ready, car);
    return [world, onCar];
  }
}
