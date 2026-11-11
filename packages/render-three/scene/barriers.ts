import type { ReadyMsg } from '@rumbo/contracts';
import * as THREE from 'three/webgpu';

const W = 256;
const H = 128;
/** Metres per arrow. */
const TILE = 2.2;
const HEIGHT = 2.0;

/** Black and white stripes with a lit orange ">" arrow pointing towards +u. */
function paint(glowOnly: boolean): THREE.CanvasTexture {
  const c = document.createElement('canvas');
  c.width = W;
  c.height = H;
  const g = c.getContext('2d');
  if (!g) throw new Error('CANVAS_2D_UNAVAILABLE');
  g.fillStyle = glowOnly ? '#000' : '#141414';
  g.fillRect(0, 0, W, H);
  if (!glowOnly) {
    g.fillStyle = '#e8e8e8';
    for (let x = -H; x < W + H; x += 32) {
      g.beginPath();
      g.moveTo(x, H);
      g.lineTo(x + 14, H);
      g.lineTo(x + 14 + H * 0.6, 0);
      g.lineTo(x + H * 0.6, 0);
      g.closePath();
      g.fill();
    }
  }
  g.strokeStyle = '#ff8a1f';
  g.lineWidth = 22;
  g.lineJoin = 'miter';
  g.beginPath();
  g.moveTo(W * 0.32, H * 0.16);
  g.lineTo(W * 0.68, H * 0.5);
  g.lineTo(W * 0.32, H * 0.84);
  g.stroke();
  const t = new THREE.CanvasTexture(c);
  t.wrapS = THREE.RepeatWrapping;
  t.colorSpace = THREE.SRGBColorSpace;
  t.anisotropy = 4;
  return t;
}

interface Strip {
  pos: number[];
  uv: number[];
  idx: number[];
}

/** One quad per polyline segment; u grows along the arrows so ">" points where the race goes. */
function addBarrier(strip: Strip, line: [number, number, number][], arrow: number): void {
  const pts = line.map(([x, y, z]) => new THREE.Vector3(x, y, z));
  const total = pts.slice(1).reduce((acc, p, i) => acc + p.distanceTo(pts[i]), 0);
  let along = 0;
  for (let i = 0; i < pts.length - 1; i++) {
    const [a, c] = [pts[i], pts[i + 1]];
    const seg = a.distanceTo(c);
    const ua = (arrow > 0 ? along : total - along) / TILE;
    const uc = (arrow > 0 ? along + seg : total - along - seg) / TILE;
    const base = strip.pos.length / 3;
    strip.pos.push(a.x, a.y - 0.05, a.z, c.x, c.y - 0.05, c.z, c.x, c.y + HEIGHT, c.z, a.x, a.y + HEIGHT, a.z);
    strip.uv.push(ua, 0, uc, 0, uc, 1, ua, 1);
    strip.idx.push(base, base + 1, base + 2, base, base + 2, base + 3);
    along += seg;
  }
}

/** Race barriers in the style of the classic NFS games, closing side streets. */
export function addBarriers(scene: THREE.Scene, ready: ReadyMsg): void {
  if (!ready.barriers?.length) return;
  const mat = new THREE.MeshStandardMaterial({
    map: paint(false),
    emissiveMap: paint(true),
    emissive: new THREE.Color(0xffffff),
    emissiveIntensity: 1.6,
    roughness: 0.7,
    side: THREE.DoubleSide,
  });
  const strip: Strip = { pos: [], uv: [], idx: [] };
  for (const b of ready.barriers) addBarrier(strip, b.line, b.arrow);
  const geo = new THREE.BufferGeometry();
  geo.setAttribute('position', new THREE.Float32BufferAttribute(strip.pos, 3));
  geo.setAttribute('uv', new THREE.Float32BufferAttribute(strip.uv, 2));
  geo.setIndex(strip.idx);
  geo.computeVertexNormals();
  const mesh = new THREE.Mesh(geo, mat);
  mesh.castShadow = true;
  scene.add(mesh);
}
