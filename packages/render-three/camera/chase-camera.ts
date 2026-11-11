import type { StateIndex } from '@rumbo/contracts';
import * as THREE from 'three/webgpu';

/** Lab camera presets: chase, close chase and aerial. */
export type CameraMode = 'persecucion' | 'cercana' | 'aerea';

const tmpFwd = new THREE.Vector3();
const tmpVel = new THREE.Vector3();
const tmpDesired = new THREE.Vector3();
const tmpTarget = new THREE.Vector3();
const tmpDisplacement = new THREE.Vector3();

/**
 * Spring-follow camera with a fixed horizontal field of view (MW-style eView): the horizontal
 * angle stays put when the window is resized. Render-only: it never feeds back into the sim.
 */
export class ChaseCamera {
  readonly camera = new THREE.PerspectiveCamera(62, 1, 0.1, 2500);
  mode: CameraMode = 'persecucion';
  /** Vertical FOV of Rumbo at 16:9, in degrees. */
  private referenceFov = 62;
  private camPos = new THREE.Vector3();
  private camLook = new THREE.Vector3();
  private camAnchor = new THREE.Vector3();
  private camInit = false;
  private camRoll = 0;
  /** Smoothed longitudinal acceleration (m/s²), so the camera reacts to braking. */
  private accelSmooth = 0;

  /** Updates the aspect ratio and projection after a viewport resize. */
  resize(width: number, height: number): void {
    this.camera.aspect = width / height;
    this.updateProjection();
  }

  /** Snaps the camera to its target on the next update (after a large jump of the car). */
  snap(): void {
    this.camInit = false;
  }

  /** Feeds the longitudinal acceleration between two consecutive ticks. */
  trackAcceleration(prev: Float32Array, curr: Float32Array, I: StateIndex, hz: number, dt: number): void {
    const accel = ((Math.abs(curr[I.speedKmh]) - Math.abs(prev[I.speedKmh])) / 3.6) * hz;
    this.accelSmooth += (accel - this.accelSmooth) * (1 - Math.exp(-dt * 6));
  }

  private updateProjection(): void {
    const camera = this.camera;
    const chase = this.mode !== 'aerea';
    const baseDeg = (this.referenceFov * (16 / 9)) / 1.1165;
    // Clamping the vertical angle avoids an inverted projection in very narrow windows.
    camera.fov = chase ? Math.min(170, (baseDeg * 1.1165) / camera.aspect) : this.referenceFov;
    camera.updateProjectionMatrix();
    if (!chase) return;
    const halfHorizontal = THREE.MathUtils.degToRad(baseDeg * 1.015) / 2;
    camera.projectionMatrix.elements[0] = 1 / Math.tan(halfHorizontal);
    camera.projectionMatrixInverse.copy(camera.projectionMatrix).invert();
  }

  /** Distance, height and look-ahead for the current mode and speed. */
  private framing(speedFactor: number) {
    if (this.mode === 'aerea') return { dist: 22, height: 26, look: 8 };
    const close = this.mode === 'cercana';
    let dist = close ? 4.2 + speedFactor * 1.25 : 6.4 + speedFactor * 2.2;
    let height = close ? 1.45 : 2.2;
    const look = (close ? 6 : 4) + speedFactor * 4;
    // Braking pulls the camera in (harder braking, closer); accelerating pushes it out a little.
    const braking = THREE.MathUtils.clamp(-this.accelSmooth / 11, 0, 1);
    const pulling = THREE.MathUtils.clamp(this.accelSmooth / 7, 0, 1);
    dist *= 1 - 0.15 * braking + 0.025 * pulling;
    height *= 1 - 0.12 * braking;
    return { dist, height, look };
  }

  /**
   * Moves the camera towards the car. Legacy tuning code from the PoC: kept as one method so the
   * feel stays identical (exceeds the 20-line guideline on purpose).
   */
  update(pos: THREE.Vector3, rot: THREE.Quaternion, s: Float32Array, I: StateIndex, frameDt: number): void {
    const dt = THREE.MathUtils.clamp(frameDt, 0, 0.1);
    const fwd = tmpFwd.set(0, 0, -1).applyQuaternion(rot);
    fwd.y = 0;
    fwd.normalize();
    // While drifting the camera looks where the car is going, not where it points.
    const vel = tmpVel.set(s[I.vx], 0, s[I.vz]);
    if (vel.length() > 3) fwd.lerp(vel.normalize(), 0.4).normalize();
    const kmh = Math.abs(s[I.speedKmh]);
    const speedFactor = THREE.MathUtils.clamp(kmh / 250, 0, 1);
    const { dist, height, look } = this.framing(speedFactor);
    const desired = tmpDesired.copy(pos).addScaledVector(fwd, -dist);
    desired.y += height;
    const target = tmpTarget.copy(pos).addScaledVector(fwd, look);
    target.y += 0.8;
    if (this.mode !== 'aerea') target.y += s[I.throttle] * 0.3 - s[I.brake] * 0.5;
    if (!this.camInit) {
      this.camPos.copy(desired);
      this.camLook.copy(target);
      this.camInit = true;
      this.camRoll = 0;
    } else {
      // Carrying the framing along with the car avoids a speed / 4 m lag: the spring smooths the
      // offset, not the race displacement.
      const displacement = tmpDisplacement.copy(pos).sub(this.camAnchor);
      this.camPos.add(displacement);
      this.camLook.add(displacement);
    }
    this.camAnchor.copy(pos);
    const k = 1 - Math.exp(-dt * 4);
    this.camPos.lerp(desired, k);
    this.camLook.lerp(target, 1 - Math.exp(-dt * 12));
    this.place(kmh, s[I.latG], dt);
    const fov = this.mode === 'aerea' ? 60 : 60 + 6 * speedFactor;
    this.referenceFov += (fov - this.referenceFov) * k;
    this.updateProjection();
  }

  /** Applies position, a light high-speed shake (render only) and the lateral roll. */
  private place(kmh: number, latG: number, dt: number): void {
    const camera = this.camera;
    camera.position.copy(this.camPos);
    if (kmh > 150) {
      const amp = ((kmh - 150) / 100) * 0.03;
      const t = performance.now() / 1000;
      camera.position.y += Math.sin(t * 37) * amp;
      camera.position.x += Math.sin(t * 29 + 1.3) * amp;
    }
    camera.lookAt(this.camLook);
    const targetRoll = this.mode === 'aerea' ? 0 : THREE.MathUtils.clamp(-latG * 0.025, -0.06, 0.06);
    this.camRoll += (targetRoll - this.camRoll) * (1 - Math.exp(-dt * 5));
    camera.rotateZ(this.camRoll);
  }
}
