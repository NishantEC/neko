// A 3D model: .glb, .gltf, .stl and .obj artifacts, and a reply's ```3d block
// naming one. three.js 0.170.0 (MIT, web/package.json; licence in
// node_modules/three/LICENSE and web/src/ui/NOTICE.md), in a chunk of its own
// loaded the first time a model opens. Orbit (drag), zoom (scroll), pan
// (right-drag), fit to view, wireframe and grid; it renders only when
// something changes, never in an idle loop.
//
// Ported from the owner's 3D viewer plugin (`<data dir>/plugins/3d-viewer`,
// written by Neko, 2026-11): theme colours read through a canvas, the glTF
// packing that keeps every byte inside Neko's page policy (no blob: or data:
// fetches, which `connect-src 'self'` refuses: buffers become one .glb chunk
// and images data: URLs that an <img> may load).
import { useCallback, useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { STLLoader } from "three/examples/jsm/loaders/STLLoader.js";
import { OBJLoader } from "three/examples/jsm/loaders/OBJLoader.js";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { Button } from "@/ui/button";
import { Switch } from "@/ui/switch";
import { CornersOut } from "@/ui/icons";
import { readBytes, type ViewerProps } from "@/canvas/viewers";
import { ViewerProblem } from "@/canvas/builtin-viewers";

class UserError extends Error {}

/** Any CSS colour the theme uses (hex, hsl, oklch…) as a THREE.Color, resolved by a canvas over the card colour. */
function cssColor(name: string, fallback: string): THREE.Color {
  const read = (n: string) => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
  const value = read(name);
  if (!value) return new THREE.Color(fallback);
  const ctx = document.createElement("canvas").getContext("2d");
  if (!ctx) return new THREE.Color(fallback);
  ctx.fillStyle = "#808080";
  ctx.fillStyle = read("--card") || "#808080";
  ctx.fillRect(0, 0, 1, 1);
  ctx.fillStyle = fallback;
  ctx.fillStyle = value;
  ctx.fillRect(0, 0, 1, 1);
  const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
  return new THREE.Color().setRGB(r / 255, g / 255, b / 255, THREE.SRGBColorSpace);
}

function countTriangles(object: THREE.Object3D): number {
  let n = 0;
  object.traverse((o) => {
    const mesh = o as THREE.Mesh;
    if (!mesh.isMesh || !mesh.geometry) return;
    const g = mesh.geometry;
    n += (g.index ? g.index.count : g.attributes.position?.count ?? 0) / 3;
  });
  return Math.round(n);
}

function materials(o: THREE.Object3D): THREE.Material[] {
  const m = (o as THREE.Mesh).material;
  return Array.isArray(m) ? m : m ? [m] : [];
}

function disposeObject(object: THREE.Object3D) {
  object.traverse((o) => {
    (o as THREE.Mesh).geometry?.dispose();
    for (const m of materials(o)) {
      for (const v of Object.values(m)) if (v && (v as THREE.Texture).isTexture) (v as THREE.Texture).dispose();
      m.dispose();
    }
  });
}

function applyWireframe(object: THREE.Object3D, on: boolean) {
  object.traverse((o) => {
    if (!(o as THREE.Mesh).isMesh) return;
    for (const m of materials(o)) if ("wireframe" in m) (m as THREE.MeshStandardMaterial).wireframe = on;
  });
}

function decodeDataUri(uri: string): Uint8Array {
  const comma = uri.indexOf(",");
  const meta = uri.slice(5, comma);
  const body = uri.slice(comma + 1);
  if (meta.endsWith(";base64")) {
    const s = atob(body);
    const out = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
    return out;
  }
  return new TextEncoder().encode(decodeURIComponent(body));
}

function toDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

/* eslint-disable @typescript-eslint/no-explicit-any */
/**
 * One self-contained .glb from a .glb or a .gltf: all buffers become its
 * single binary chunk and every image a data: URL. A .gltf that points at
 * files beside it cannot be read (an artifact is one file): say so.
 */
async function packGlb(bytes: Uint8Array, kind: string): Promise<ArrayBuffer> {
  let json: any;
  let glbBin: Uint8Array | null = null;
  if (kind === "glb") {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    if (bytes.length < 20 || view.getUint32(0, true) !== 0x46546c67) throw new UserError("That .glb file isn't a valid glTF binary.");
    let offset = 12;
    while (offset + 8 <= bytes.length) {
      const length = view.getUint32(offset, true);
      const type = view.getUint32(offset + 4, true);
      const chunk = bytes.subarray(offset + 8, offset + 8 + length);
      if (type === 0x4e4f534a) json = JSON.parse(new TextDecoder().decode(chunk));
      else if (type === 0x004e4942) glbBin = chunk;
      offset += 8 + length;
    }
    if (!json) throw new UserError("That .glb file isn't a valid glTF binary.");
  } else {
    try { json = JSON.parse(new TextDecoder().decode(bytes)); } catch { throw new UserError("That .gltf file isn't valid glTF JSON."); }
  }
  const external = (uri: unknown) => typeof uri === "string" && !uri.startsWith("data:");
  if ((json.buffers ?? []).some((b: any) => external(b.uri)) || (json.images ?? []).some((i: any) => external(i.uri))) {
    throw new UserError("This .gltf refers to files beside it, which an artifact cannot hold. Save it as one .glb (or embed its buffers and images as data: URIs).");
  }
  const sources: Uint8Array[] = (json.buffers ?? []).map((b: any, i: number) =>
    !b.uri ? (i === 0 && glbBin ? glbBin : new Uint8Array(b.byteLength || 0)) : decodeDataUri(b.uri));
  const offsets: number[] = [];
  let total = 0;
  for (const s of sources) { offsets.push(total); total += Math.ceil(s.length / 4) * 4; }
  const bin = new Uint8Array(total);
  sources.forEach((s, i) => bin.set(s, offsets[i]));
  for (const bv of json.bufferViews ?? []) { bv.byteOffset = (bv.byteOffset || 0) + offsets[bv.buffer]; bv.buffer = 0; }
  json.buffers = total || json.bufferViews?.length ? [{ byteLength: total }] : [];
  for (const img of json.images ?? []) {
    if (img.bufferView != null) {
      const bv = json.bufferViews[img.bufferView];
      img.uri = await toDataUrl(new Blob([bin.subarray(bv.byteOffset, bv.byteOffset + bv.byteLength)], { type: img.mimeType || "image/png" }));
      delete img.bufferView;
    }
  }
  const enc = new TextEncoder().encode(JSON.stringify(json));
  const jsonLen = Math.ceil(enc.length / 4) * 4;
  const out = new Uint8Array(12 + 8 + jsonLen + (total ? 8 + total : 0));
  const dv = new DataView(out.buffer);
  dv.setUint32(0, 0x46546c67, true);
  dv.setUint32(4, 2, true);
  dv.setUint32(8, out.length, true);
  dv.setUint32(12, jsonLen, true);
  dv.setUint32(16, 0x4e4f534a, true);
  out.fill(0x20, 20, 20 + jsonLen);
  out.set(enc, 20);
  if (total) {
    dv.setUint32(20 + jsonLen, total, true);
    dv.setUint32(24 + jsonLen, 0x004e4942, true);
    out.set(bin, 28 + jsonLen);
  }
  return out.buffer;
}

async function loadModel(buffer: ArrayBuffer, kind: string, meshColor: THREE.Color): Promise<THREE.Object3D> {
  if (kind === "stl") {
    const geometry = new STLLoader().parse(buffer);
    // Many exporters write zero facet normals; recompute (flat, as STL is unindexed).
    geometry.computeVertexNormals();
    const colored = !!(geometry as any).hasColors;
    return new THREE.Mesh(geometry, new THREE.MeshStandardMaterial({ color: colored ? 0xffffff : meshColor, vertexColors: colored, metalness: 0.1, roughness: 0.6 }));
  }
  if (kind === "obj") {
    const object = new OBJLoader().parse(new TextDecoder().decode(buffer));
    object.traverse((o) => {
      for (const m of materials(o)) {
        const std = m as THREE.MeshPhongMaterial;
        if (!std.map && std.color?.getHex() === 0xffffff) std.color.copy(meshColor);
      }
    });
    return object;
  }
  const glb = await packGlb(new Uint8Array(buffer), kind);
  const loader = new GLTFLoader();
  // three's ImageBitmapLoader fetches, which the page policy refuses for data:
  // URLs; an <img> may load them, so textures go through TextureLoader.
  loader.register((parser: any) => {
    parser.textureLoader = new THREE.TextureLoader(parser.options.manager);
    return { name: "neko_csp_textures" } as any;
  });
  const gltf = await new Promise<any>((resolve, reject) => loader.parse(glb, "", resolve, reject));
  return gltf.scene as THREE.Object3D;
}
/* eslint-enable @typescript-eslint/no-explicit-any */

interface Scene {
  renderer: THREE.WebGLRenderer;
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  controls: OrbitControls;
  grid: THREE.GridHelper | null;
  gridSize: number;
  gridAt: THREE.Vector3;
  gridVisible: boolean;
  model: THREE.Object3D | null;
  render: () => void;
  makeGrid: () => void;
  disposed: boolean;
}

export default function ModelViewer(props: ViewerProps) {
  const { url, text, kind, theme, inline } = props;
  const mount = useRef<HTMLDivElement>(null);
  const sceneRef = useRef<Scene | null>(null);
  const [model, setModel] = useState<{ triangles: number } | null>(null);
  const [loadProblem, setProblem] = useState<string | null>(null);
  // WebGL failing is kept apart: the model effect clears its own problem on every new file, never this one.
  const [glProblem, setGlProblem] = useState<string | null>(null);
  const problem = glProblem ?? loadProblem;
  const [wireframe, setWireframe] = useState(false);
  const [grid, setGrid] = useState(true);

  // three.js is set up once per mount; it renders only on change.
  useEffect(() => {
    const host = mount.current;
    if (!host) return;
    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    } catch {
      setGlProblem("This browser can't start WebGL, so the model can't be shown here. Download it to open it elsewhere.");
      return;
    }
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.setClearColor(0x000000, 0);
    renderer.domElement.style.display = "block";
    renderer.domElement.dataset.slot = "model-canvas";
    host.appendChild(renderer.domElement);
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(45, 1, 0.01, 1000);
    camera.position.set(3, 2.5, 4);
    scene.add(camera);
    scene.add(new THREE.HemisphereLight(0xffffff, 0x444444, 1.6));
    const key = new THREE.DirectionalLight(0xffffff, 1.8);
    key.position.set(1, 2, 3);
    key.target.position.set(0, 0, -1);
    camera.add(key, key.target); // the light follows the camera, so the side you see is lit
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.screenSpacePanning = true;
    const s: Scene = {
      renderer, scene, camera, controls, grid: null, gridSize: 10, gridAt: new THREE.Vector3(), gridVisible: true, model: null, disposed: false,
      render: () => renderer.render(scene, camera),
      makeGrid: () => {
        if (s.grid) { scene.remove(s.grid); s.grid.geometry.dispose(); (s.grid.material as THREE.Material).dispose(); }
        const g = new THREE.GridHelper(s.gridSize, 20, cssColor("--foreground", "#888888"), cssColor("--border", "#444444"));
        (g.material as THREE.Material).transparent = true;
        (g.material as THREE.Material).opacity = 0.6;
        g.position.copy(s.gridAt);
        g.visible = s.gridVisible;
        scene.add(g);
        s.grid = g;
      },
    };
    s.makeGrid();
    controls.addEventListener("change", s.render);
    const resize = () => {
      const { clientWidth: w, clientHeight: h } = host;
      if (!w || !h) return;
      renderer.setSize(w, h, false);
      renderer.domElement.style.width = "100%";
      renderer.domElement.style.height = "100%";
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      s.render();
    };
    const observer = new ResizeObserver(resize);
    observer.observe(host);
    resize();
    sceneRef.current = s;
    return () => {
      s.disposed = true;
      observer.disconnect();
      controls.dispose();
      if (s.model) disposeObject(s.model);
      if (s.grid) { s.grid.geometry.dispose(); (s.grid.material as THREE.Material).dispose(); }
      renderer.dispose();
      renderer.forceContextLoss();
      renderer.domElement.remove();
      sceneRef.current = null;
    };
  }, []);

  const fit = useCallback(() => {
    const s = sceneRef.current;
    if (!s?.model) return;
    const box = new THREE.Box3().setFromObject(s.model);
    if (box.isEmpty()) return;
    const size = box.getSize(new THREE.Vector3());
    const center = box.getCenter(new THREE.Vector3());
    const radius = Math.max(size.length() / 2, 1e-6);
    const fov = THREE.MathUtils.degToRad(s.camera.fov);
    const fitFov = s.camera.aspect < 1 ? 2 * Math.atan(Math.tan(fov / 2) * s.camera.aspect) : fov;
    const distance = (radius / Math.sin(fitFov / 2)) * 1.1;
    const dir = s.camera.position.clone().sub(s.controls.target);
    if (dir.lengthSq() < 1e-12) dir.set(1, 0.8, 1.2);
    dir.normalize();
    s.camera.near = distance / 1000;
    s.camera.far = distance * 100;
    s.camera.position.copy(center).addScaledVector(dir, distance);
    s.camera.updateProjectionMatrix();
    s.controls.target.copy(center);
    s.controls.maxDistance = distance * 20;
    s.controls.update();
    s.render();
  }, []);

  // The model: read, parsed, placed on a grid sized to it, framed.
  useEffect(() => {
    let live = true;
    setModel(null);
    setProblem(null);
    (async () => {
      const s = sceneRef.current;
      if (!s) return;
      const buffer = await readBytes({ url, text });
      const object = await loadModel(buffer, kind, cssColor("--primary", "#8899aa"));
      if (!live || s.disposed) { disposeObject(object); return; }
      if (s.model) { s.scene.remove(s.model); disposeObject(s.model); }
      s.model = object;
      s.scene.add(object);
      const box = new THREE.Box3().setFromObject(object);
      const size = box.getSize(new THREE.Vector3());
      const maxDim = Math.max(size.x, size.y, size.z, 1e-6);
      const step = Math.pow(10, Math.floor(Math.log10(maxDim)));
      s.gridSize = Math.ceil((maxDim * 2) / step) * step;
      const center = box.getCenter(new THREE.Vector3());
      s.gridAt = box.isEmpty() ? new THREE.Vector3() : new THREE.Vector3(center.x, box.min.y, center.z);
      s.makeGrid();
      setModel({ triangles: countTriangles(object) });
      fit();
    })().catch((e) => {
      if (!live) return;
      setProblem(e instanceof UserError ? e.message : "That file couldn't be opened as a 3D model. It may be damaged, or use a feature this viewer doesn't support.");
    });
    return () => { live = false; };
  }, [url, text, kind, fit]);

  useEffect(() => {
    const s = sceneRef.current;
    if (!s?.model) return;
    applyWireframe(s.model, wireframe);
    s.render();
  }, [wireframe, model]);

  useEffect(() => {
    const s = sceneRef.current;
    if (!s) return;
    s.gridVisible = grid;
    if (s.grid) s.grid.visible = grid;
    s.render();
  }, [grid]);

  // A new theme: the grid takes its colours, a frame later when the variables are in place.
  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      const s = sceneRef.current;
      if (!s || s.disposed) return;
      s.makeGrid();
      s.render();
    });
    return () => cancelAnimationFrame(frame);
  }, [theme.id, theme.appearance]);

  return (
    <div data-slot="model-viewer" data-loaded={model ? "true" : undefined} className={inline ? "flex h-80 flex-col overflow-hidden rounded-lg border border-border/70" : "flex h-full min-h-0 flex-col"}>
      <div role="toolbar" aria-label="3D view" className="flex flex-wrap items-center gap-3 border-b border-border/70 px-3 py-1.5 text-xs">
        <Button variant="ghost" size="xs" onClick={fit} disabled={!model}><CornersOut /> Fit</Button>
        <label className="flex items-center gap-1.5"><Switch size="sm" checked={wireframe} onCheckedChange={setWireframe} disabled={!model} aria-label="Wireframe" /> Wireframe</label>
        <label className="flex items-center gap-1.5"><Switch size="sm" checked={grid} onCheckedChange={setGrid} aria-label="Grid" /> Grid</label>
        <span className="ml-auto text-muted-foreground tabular-nums">{model ? `${model.triangles.toLocaleString()} triangles` : "Drag to orbit, scroll to zoom, right-drag to pan"}</span>
      </div>
      <div className="relative min-h-0 flex-1 bg-card">
        <div ref={mount} className="absolute inset-0" />
        {problem ? <div className="absolute inset-0 flex items-center justify-center"><ViewerProblem message={problem} /></div> : null}
        {!model && !problem ? <p className="pointer-events-none absolute inset-0 flex items-center justify-center text-sm text-muted-foreground" aria-busy="true">Opening the model…</p> : null}
      </div>
    </div>
  );
}
