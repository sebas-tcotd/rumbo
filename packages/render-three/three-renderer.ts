// Lab renderer. It only reads state and never writes into the simulation: anything visual that
// would need physics goes into the track in sim-core, not here.

import {
  GHOST_STRIDE,
  type OverlayKind,
  type ReadyMsg,
  type RenderDebugPort,
  type RendererPort,
  type StateIndex,
} from '@rumbo/contracts';
import * as THREE from 'three/webgpu';
import { type CameraMode, ChaseCamera } from './camera/chase-camera.ts';
import { buildCar, type CarRig, placeWheels, type WheelSetup } from './car/car-rig.ts';
import { DebugOverlays } from './overlays/debug-overlays.ts';
import { addBarriers } from './scene/barriers.ts';
import { addLandmarks } from './scene/landmarks.ts';
import { addLighting } from './scene/lighting.ts';
import { addScenery } from './scene/scenery.ts';
import { addTrackMeshes } from './track/track-meshes.ts';

/** Above this jump (m) between two ticks the car was relocated: snap instead of interpolating. */
const SNAP_DISTANCE = 5;

const tmpPos = new THREE.Vector3();
const tmpQ = new THREE.Quaternion();
const tmpQ2 = new THREE.Quaternion();
const SUN_OFFSET = new THREE.Vector3(-60, 140, 45);

/** Rendering backend in use, as shown by the lab telemetry. */
export type RenderBackend = 'iniciando' | 'WebGPU' | 'WebGL 2';

/**
 * three.js (WebGPU with WebGL 2 fallback) implementation of RendererPort and RenderDebugPort. It has no loop of its
 * own: the host calls `frame()` from its single requestAnimationFrame after `init()` resolved.
 * The extra members (camera, ghost visibility, suspension rest, backend) are lab controls used only by
 * apps/web; ui and devtools never import this class.
 */
export class ThreeRenderer implements RendererPort, RenderDebugPort {
  private readonly renderer: THREE.WebGPURenderer;
  private readonly scene = new THREE.Scene();
  private readonly chase = new ChaseCamera();
  private readonly sun: THREE.DirectionalLight;
  private readonly overlays = new DebugOverlays(this.scene);
  private readonly container: HTMLElement;
  private readonly resizeObs: ResizeObserver;
  private status: RenderBackend = 'iniciando';
  private I: StateIndex | null = null;
  private car: CarRig | null = null;
  private ghost: THREE.Group | null = null;
  private ghostFrames: Float32Array | null = null;
  private ghostVisible = true;
  private wheels: WheelSetup = { hardpoints: [], wheelRadius: 0.33, suspensionRest: 0.3 };
  private wheelSpin = 0;
  private tickHz = 120;
  private lastFrame = performance.now();

  constructor(container: HTMLElement) {
    this.container = container;
    this.renderer = new THREE.WebGPURenderer({ antialias: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFShadowMap;
    container.appendChild(this.renderer.domElement);
    this.sun = addLighting(this.scene);
    this.resizeObs = new ResizeObserver(() => this.resize());
    this.resizeObs.observe(container);
  }

  /** Initialises the GPU backend. `frame()` must not be called before this resolves. */
  async init(): Promise<void> {
    await this.renderer.init();
    const webgpu = (this.renderer.backend as { isWebGPUBackend?: boolean }).isWebGPUBackend;
    this.status = webgpu ? 'WebGPU' : 'WebGL 2';
    this.resize();
  }

  /** Backend actually in use, once `init()` resolved. */
  get backend(): RenderBackend {
    return this.status;
  }

  /** Lab camera preset. */
  setCameraMode(mode: CameraMode): void {
    this.chase.mode = mode;
  }

  /** Shows or hides the best-lap ghost without dropping its frames. */
  setGhostVisible(on: boolean): void {
    this.ghostVisible = on;
  }

  /** Suspension rest length of the applied profile (visual wheel drop only). */
  setSuspensionRest(metres: number): void {
    this.wheels.suspensionRest = metres;
  }

  dispose(): void {
    this.resizeObs.disconnect();
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }

  private resize(): void {
    const w = this.container.clientWidth || 1;
    const h = this.container.clientHeight || 1;
    this.renderer.setSize(w, h, false);
    this.chase.resize(w, h);
  }

  build(ready: ReadyMsg, index: StateIndex): void {
    this.I = index;
    this.tickHz = ready.tickHz;
    this.wheels.wheelRadius = ready.vehicle.wheelRadius;
    this.wheels.hardpoints = ready.vehicle.hardpoints;
    addTrackMeshes(this.scene, ready);
    addScenery(this.scene, ready);
    addLandmarks(this.scene, ready);
    addBarriers(this.scene, ready);
    this.car = buildCar(ready.vehicle, false);
    this.scene.add(this.car.root);
    this.ghost = buildCar(ready.vehicle, true).root;
    this.ghost.visible = false;
    this.scene.add(this.ghost);
    this.overlays.attach(ready, this.car);
  }

  /** Debug overlays (AR28); built the first time they are switched on. */
  setOverlay(kind: OverlayKind, on: boolean): void {
    this.overlays.set(kind, on);
  }

  setGhost(frames: Float32Array | null): void {
    this.ghostFrames = frames;
  }

  frame(prev: Float32Array, curr: Float32Array, alpha: number): void {
    const now = performance.now();
    // Camera smoothing uses the real frame time (up to 0.5 s) so it does not lag at low frame rates.
    const dt = Math.min(0.5, (now - this.lastFrame) / 1000);
    this.lastFrame = now;
    const I = this.I;
    if (I && this.car) this.pose(prev, curr, alpha, I, this.car, dt);
    this.renderer.render(this.scene, this.chase.camera);
  }

  private pose(a: Float32Array, b: Float32Array, alpha: number, I: StateIndex, car: CarRig, dt: number): void {
    const pos = tmpPos.set(
      THREE.MathUtils.lerp(a[I.px], b[I.px], alpha),
      THREE.MathUtils.lerp(a[I.py], b[I.py], alpha),
      THREE.MathUtils.lerp(a[I.pz], b[I.pz], alpha),
    );
    tmpQ.set(a[I.qx], a[I.qy], a[I.qz], a[I.qw]);
    tmpQ2.set(b[I.qx], b[I.qy], b[I.qz], b[I.qw]);
    tmpQ.slerp(tmpQ2, alpha);
    // A large jump (reset, teleport) is not interpolated.
    if (Math.hypot(b[I.px] - a[I.px], b[I.pz] - a[I.pz]) > SNAP_DISTANCE) {
      pos.set(b[I.px], b[I.py], b[I.pz]);
      tmpQ.copy(tmpQ2);
      this.chase.snap();
    }
    car.root.position.copy(pos);
    car.root.quaternion.copy(tmpQ);
    this.wheelSpin = placeWheels(car, b, I, this.wheels, this.wheelSpin, dt);
    this.updateGhost(b, I);
    this.chase.trackAcceleration(a, b, I, this.tickHz, dt);
    this.chase.update(pos, tmpQ, b, I, dt);
    this.sun.position.copy(pos).add(SUN_OFFSET);
    this.sun.target.position.copy(pos);
  }

  private updateGhost(s: Float32Array, I: StateIndex): void {
    const g = this.ghostFrames;
    if (!this.ghost) return;
    const frame = Math.floor(s[I.lapTicks]);
    if (!g || !this.ghostVisible || s[I.nextCheckpoint] < 1 || frame * GHOST_STRIDE >= g.length) {
      this.ghost.visible = false;
      return;
    }
    const o = frame * GHOST_STRIDE;
    this.ghost.visible = true;
    this.ghost.position.set(g[o], g[o + 1], g[o + 2]);
    this.ghost.quaternion.set(g[o + 3], g[o + 4], g[o + 5], g[o + 6]);
  }
}

/** Factory used by the composition root. Call `init()` before the first frame. */
export function createThreeRenderer(container: HTMLElement): ThreeRenderer {
  return new ThreeRenderer(container);
}
