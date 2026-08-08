import { useEffect, useRef } from "react";
import {
  BufferAttribute,
  BufferGeometry,
  CanvasTexture,
  Color,
  DoubleSide,
  Mesh,
  MeshBasicMaterial,
  NearestFilter,
  OrthographicCamera,
  Scene,
  SRGBColorSpace,
  WebGLRenderer,
} from "three";
import { CLASSIC_SKYBLOCK, type BlockId } from "./skyblockMap";
import "./SkyIsland.css";

export type SkyInstance = {
  id: string;
  name: string;
  versionId: string;
  loaderLabel: string;
};

type Props = {
  instances: SkyInstance[];
  selectedId: string | null;
  onSelect: (id: string) => void;
};

const ORBIT_PERIOD_S = 48;
const ORBIT_R = 320;
const ORBIT_RY = 48;
const TILT = 0.14;
const YAW_DIRECT = 0.0095;
const YAW_ACCEL = 20;
const YAW_FRICTION = 1.55;
const YAW_MAX = 9;
const YAW_PX_SCALE = 0.005;
const SPIN_SPEED = (Math.PI * 2) / ORBIT_PERIOD_S;
const BEHIND_EPS = 0.04;
const VIEW_H = 9.6;
const ISLAND_SCALE = 0.36;
/** Internal GL resolution cap — fill-rate is the fullscreen killer. */
const MAX_GL_EDGE = 720;
const IDLE_DT = 1 / 60;
const DRAG_DT = 1 / 120;
const COAST_EPS = 0.015;

type FaceKind = "top" | "side" | "bottom";

type FaceDef = {
  dx: number;
  dy: number;
  dz: number;
  kind: (id: BlockId) => FaceKind;
  /** Quad corners in block-local space (CCW, outward). */
  verts: ReadonlyArray<readonly [number, number, number]>;
};

const FACES: readonly FaceDef[] = [
  {
    dx: 1,
    dy: 0,
    dz: 0,
    kind: () => "side",
    verts: [
      [0.5, -0.5, -0.5],
      [0.5, -0.5, 0.5],
      [0.5, 0.5, 0.5],
      [0.5, 0.5, -0.5],
    ],
  },
  {
    dx: -1,
    dy: 0,
    dz: 0,
    kind: () => "side",
    verts: [
      [-0.5, -0.5, 0.5],
      [-0.5, -0.5, -0.5],
      [-0.5, 0.5, -0.5],
      [-0.5, 0.5, 0.5],
    ],
  },
  {
    dx: 0,
    dy: 1,
    dz: 0,
    kind: (id) => (id === "grass" || id === "log" ? "top" : "side"),
    verts: [
      [-0.5, 0.5, -0.5],
      [0.5, 0.5, -0.5],
      [0.5, 0.5, 0.5],
      [-0.5, 0.5, 0.5],
    ],
  },
  {
    dx: 0,
    dy: -1,
    dz: 0,
    kind: (id) => (id === "grass" || id === "log" ? "bottom" : "side"),
    verts: [
      [-0.5, -0.5, 0.5],
      [0.5, -0.5, 0.5],
      [0.5, -0.5, -0.5],
      [-0.5, -0.5, -0.5],
    ],
  },
  {
    dx: 0,
    dy: 0,
    dz: 1,
    kind: () => "side",
    verts: [
      [0.5, -0.5, 0.5],
      [-0.5, -0.5, 0.5],
      [-0.5, 0.5, 0.5],
      [0.5, 0.5, 0.5],
    ],
  },
  {
    dx: 0,
    dy: 0,
    dz: -1,
    kind: () => "side",
    verts: [
      [-0.5, -0.5, -0.5],
      [0.5, -0.5, -0.5],
      [0.5, 0.5, -0.5],
      [-0.5, 0.5, -0.5],
    ],
  },
];

function paintFace(g: CanvasRenderingContext2D, id: BlockId, face: FaceKind) {
  const fill = (color: string) => {
    g.fillStyle = color;
    g.fillRect(0, 0, 16, 16);
  };
  const dot = (x: number, y: number, color: string) => {
    g.fillStyle = color;
    g.fillRect(x, y, 1, 1);
  };
  const speck = (base: string, alts: string[], dens: number, seed: number) => {
    fill(base);
    for (let y = 0; y < 16; y++) {
      for (let x = 0; x < 16; x++) {
        if (((x * 17 + y * 31 + seed) & 127) < dens * 127) {
          dot(x, y, alts[(x + y * 3) % alts.length]!);
        }
      }
    }
  };

  switch (id) {
    case "grass":
      if (face === "top") speck("#62a830", ["#73b83c", "#528f28", "#6aaf34"], 0.5, 11);
      else if (face === "bottom") speck("#6b4a2e", ["#5c3d22", "#7a5634"], 0.4, 12);
      else {
        speck("#6b4a2e", ["#5c3d22", "#7a5634"], 0.22, 13);
        g.fillStyle = "#62a830";
        g.fillRect(0, 0, 16, 5);
        for (let x = 0; x < 16; x++) if ((x * 5) % 3 === 0) dot(x, 5, "#528f28");
      }
      break;
    case "dirt":
      speck("#6b4a2e", ["#5c3d22", "#7a5634", "#8a6240"], 0.45, 14);
      break;
    case "log":
      if (face === "top" || face === "bottom") {
        fill("#6b4a28");
        g.fillStyle = "#c4a06a";
        g.beginPath();
        g.arc(8, 8, 5, 0, Math.PI * 2);
        g.fill();
        g.fillStyle = "#a08050";
        g.beginPath();
        g.arc(8, 8, 2, 0, Math.PI * 2);
        g.fill();
      } else {
        for (let x = 0; x < 16; x++) {
          g.fillStyle = x % 4 < 2 ? "#6b4a28" : "#5a3c20";
          g.fillRect(x, 0, 1, 16);
        }
      }
      break;
    case "leaves":
      speck("#3f922c", ["#4aa032", "#2e6a1c", "#58b03a"], 0.6, 15);
      break;
    case "chest":
      fill("#8b5a2b");
      g.fillStyle = "#6e4520";
      g.fillRect(0, 7, 16, 2);
      g.fillStyle = "#c9a227";
      g.fillRect(7, 8, 2, 3);
      g.fillStyle = "#a06a34";
      g.fillRect(1, 1, 14, 5);
      break;
    case "planks":
      fill("#c4a06a");
      for (let y = 0; y < 16; y += 4) {
        g.fillStyle = "#9a7848";
        g.fillRect(0, y + 3, 16, 1);
        g.fillStyle = "#b8945a";
        g.fillRect(0, y, 16, 3);
      }
      g.fillStyle = "#8a6840";
      g.fillRect(7, 0, 1, 16);
      break;
    case "sand":
      speck("#e8d49a", ["#dcc484", "#f0e0b0", "#cbb872"], 0.4, 16);
      break;
    case "cobble":
      speck("#7a7a7a", ["#6a6a6a", "#8a8a8a", "#5a5a5a"], 0.65, 17);
      break;
    case "water":
      speck("#2f6fbf", ["#3a7fd0", "#245a9a"], 0.45, 18);
      break;
    case "lava":
      speck("#d45a12", ["#ff8a20", "#a83808"], 0.55, 19);
      break;
    case "bedrock":
      speck("#333", ["#222", "#444"], 0.65, 20);
      break;
  }
}

type AtlasUv = { u0: number; v0: number; u1: number; v1: number };

function buildAtlas(): {
  texture: CanvasTexture;
  uv: (id: BlockId, face: FaceKind) => AtlasUv;
  dispose: () => void;
} {
  type Slot = { id: BlockId; face: FaceKind };
  const slots: Slot[] = [
    { id: "grass", face: "top" },
    { id: "grass", face: "side" },
    { id: "grass", face: "bottom" },
    { id: "dirt", face: "side" },
    { id: "log", face: "top" },
    { id: "log", face: "side" },
    { id: "leaves", face: "side" },
    { id: "chest", face: "side" },
    { id: "planks", face: "side" },
    { id: "sand", face: "side" },
    { id: "cobble", face: "side" },
    { id: "water", face: "side" },
    { id: "lava", face: "side" },
    { id: "bedrock", face: "side" },
  ];

  const cols = 4;
  const rows = 4;
  const tile = 16;
  const canvas = document.createElement("canvas");
  canvas.width = cols * tile;
  canvas.height = rows * tile;
  const g = canvas.getContext("2d", { willReadFrequently: false })!;
  const map = new Map<string, AtlasUv>();
  const pad = 0.5 / (cols * tile);

  for (let i = 0; i < slots.length; i++) {
    const s = slots[i]!;
    const col = i % cols;
    const row = (i / cols) | 0;
    g.save();
    g.translate(col * tile, row * tile);
    paintFace(g, s.id, s.face);
    g.restore();
    // CanvasTexture flipY=true → v grows upward in GL; atlas row 0 is top of canvas.
    const u0 = col / cols + pad;
    const u1 = (col + 1) / cols - pad;
    const v1 = 1 - row / rows - pad;
    const v0 = 1 - (row + 1) / rows + pad;
    map.set(`${s.id}:${s.face}`, { u0, v0, u1, v1 });
  }

  const texture = new CanvasTexture(canvas);
  texture.magFilter = NearestFilter;
  texture.minFilter = NearestFilter;
  texture.generateMipmaps = false;
  texture.colorSpace = SRGBColorSpace;
  texture.flipY = true;

  return {
    texture,
    uv: (id, face) => map.get(`${id}:${face}`) ?? map.get(`${id}:side`)!,
    dispose: () => texture.dispose(),
  };
}

function packKey(x: number, y: number, z: number): number {
  return ((x + 32) << 16) | ((y + 32) << 8) | (z + 32);
}

/** One mesh, one material, only visible faces — single draw call. */
function buildIsland(): { root: Mesh; dispose: () => void } {
  const atlas = buildAtlas();
  const occ = new Set<number>();
  for (const b of CLASSIC_SKYBLOCK) occ.add(packKey(b.x, b.y, b.z));

  const pos: number[] = [];
  const uv: number[] = [];
  const idx: number[] = [];
  let vert = 0;
  let minX = Infinity;
  let minY = Infinity;
  let minZ = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  let maxZ = -Infinity;

  for (const b of CLASSIC_SKYBLOCK) {
    for (const face of FACES) {
      if (occ.has(packKey(b.x + face.dx, b.y + face.dy, b.z + face.dz))) continue;
      const uvs = atlas.uv(b.id, face.kind(b.id));
      const uvCorner: Array<readonly [number, number]> = [
        [uvs.u0, uvs.v0],
        [uvs.u1, uvs.v0],
        [uvs.u1, uvs.v1],
        [uvs.u0, uvs.v1],
      ];
      for (let i = 0; i < 4; i++) {
        const [lx, ly, lz] = face.verts[i]!;
        const x = b.x + lx;
        const y = b.y + ly;
        const z = b.z + lz;
        pos.push(x, y, z);
        const [uu, vv] = uvCorner[i]!;
        uv.push(uu, vv);
        if (x < minX) minX = x;
        if (y < minY) minY = y;
        if (z < minZ) minZ = z;
        if (x > maxX) maxX = x;
        if (y > maxY) maxY = y;
        if (z > maxZ) maxZ = z;
      }
      idx.push(vert, vert + 1, vert + 2, vert, vert + 2, vert + 3);
      vert += 4;
    }
  }

  const cx = (minX + maxX) * 0.5;
  const cy = (minY + maxY) * 0.5;
  const cz = (minZ + maxZ) * 0.5;
  for (let i = 0; i < pos.length; i += 3) {
    pos[i]! -= cx;
    pos[i + 1]! -= cy;
    pos[i + 2]! -= cz;
    pos[i]! *= ISLAND_SCALE;
    pos[i + 1]! *= ISLAND_SCALE;
    pos[i + 2]! *= ISLAND_SCALE;
  }

  const geo = new BufferGeometry();
  geo.setAttribute("position", new BufferAttribute(new Float32Array(pos), 3));
  geo.setAttribute("uv", new BufferAttribute(new Float32Array(uv), 2));
  geo.setIndex(idx);
  geo.computeBoundingSphere();

  const mat = new MeshBasicMaterial({
    map: atlas.texture,
    toneMapped: false,
    fog: false,
    side: DoubleSide,
  });

  const mesh = new Mesh(geo, mat);
  mesh.frustumCulled = false;
  mesh.rotation.x = TILT;

  return {
    root: mesh,
    dispose: () => {
      geo.dispose();
      mat.dispose();
      atlas.dispose();
    },
  };
}

function PodFace({ inst }: { inst: SkyInstance }) {
  return (
    <>
      <span className="pod-cube" aria-hidden />
      <span className="pod-label">
        <strong>{inst.name}</strong>
        <span>
          {inst.versionId} · {inst.loaderLabel}
        </span>
      </span>
    </>
  );
}

export default function SkyIsland({ instances, selectedId, onSelect }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const backRef = useRef<HTMLDivElement>(null);
  const frontRef = useRef<HTMLDivElement>(null);
  const yawRef = useRef(0.55);
  const yawVelRef = useRef(0);
  const spinRef = useRef(0);
  const dragRef = useRef<{ lastX: number; lastT: number } | null>(null);
  const dirtyRef = useRef(true);
  const pairsRef = useRef<Array<{ back: HTMLElement; front: HTMLElement }>>([]);
  const behindRef = useRef<Uint8Array>(new Uint8Array(0));
  const sizeRef = useRef({ w: 1, h: 1 });

  useEffect(() => {
    const backLayer = backRef.current;
    const frontLayer = frontRef.current;
    const pairs: Array<{ back: HTMLElement; front: HTMLElement }> = [];
    if (backLayer && frontLayer) {
      for (let i = 0; i < instances.length; i++) {
        const back = backLayer.querySelector<HTMLElement>(`[data-index="${i}"]`);
        const front = frontLayer.querySelector<HTMLElement>(`[data-index="${i}"]`);
        if (back && front) pairs.push({ back, front });
      }
    }
    pairsRef.current = pairs;
    behindRef.current = new Uint8Array(pairs.length);
  }, [instances, selectedId]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const host = hostRef.current;
    if (!canvas || !host) return;

    const renderer = new WebGLRenderer({
      canvas,
      antialias: false,
      alpha: true,
      powerPreference: "high-performance",
      stencil: false,
      depth: true,
      preserveDrawingBuffer: false,
    });
    renderer.setPixelRatio(1);
    renderer.outputColorSpace = SRGBColorSpace;
    renderer.setClearColor(new Color(0x000000), 0);
    renderer.sortObjects = false;

    const scene = new Scene();

    const camera = new OrthographicCamera(-1, 1, 1, -1, 0.1, 80);
    camera.position.set(0, 0.55, 12);
    camera.lookAt(0, 0.35, 0);

    const { root: island, dispose: disposeIsland } = buildIsland();
    island.rotation.y = yawRef.current;
    scene.add(island);

    const resize = () => {
      const w = host.clientWidth | 0;
      const h = host.clientHeight | 0;
      if (w < 2 || h < 2) return;
      sizeRef.current = { w, h };
      const glScale = Math.min(1, MAX_GL_EDGE / Math.max(w, h));
      renderer.setSize(Math.max(2, (w * glScale) | 0), Math.max(2, (h * glScale) | 0), false);
      const viewW = VIEW_H * (w / h);
      camera.left = -viewW * 0.5;
      camera.right = viewW * 0.5;
      camera.top = VIEW_H * 0.5;
      camera.bottom = -VIEW_H * 0.5;
      camera.updateProjectionMatrix();
      dirtyRef.current = true;
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(host);

    let alive = true;
    let last = performance.now();
    let tickAcc = 0;
    let podFrame = 0;

    const syncPods = (spin: number, yaw: number) => {
      const pairs = pairsRef.current;
      const n = pairs.length;
      if (!n) return;

      let bits = behindRef.current;
      if (bits.length !== n) {
        bits = new Uint8Array(n);
        behindRef.current = bits;
      }

      const { w, h } = sizeRef.current;
      const cx = w * 0.5;
      const cy = h * 0.46;
      const step = (Math.PI * 2) / n;

      for (let i = 0; i < n; i++) {
        const { back, front } = pairs[i]!;
        const a = i * step + spin + yaw;
        const depth = Math.sin(a);
        const behind = depth > BEHIND_EPS ? 1 : 0;
        const x = (cx + Math.cos(a) * ORBIT_R) | 0;
        const y = (cy + depth * ORBIT_RY) | 0;
        const scale = 0.84 + (1 - depth) * 0.18;
        const tf = `translate3d(${x}px,${y}px,0) translate(-50%,-50%) scale(${scale.toFixed(3)})`;

        if (bits[i] !== behind) {
          bits[i] = behind;
          back.hidden = !behind;
          front.hidden = !!behind;
          back.style.transform = tf;
          front.style.transform = tf;
        } else if (behind) {
          back.style.transform = tf;
        } else {
          front.style.transform = tf;
        }
      }
    };

    const tick = (t: number) => {
      if (!alive) return;
      const rawDt = Math.min(0.05, (t - last) * 0.001);
      last = t;
      tickAcc += rawDt;

      const dragging = dragRef.current != null;
      const velNow = yawVelRef.current;
      const coasting = velNow > COAST_EPS || velNow < -COAST_EPS;
      const budget = dragging || coasting ? DRAG_DT : IDLE_DT;
      if (tickAcc < budget) return;

      const dt = tickAcc > budget * 2 ? budget * 2 : tickAcc;
      tickAcc = 0;

      spinRef.current += dt * SPIN_SPEED;
      const spin = spinRef.current;
      let vel = yawVelRef.current;
      let yaw = yawRef.current;

      if (!dragging) {
        yaw += vel * dt;
        vel *= Math.exp(-dt * YAW_FRICTION);
        if (vel < COAST_EPS && vel > -COAST_EPS) vel = 0;
        else if (vel > YAW_MAX) vel = YAW_MAX;
        else if (vel < -YAW_MAX) vel = -YAW_MAX;
        yawVelRef.current = vel;
        yawRef.current = yaw;
      } else {
        yaw = yawRef.current;
        vel = yawVelRef.current;
      }

      podFrame++;
      if (!dragging || (podFrame & 1) === 0) syncPods(spin, yaw);

      const moving = dragging || dirtyRef.current || vel > COAST_EPS || vel < -COAST_EPS;
      if (moving) {
        island.rotation.y = yaw;
        renderer.render(scene, camera);
        if (!dragging && !(vel > COAST_EPS || vel < -COAST_EPS)) dirtyRef.current = false;
      }
    };

    const onVis = () => {
      if (document.hidden) renderer.setAnimationLoop(null);
      else {
        last = performance.now();
        tickAcc = 0;
        dirtyRef.current = true;
        renderer.setAnimationLoop(tick);
      }
    };
    document.addEventListener("visibilitychange", onVis);
    renderer.setAnimationLoop(tick);

    return () => {
      alive = false;
      document.removeEventListener("visibilitychange", onVis);
      renderer.setAnimationLoop(null);
      ro.disconnect();
      disposeIsland();
      renderer.dispose();
    };
  }, []);

  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      const drag = dragRef.current;
      if (!drag) return;
      const now = performance.now();
      const dt = Math.max(0.001, (now - drag.lastT) * 0.001);
      const dx = e.clientX - drag.lastX;
      drag.lastX = e.clientX;
      drag.lastT = now;

      yawRef.current += dx * YAW_DIRECT;

      const desired = (dx / dt) * YAW_PX_SCALE;
      const blend = 1 - Math.exp(-dt * YAW_ACCEL);
      let vel = yawVelRef.current;
      vel += (desired - vel) * blend;
      if (vel > YAW_MAX) vel = YAW_MAX;
      else if (vel < -YAW_MAX) vel = -YAW_MAX;
      yawVelRef.current = vel;
      dirtyRef.current = true;
    };
    const onUp = () => {
      dragRef.current = null;
      hostRef.current?.classList.remove("dragging");
      dirtyRef.current = true;
    };
    window.addEventListener("pointermove", onMove, { passive: true });
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };
  }, []);

  const n = Math.max(instances.length, 1);
  const renderPod = (inst: SkyInstance, i: number, side: "back" | "front") => (
    <button
      key={`${side}-${inst.id}`}
      type="button"
      data-pod
      data-index={i}
      tabIndex={side === "back" ? -1 : 0}
      aria-hidden={side === "back" ? true : undefined}
      className={`orbit-pod ${selectedId === inst.id ? "selected" : ""}`}
      style={{ ["--i" as string]: i, ["--n" as string]: n }}
      hidden={side === "back"}
      onClick={() => onSelect(inst.id)}
    >
      <PodFace inst={inst} />
    </button>
  );

  return (
    <div
      ref={hostRef}
      className="sky-scene"
      onPointerDown={(e) => {
        if ((e.target as HTMLElement).closest(".orbit-pod")) return;
        if (e.button !== 0) return;
        dragRef.current = { lastX: e.clientX, lastT: performance.now() };
        dirtyRef.current = true;
        hostRef.current?.classList.add("dragging");
        try {
          (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        } catch {
          /* ignore */
        }
      }}
    >
      <div ref={backRef} className="orbit-layer orbit-back">
        {instances.map((inst, i) => renderPod(inst, i, "back"))}
      </div>

      <canvas ref={canvasRef} className="sky-canvas" aria-hidden />
      <div className="sky-haze" aria-hidden />

      <div ref={frontRef} className="orbit-layer orbit-front">
        {instances.length === 0 ? (
          <div className="orbit-pod hint">
            <p>Drag to turn · New instance to land</p>
          </div>
        ) : (
          instances.map((inst, i) => renderPod(inst, i, "front"))
        )}
      </div>
    </div>
  );
}
