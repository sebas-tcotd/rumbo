// Render del laboratorio. Solo lee estado: nunca escribe en la simulación.
// Si algo visual necesitara física, se agrega a la pista en sim-core, no aquí.

import * as THREE from 'three/webgpu';
import { GHOST_STRIDE, type MeshData, type ReadyMsg, type StateMsg } from '../sim/protocol';
import type { StateIndex } from '../sim/state';
import { SCENE, SURFACE_COLORS } from './palette';

export type CameraMode = 'persecucion' | 'cercana' | 'aerea';

interface CarRig {
  root: THREE.Group;
  body: THREE.Group;
  wheels: THREE.Group[];
  spinners: THREE.Object3D[];
}

const tmpQ = new THREE.Quaternion();
const tmpQ2 = new THREE.Quaternion();
const tmpV = new THREE.Vector3();
const tmpV2 = new THREE.Vector3();
const Y = new THREE.Vector3(0, 1, 0);

/** Generador pseudoaleatorio con semilla (solo decoración; no afecta la simulación). */
function mulberry32(seed: number) {
  return () => {
    seed |= 0;
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export class LabRenderer {
  readonly renderer: THREE.WebGPURenderer;
  readonly scene = new THREE.Scene();
  readonly camera = new THREE.PerspectiveCamera(62, 1, 0.1, 2500);
  backend = 'iniciando';
  cameraMode: CameraMode = 'persecucion';
  showGhost = true;

  private container: HTMLElement;
  private I: StateIndex | null = null;
  private car: CarRig | null = null;
  private ghost: THREE.Group | null = null;
  private ghostFrames: Float32Array | null = null;
  private sun = new THREE.DirectionalLight(0xffffff, 1.5);
  private latest: StateMsg | null = null;
  private suspensionRest = 0.3;
  private wheelRadius = 0.33;
  private hardpoints: [number, number, number][] = [];
  private wheelSpin = 0;
  private lastFrame = performance.now();
  private camPos = new THREE.Vector3();
  private camLook = new THREE.Vector3();
  private camInit = false;
  /** Aceleración longitudinal suavizada (m/s²), para que la cámara reaccione al frenar. */
  private accelSmooth = 0;
  private tickHz = 120;
  private resizeObs: ResizeObserver;
  onFrame: (dt: number) => void = () => {};

  constructor(container: HTMLElement) {
    this.container = container;
    this.renderer = new THREE.WebGPURenderer({ antialias: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFShadowMap;
    container.appendChild(this.renderer.domElement);

    this.scene.background = new THREE.Color(SCENE.sky);
    this.scene.fog = new THREE.Fog(SCENE.sky, 140, 900);

    // Luz de cielo nublado: mucha luz difusa, sombras suaves.
    const hemi = new THREE.HemisphereLight(0xe4e7e4, 0x8a7a5e, 1.9);
    this.scene.add(hemi);
    this.sun.position.set(-120, 220, 90);
    this.sun.castShadow = true;
    this.sun.shadow.mapSize.set(2048, 2048);
    const sc = this.sun.shadow.camera;
    sc.left = -60;
    sc.right = 60;
    sc.top = 60;
    sc.bottom = -60;
    sc.near = 10;
    sc.far = 600;
    this.sun.shadow.bias = -0.0005;
    this.scene.add(this.sun, this.sun.target);

    this.resizeObs = new ResizeObserver(() => this.resize());
    this.resizeObs.observe(container);
  }

  async start() {
    await this.renderer.init();
    const backend = (this.renderer.backend as { isWebGPUBackend?: boolean }).isWebGPUBackend;
    this.backend = backend ? 'WebGPU' : 'WebGL 2';
    this.resize();
    this.renderer.setAnimationLoop(() => this.frame());
  }

  dispose() {
    this.renderer.setAnimationLoop(null);
    this.resizeObs.disconnect();
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }

  private resize() {
    const w = this.container.clientWidth || 1;
    const h = this.container.clientHeight || 1;
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }

  // ── Construcción de la escena ──

  build(ready: ReadyMsg, index: StateIndex) {
    this.I = index;
    this.tickHz = ready.tickHz;
    this.wheelRadius = ready.vehicle.wheelRadius;
    this.hardpoints = ready.vehicle.hardpoints;
    const surface = new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.95, metalness: 0 });
    const road = this.meshFrom(ready.meshes.drivable, surface);
    road.receiveShadow = true;
    const walls = this.meshFrom(ready.meshes.walls, surface);
    walls.receiveShadow = true;
    walls.castShadow = true;
    const decals = this.meshFrom(ready.meshes.decals, new THREE.MeshStandardMaterial({ color: SCENE.dash, roughness: 0.9 }));
    decals.receiveShadow = true;
    this.scene.add(road, walls, decals);

    const ground = new THREE.Mesh(
      new THREE.PlaneGeometry(4000, 4000),
      new THREE.MeshStandardMaterial({ color: SCENE.ground, roughness: 1 }),
    );
    ground.rotation.x = -Math.PI / 2;
    ground.position.y = ready.groundY;
    ground.receiveShadow = true;
    this.scene.add(ground);

    this.addScenery(ready);
    this.addLandmarks(ready);
    this.car = this.buildCar(ready.vehicle.halfExtents, false);
    this.scene.add(this.car.root);
    const g = this.buildCar(ready.vehicle.halfExtents, true);
    this.ghost = g.root;
    this.ghost.visible = false;
    this.scene.add(this.ghost);
  }

  private meshFrom(data: MeshData, material: THREE.Material) {
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
    geo.computeVertexNormals(); // vértices sin compartir: sombreado plano, estilo maqueta
    return new THREE.Mesh(geo, material);
  }

  /** Árboles y rocas fuera de la pista, con semilla fija. */
  private addScenery(ready: ReadyMsg) {
    const cl = ready.centerline;
    const n = cl.length / 10;
    const rand = mulberry32(20261103);
    const pts: THREE.Vector2[] = [];
    for (let i = 0; i < n; i += 4) pts.push(new THREE.Vector2(cl[i * 10], cl[i * 10 + 2]));
    let minX = Infinity, maxX = -Infinity, minZ = Infinity, maxZ = -Infinity;
    for (const p of pts) {
      minX = Math.min(minX, p.x); maxX = Math.max(maxX, p.x);
      minZ = Math.min(minZ, p.y); maxZ = Math.max(maxZ, p.y);
    }
    const pad = 220;
    const clearance = (x: number, z: number) => {
      let best = Infinity;
      for (const p of pts) best = Math.min(best, (p.x - x) ** 2 + (p.y - z) ** 2);
      return Math.sqrt(best);
    };

    const crown = new THREE.IcosahedronGeometry(1, 0);
    const trunk = new THREE.CylinderGeometry(0.18, 0.26, 1, 5);
    const rock = new THREE.DodecahedronGeometry(1, 0);
    const crownMats = [SCENE.tree, SCENE.treeDark].map((c) => new THREE.MeshStandardMaterial({ color: c, roughness: 1, flatShading: true }));
    const trunkMat = new THREE.MeshStandardMaterial({ color: SCENE.trunk, roughness: 1 });
    const rockMat = new THREE.MeshStandardMaterial({ color: SCENE.rock, roughness: 1, flatShading: true });

    const trees: THREE.Matrix4[] = [];
    const trunks: THREE.Matrix4[] = [];
    const rocks: THREE.Matrix4[] = [];
    const m = new THREE.Matrix4();
    for (let k = 0; k < 900; k++) {
      const x = minX - pad + rand() * (maxX - minX + pad * 2);
      const z = minZ - pad + rand() * (maxZ - minZ + pad * 2);
      const d = clearance(x, z);
      if (d < 26) continue;
      const y = ready.groundY;
      if (rand() < 0.72) {
        const s = 1.6 + rand() * 2.4;
        m.compose(new THREE.Vector3(x, y + s * 1.2 + 1.4, z), new THREE.Quaternion().setFromAxisAngle(Y, rand() * 6.28), new THREE.Vector3(s, s * 1.25, s));
        trees.push(m.clone());
        m.compose(new THREE.Vector3(x, y + 1.0, z), new THREE.Quaternion(), new THREE.Vector3(1, 2.0, 1));
        trunks.push(m.clone());
      } else {
        const s = 0.6 + rand() * 1.8;
        m.compose(new THREE.Vector3(x, y + s * 0.4, z), new THREE.Quaternion().setFromEuler(new THREE.Euler(rand(), rand() * 6.28, rand())), new THREE.Vector3(s * 1.3, s * 0.8, s));
        rocks.push(m.clone());
      }
    }
    const inst = (geo: THREE.BufferGeometry, mat: THREE.Material, list: THREE.Matrix4[]) => {
      const mesh = new THREE.InstancedMesh(geo, mat, list.length);
      list.forEach((mm, i) => mesh.setMatrixAt(i, mm));
      mesh.castShadow = true;
      mesh.receiveShadow = true;
      this.scene.add(mesh);
    };
    const half = Math.floor(trees.length / 2);
    inst(crown, crownMats[0], trees.slice(0, half));
    inst(crown, crownMats[1], trees.slice(half));
    inst(trunk, trunkMat, trunks);
    inst(rock, rockMat, rocks);
  }

  /** Pórtico de meta y postes ámbar en los checkpoints. */
  private addLandmarks(ready: ReadyMsg) {
    const cl = ready.centerline;
    const n = cl.length / 10;
    const spacing = ready.trackLength / n;
    const at = (s: number) => {
      const i = Math.round(s / spacing) % n;
      const o = i * 10;
      return {
        pos: new THREE.Vector3(cl[o], cl[o + 1], cl[o + 2]),
        right: new THREE.Vector3(cl[o + 3], cl[o + 4], cl[o + 5]),
        width: cl[o + 9],
      };
    };
    const dark = new THREE.MeshStandardMaterial({ color: SCENE.gantry, roughness: 0.8 });
    const accent = new THREE.MeshStandardMaterial({ color: SCENE.checkpoint, roughness: 0.6 });

    const start = at(0);
    const span = start.width + 6;
    for (const side of [-1, 1]) {
      const post = new THREE.Mesh(new THREE.BoxGeometry(0.5, 7, 0.5), dark);
      post.position.copy(start.pos).addScaledVector(start.right, (span / 2) * side).add(new THREE.Vector3(0, 3.5, 0));
      post.castShadow = true;
      this.scene.add(post);
    }
    const beam = new THREE.Mesh(new THREE.BoxGeometry(span + 0.5, 0.9, 0.6), accent);
    beam.position.copy(start.pos).add(new THREE.Vector3(0, 7, 0));
    beam.quaternion.setFromUnitVectors(new THREE.Vector3(1, 0, 0), new THREE.Vector3(start.right.x, 0, start.right.z).normalize());
    beam.castShadow = true;
    this.scene.add(beam);

    for (const s of ready.checkpoints.slice(1)) {
      const cp = at(s);
      for (const side of [-1, 1]) {
        const pole = new THREE.Mesh(new THREE.BoxGeometry(0.35, 2.4, 0.35), accent);
        pole.position.copy(cp.pos).addScaledVector(cp.right, (cp.width / 2 + 0.6) * side).add(new THREE.Vector3(0, 1.2, 0));
        pole.castShadow = true;
        this.scene.add(pole);
      }
    }
  }

  private buildCar(he: [number, number, number], ghost: boolean): CarRig {
    const mat = (color: number, rough = 0.6) =>
      ghost
        ? new THREE.MeshStandardMaterial({ color: SCENE.ghost, transparent: true, opacity: 0.32, depthWrite: false, roughness: 0.5 })
        : new THREE.MeshStandardMaterial({ color, roughness: rough, metalness: 0.05, flatShading: true });
    const root = new THREE.Group();
    const body = new THREE.Group();
    root.add(body);
    const [hx, hy, hz] = he;
    const lower = new THREE.Mesh(new THREE.BoxGeometry(hx * 2, hy * 1.25, hz * 2), mat(SCENE.car));
    lower.position.y = -hy * 0.2;
    const cabin = new THREE.Mesh(new THREE.BoxGeometry(hx * 1.7, hy * 1.3, hz * 1.05), mat(SCENE.glass, 0.25));
    cabin.position.set(0, hy * 0.95, hz * 0.12);
    const roof = new THREE.Mesh(new THREE.BoxGeometry(hx * 1.62, hy * 0.18, hz * 0.8), mat(SCENE.car));
    roof.position.set(0, hy * 1.66, hz * 0.16);
    const spoiler = new THREE.Mesh(new THREE.BoxGeometry(hx * 1.9, 0.08, 0.4), mat(SCENE.carDark));
    spoiler.position.set(0, hy * 0.9, hz * 0.95);
    body.add(lower, cabin, roof, spoiler);
    if (!ghost) {
      const lamp = new THREE.MeshStandardMaterial({ color: 0xfff6dc, emissive: 0xfff2c8, emissiveIntensity: 0.6 });
      const tail = new THREE.MeshStandardMaterial({ color: 0xb3261e, emissive: 0x7a0f0a, emissiveIntensity: 0.5 });
      for (const side of [-1, 1]) {
        const h = new THREE.Mesh(new THREE.BoxGeometry(0.42, 0.14, 0.05), lamp);
        h.position.set(side * hx * 0.62, 0, -hz - 0.01);
        const t = new THREE.Mesh(new THREE.BoxGeometry(0.42, 0.14, 0.05), tail);
        t.position.set(side * hx * 0.62, 0.05, hz + 0.01);
        body.add(h, t);
      }
    }
    body.traverse((o) => {
      if (o instanceof THREE.Mesh) o.castShadow = !ghost;
    });

    const wheels: THREE.Group[] = [];
    const spinners: THREE.Object3D[] = [];
    const r = this.wheelRadius;
    const tyreGeo = new THREE.CylinderGeometry(r, r, 0.26, 12);
    tyreGeo.rotateZ(Math.PI / 2);
    const rimGeo = new THREE.CylinderGeometry(r * 0.58, r * 0.58, 0.27, 6);
    rimGeo.rotateZ(Math.PI / 2);
    for (let i = 0; i < 4; i++) {
      const steer = new THREE.Group();
      const spin = new THREE.Group();
      spin.add(new THREE.Mesh(tyreGeo, mat(SCENE.tyre, 0.9)), new THREE.Mesh(rimGeo, mat(SCENE.rim, 0.4)));
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

  // ── Datos de entrada ──

  setState(msg: StateMsg) {
    this.latest = msg;
  }

  setSuspensionRest(m: number) {
    this.suspensionRest = m;
  }

  setGhost(frames: Float32Array | null) {
    this.ghostFrames = frames;
  }

  /** Tiempo del fantasma (ticks) al llegar a la distancia `s` de la vuelta, o null. */
  ghostTicksAt(s: number): number | null {
    const g = this.ghostFrames;
    if (!g) return null;
    const n = g.length / GHOST_STRIDE;
    let lo = 0;
    let hi = n - 1;
    // `s` crece a lo largo de la vuelta salvo el salto de la meta en el primer frame.
    const sAt = (i: number) => g[i * GHOST_STRIDE + 7];
    if (n < 2) return null;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (sAt(mid) < s) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }

  // ── Bucle de render ──

  private frame() {
    const now = performance.now();
    // El suavizado de cámara usa el tiempo real del cuadro (hasta 0,5 s) para no
    // quedarse atrás cuando la tasa de cuadros es baja.
    const dt = Math.min(0.5, (now - this.lastFrame) / 1000);
    this.lastFrame = now;
    this.onFrame(dt);
    const I = this.I;
    const msg = this.latest;
    if (I && msg && this.car) {
      const hz = this.tickHz;
      const alpha = THREE.MathUtils.clamp((performance.timeOrigin + now - msg.at) / (1000 / hz), 0, 1);
      const a = msg.prev;
      const b = msg.curr;
      const pos = tmpV.set(
        THREE.MathUtils.lerp(a[I.px], b[I.px], alpha),
        THREE.MathUtils.lerp(a[I.py], b[I.py], alpha),
        THREE.MathUtils.lerp(a[I.pz], b[I.pz], alpha),
      );
      tmpQ.set(a[I.qx], a[I.qy], a[I.qz], a[I.qw]);
      tmpQ2.set(b[I.qx], b[I.qy], b[I.qz], b[I.qw]);
      tmpQ.slerp(tmpQ2, alpha);
      // Un salto grande (recolocar) no se interpola.
      if (Math.hypot(b[I.px] - a[I.px], b[I.pz] - a[I.pz]) > 5) {
        pos.set(b[I.px], b[I.py], b[I.pz]);
        tmpQ.copy(tmpQ2);
        this.camInit = false;
      }
      this.car.root.position.copy(pos);
      this.car.root.quaternion.copy(tmpQ);
      this.placeWheels(b, I, dt);
      this.updateGhost(b, I);
      // Cambio de rapidez entre dos ticks consecutivos: aceleración longitudinal real.
      const accel = ((Math.abs(b[I.speedKmh]) - Math.abs(a[I.speedKmh])) / 3.6) * hz;
      this.accelSmooth += (accel - this.accelSmooth) * (1 - Math.exp(-dt * 6));
      this.updateCamera(pos, tmpQ, b, I, dt);
      this.sun.position.copy(pos).add(new THREE.Vector3(-60, 140, 45));
      this.sun.target.position.copy(pos);
    }
    this.renderer.render(this.scene, this.camera);
  }

  private placeWheels(s: Float32Array, I: StateIndex, dt: number) {
    const car = this.car!;
    const comps = [s[I.compFL], s[I.compFR], s[I.compRL], s[I.compRR]];
    const contacts = [s[I.contactFL], s[I.contactFR], s[I.contactRL], s[I.contactRR]];
    const steer = (-s[I.steerDeg] * Math.PI) / 180;
    const speed = s[I.speedKmh] / 3.6;
    this.wheelSpin -= (speed * dt) / this.wheelRadius;
    for (let i = 0; i < 4; i++) {
      const [x, y, z] = this.hardpoints[i];
      const drop = contacts[i] > 0.5 ? this.suspensionRest - comps[i] : this.suspensionRest;
      car.wheels[i].position.set(x + Math.sign(x) * 0.17, y - drop, z); // asoman de la carrocería (solo visual)
      car.wheels[i].rotation.set(0, i < 2 ? steer : 0, 0);
      car.spinners[i].rotation.x = this.wheelSpin;
    }
  }

  private updateGhost(s: Float32Array, I: StateIndex) {
    const g = this.ghostFrames;
    if (!this.ghost) return;
    const frame = Math.floor(s[I.lapTicks]);
    if (!g || !this.showGhost || s[I.nextCheckpoint] < 1 || frame * GHOST_STRIDE >= g.length) {
      this.ghost.visible = false;
      return;
    }
    const o = frame * GHOST_STRIDE;
    this.ghost.visible = true;
    this.ghost.position.set(g[o], g[o + 1], g[o + 2]);
    this.ghost.quaternion.set(g[o + 3], g[o + 4], g[o + 5], g[o + 6]);
  }

  private updateCamera(pos: THREE.Vector3, rot: THREE.Quaternion, s: Float32Array, I: StateIndex, dt: number) {
    const fwd = tmpV2.set(0, 0, -1).applyQuaternion(rot);
    fwd.y = 0;
    fwd.normalize();
    // En derrape la cámara mira hacia donde va el auto, no hacia donde apunta.
    const vel = new THREE.Vector3(s[I.vx], 0, s[I.vz]);
    const speed = vel.length();
    if (speed > 3) fwd.lerp(vel.normalize(), 0.4).normalize();
    const kmh = Math.abs(s[I.speedKmh]);

    let dist = 6.4 + kmh * 0.008;
    let height = 2.2;
    let look = 4;
    if (this.cameraMode === 'cercana') {
      dist = 4.2 + kmh * 0.004;
      height = 1.45;
      look = 6;
    } else if (this.cameraMode === 'aerea') {
      dist = 22;
      height = 26;
      look = 8;
    }
    if (this.cameraMode !== 'aerea') {
      // Frenada: la cámara se acerca (más cuanto más brusca); aceleración: se aleja un poco.
      const braking = THREE.MathUtils.clamp(-this.accelSmooth / 11, 0, 1);
      const pulling = THREE.MathUtils.clamp(this.accelSmooth / 7, 0, 1);
      dist *= 1 - 0.3 * braking + 0.07 * pulling;
      height *= 1 - 0.12 * braking;
    }
    const desired = pos.clone().addScaledVector(fwd, -dist).add(new THREE.Vector3(0, height, 0));
    const target = pos.clone().addScaledVector(fwd, look).add(new THREE.Vector3(0, 0.8, 0));
    if (!this.camInit) {
      this.camPos.copy(desired);
      this.camLook.copy(target);
      this.camInit = true;
    }
    const k = 1 - Math.exp(-dt * 7);
    this.camPos.lerp(desired, k);
    this.camLook.lerp(target, 1 - Math.exp(-dt * 12));
    this.camera.position.copy(this.camPos);
    // Temblor leve a alta velocidad (solo render).
    if (kmh > 150) {
      const amp = ((kmh - 150) / 100) * 0.03;
      const t = performance.now() / 1000;
      this.camera.position.y += Math.sin(t * 37) * amp;
      this.camera.position.x += Math.sin(t * 29 + 1.3) * amp;
    }
    this.camera.lookAt(this.camLook);
    const fov = 60 + 16 * THREE.MathUtils.clamp(kmh / 220, 0, 1);
    if (Math.abs(this.camera.fov - fov) > 0.05) {
      this.camera.fov += (fov - this.camera.fov) * k;
      this.camera.updateProjectionMatrix();
    }
  }
}
