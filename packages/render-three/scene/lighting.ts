import * as THREE from 'three/webgpu';
import { SCENE } from '../palette.ts';

/** Overcast sky: lots of diffuse light and soft shadows. Returns the sun that follows the car. */
export function addLighting(scene: THREE.Scene): THREE.DirectionalLight {
  scene.background = new THREE.Color(SCENE.sky);
  scene.fog = new THREE.Fog(SCENE.sky, 140, 900);
  scene.add(new THREE.HemisphereLight(0xe4e7e4, 0x8a7a5e, 1.9));
  const sun = new THREE.DirectionalLight(0xffffff, 1.5);
  sun.position.set(-120, 220, 90);
  sun.castShadow = true;
  sun.shadow.mapSize.set(2048, 2048);
  const sc = sun.shadow.camera;
  sc.left = -60;
  sc.right = 60;
  sc.top = 60;
  sc.bottom = -60;
  sc.near = 10;
  sc.far = 600;
  sun.shadow.bias = -0.0005;
  scene.add(sun, sun.target);
  return sun;
}
