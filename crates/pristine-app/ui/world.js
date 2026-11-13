// A small snowy voxel village drawn in perspective on a canvas, used behind the home screen.
// Generated from a seed: original art, no game assets.

const WORLD_BLOCKS = {
  snow:     { top: "#e9eef8", side: "#c9d2e2" },
  dirt:     { top: "#7a5634", side: "#6a4a2c" },
  path:     { top: "#9b7444", side: "#7d5c36" },
  ice:      { top: "#9cc9f2", side: "#7eb0e2" },
  blueice:  { top: "#6fa6f0", side: "#4f86d8" },
  stone:    { top: "#8c93a3", side: "#727a8b" },
  planks:   { top: "#a8743f", side: "#93622f" },
  log:      { top: "#9a6b3c", side: "#5b3a1f" },
  roof:     { top: "#6b4424", side: "#583619" },
  roofsnow: { top: "#eef2fa", side: "#6b4424" },
  leaves:   { top: "#2f5a45", side: "#244a38" },
  leafsnow: { top: "#e4ecf4", side: "#2b5240" },
  window:   { top: "#ffcf7a", side: "#ffbf5c", glow: "#ffb347" },
  door:     { top: "#4a2e18", side: "#3e2614" },
  fence:    { top: "#6e4826", side: "#5a3a1e", size: [0.28, 1, 0.28], see: true },
  lantern:  { top: "#ffd27a", side: "#ffb547", glow: "#ffae3d", size: [0.5, 0.55, 0.5], see: true },
};

function buildWorld(seed) {
  const r = rngFor("world-" + seed), map = new Map();
  const key = (x, y, z) => x + "," + y + "," + z;
  const set = (x, y, z, t) => map.set(key(x, y, z), t);
  const get = (x, y, z) => map.get(key(x, y, z));
  const NX = 56, NZ = 64, cx = NX / 2;
  const pathX = (z) => cx + 2.6 * Math.sin(z * 0.13 + r() * 0.01) + (z > 30 ? (z - 30) * 0.12 : 0);
  const height = (x, z) => {
    if (z < 26 && Math.abs(x - cx) < 18) return 1;
    const n = Math.sin(x * 0.21 + 1.3) + Math.sin(z * 0.17 + 0.4) + 0.6 * Math.sin((x + z) * 0.11);
    return 1 + Math.max(0, Math.round(n * 1.1 + (z - 26) * 0.05));
  };
  const H = [];
  for (let x = 0; x < NX; x++) {
    H[x] = [];
    for (let z = 0; z < NZ; z++) {
      const h = height(x, z); H[x][z] = h;
      for (let y = Math.max(0, h - 3); y < h; y++) set(x, y, z, y === h - 1 ? "snow" : "dirt");
      if (Math.abs(x - pathX(z)) < 2.2 && r() > 0.32) set(x, h - 1, z, "path");
    }
  }
  // a frozen pond on the left
  const px = cx - 13, pz = 9;
  for (let x = px - 4; x <= px + 4; x++) for (let z = pz - 3; z <= pz + 3; z++)
    if ((x - px) ** 2 / 20 + (z - pz) ** 2 / 10 < 1) set(x, 0, z, "ice");

  const used = [];
  const free = (x0, z0, w, d) => !used.some(([a, b, c, e]) => x0 < a + c + 1 && a < x0 + w + 1 && z0 < b + e + 1 && b < z0 + d + 1) && Math.abs(x0 + w / 2 - cx) > w / 2 + 4;
  const ground = (x, z) => (H[x] && H[x][z]) || 1;

  function house(x0, z0, w, d, wallH) {
    used.push([x0, z0, w, d]);
    const g = Math.max(...Array.from({ length: w }, (_, i) => ground(x0 + i, z0)));
    for (let x = x0; x < x0 + w; x++) for (let z = z0; z < z0 + d; z++) for (let y = g - 1; y < g; y++) set(x, y, z, "stone");
    for (let y = g; y < g + wallH; y++) for (let x = x0; x < x0 + w; x++) for (let z = z0; z < z0 + d; z++) {
      const edgeX = x === x0 || x === x0 + w - 1, edgeZ = z === z0 || z === z0 + d - 1;
      if (!edgeX && !edgeZ) continue;
      let t = edgeX && edgeZ ? "log" : "planks";
      const mid = Math.floor(x0 + w / 2);
      if (z === z0 && !edgeX && y === g + 1 && (x === mid - 2 || x === mid + 2) && w > 6) t = "window";
      if (edgeX && !edgeZ && y === g + 1 && (z - z0) % 3 === 1) t = "window";
      if (z === z0 && x === mid && y < g + 2) t = "door";
      set(x, y, z, t);
    }
    // stepped roof along x, with snow on the steps
    for (let k = 0; k <= Math.ceil(d / 2); k++) {
      const y = g + wallH + k, za = z0 - 1 + k, zb = z0 + d - k;
      if (za > zb) break;
      for (let x = x0 - 1; x <= x0 + w; x++) {
        set(x, y, za, "roof"); set(x, y, zb, "roof");
        if (k === Math.ceil(d / 2) || za + 1 >= zb) for (let z = za; z <= zb; z++) set(x, y, z, "roofsnow");
        else { set(x, y + 1, za, "roofsnow"); set(x, y + 1, zb, "roofsnow"); }
      }
      if (za !== zb) for (let z = za + 1; z < zb; z++) for (const x of [x0 - 1 + 1, x0 + w - 1]) if (!get(x, y, z)) set(x, y, z, "planks");
    }
  }
  function spruce(x, z) {
    const g = ground(x, z), h = 4 + Math.floor(r() * 3);
    for (let y = g; y < g + h; y++) set(x, y, z, "log");
    const rings = [[2, 1], [2, 2], [1, 3], [1, 4], [0, 5]];
    for (const [rad, dy] of rings) {
      const y = g + h - 5 + dy;
      for (let dx = -rad; dx <= rad; dx++) for (let dz = -rad; dz <= rad; dz++) {
        if (Math.abs(dx) + Math.abs(dz) > rad + (rad > 1 ? 1 : 0) || (!dx && !dz && dy < 5)) continue;
        set(x + dx, y, z + dz, "leaves");
      }
    }
    for (let dx = -2; dx <= 2; dx++) for (let dz = -2; dz <= 2; dz++) {
      for (let y = g + h + 2; y >= g; y--) { const t = get(x + dx, y, z + dz); if (t === "leaves") { set(x + dx, y, z + dz, "leafsnow"); break; } if (t) break; }
    }
  }

  const spots = [[cx - 20, 18, 9, 7], [cx + 9, 14, 10, 8], [cx - 15, 33, 8, 7], [cx + 6, 36, 9, 7], [cx - 4, 48, 10, 8], [cx + 17, 30, 8, 7]];
  for (const [x, z, w, d] of spots) {
    const jx = x + Math.floor((r() - 0.5) * 4), jz = z + Math.floor((r() - 0.5) * 4);
    if (free(jx, jz, w, d)) house(jx, jz, w, d, 4 + Math.floor(r() * 2));
  }
  for (let n = 0, placed = 0; n < 200 && placed < 22; n++) {
    const x = 2 + Math.floor(r() * (NX - 4)), z = 6 + Math.floor(r() * (NZ - 8));
    if (!free(x - 2, z - 2, 4, 4) || Math.abs(x - pathX(z)) < 6) continue;
    used.push([x - 2, z - 2, 4, 4]); spruce(x, z); placed++;
  }
  // a blue ice spire, and lanterns on posts along the path
  const ix = cx - 7, iz = 22;
  if (free(ix, iz, 2, 2)) { used.push([ix, iz, 2, 2]); for (let y = 1; y < 9; y++) for (const [a, b] of [[0, 0], [1, 0], [0, 1], [1, 1]]) if (y < 6 || a + b < 2) set(ix + a, y, iz + b, "blueice"); for (let y = 9; y < 12; y++) set(ix, y, iz, "blueice"); }
  for (let z = 6; z < 44; z += 7) for (const side of [-1, 1]) {
    const x = Math.round(pathX(z) + side * 4), g = ground(x, z);
    if (get(x, g, z)) continue;
    set(x, g, z, "fence"); set(x, g + 1, z, "fence"); set(x, g + 2, z, "lantern");
  }
  return { map, NX, NZ, cx, pathX };
}

function renderWorld(canvas, seed) {
  const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
  const W = Math.max(1, Math.round(canvas.clientWidth * dpr)), H = Math.max(1, Math.round(canvas.clientHeight * dpr));
  canvas.width = W; canvas.height = H;
  const g = canvas.getContext("2d");
  const world = buildWorld(seed), { map, cx } = world, r = rngFor("cam-" + seed);

  const sky = g.createLinearGradient(0, 0, 0, H);
  sky.addColorStop(0, "#0d1530"); sky.addColorStop(0.45, "#2a3a63"); sky.addColorStop(1, "#7c8fb6");
  g.fillStyle = sky; g.fillRect(0, 0, W, H);
  for (let i = 0; i < 90; i++) { g.fillStyle = `rgba(255,255,255,${0.25 + r() * 0.5})`; g.fillRect(r() * W, r() * H * 0.35, 1.5 * dpr, 1.5 * dpr); }

  const C = { x: cx + 0.5, y: 11.5, z: -5 }, yaw = (r() - 0.5) * 0.12, pitch = 0.4;
  const cy = Math.cos(yaw), sy = Math.sin(yaw), cp = Math.cos(pitch), sp = Math.sin(pitch);
  const f = Math.max(W * 0.62, H * 1.05);
  const proj = (x, y, z) => {
    const dx = x - C.x, dy = y - C.y, dz = z - C.z;
    const x1 = dx * cy - dz * sy, z1 = dx * sy + dz * cy;
    const y2 = dy * cp + z1 * sp, z2 = -dy * sp + z1 * cp;
    if (z2 < 0.4) return null;
    return [W / 2 + (f * x1) / z2, H * 0.42 - (f * y2) / z2];
  };
  const fog = [124, 143, 182];
  const hex = (h) => [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)];
  const shade = (col, k, t, jit) => {
    const c = hex(col).map((v, i) => Math.min(255, v * k * (1 + jit)) * (1 - t) + fog[i] * t);
    return `rgb(${c[0] | 0},${c[1] | 0},${c[2] | 0})`;
  };

  const solid = (x, y, z) => { const t = map.get(x + "," + y + "," + z); return t && !WORLD_BLOCKS[t].see; };
  const blocks = [];
  for (const [k, t] of map) {
    const [x, y, z] = k.split(",").map(Number);
    const d = Math.hypot(x + 0.5 - C.x, y + 0.5 - C.y, z + 0.5 - C.z);
    blocks.push({ x, y, z, t, d });
  }
  blocks.sort((a, b) => b.d - a.d);
  const glows = [];

  for (const b of blocks) {
    const def = WORLD_BLOCKS[b.t], [sx, syz, sz] = def.size || [1, 1, 1];
    const x0 = b.x + (1 - sx) / 2, x1 = x0 + sx, y0 = b.y, y1 = b.y + syz, z0 = b.z + (1 - sz) / 2, z1 = z0 + sz;
    const t = Math.max(0, Math.min(0.85, (b.d - 14) / 46)), jit = ((hash(b.x * 73 + b.z * 19 + b.y * 7) % 13) - 6) / 100;
    const faces = [];
    if (C.y > y1 && (def.see || !solid(b.x, b.y + 1, b.z))) faces.push([[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1], def.top, 1]);
    if (C.z < z0 && (def.see || !solid(b.x, b.y, b.z - 1))) faces.push([[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], def.side, 0.8]);
    if (C.x < x0 && (def.see || !solid(b.x - 1, b.y, b.z))) faces.push([[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], def.side, 0.66]);
    if (C.x > x1 && (def.see || !solid(b.x + 1, b.y, b.z))) faces.push([[x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0], def.side, 0.7]);
    if (C.z > z1 && (def.see || !solid(b.x, b.y, b.z + 1))) faces.push([[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], def.side, 0.6]);
    for (const [a, bq, c, dq, col, k] of faces) {
      const n = b.d < 18 ? 3 : 1;
      for (let i = 0; i < n; i++) for (let j = 0; j < n; j++) {
        const lerp = (p, q, s) => [p[0] + (q[0] - p[0]) * s, p[1] + (q[1] - p[1]) * s, p[2] + (q[2] - p[2]) * s];
        const pt = (u, v) => lerp(lerp(a, bq, u), lerp(dq, c, u), v);
        const quad = [pt(i / n, j / n), pt((i + 1) / n, j / n), pt((i + 1) / n, (j + 1) / n), pt(i / n, (j + 1) / n)].map((p) => proj(...p));
        if (quad.some((p) => !p)) continue;
        const pj = n > 1 ? ((hash(`${b.x},${b.y},${b.z},${i},${j},${k}`) % 11) - 5) / 100 : 0;
        g.beginPath(); g.moveTo(...quad[0]); for (let q = 1; q < 4; q++) g.lineTo(...quad[q]); g.closePath();
        const fill = shade(col, k, t, jit + pj);
        g.fillStyle = fill; g.strokeStyle = fill; g.lineWidth = 0.6; g.fill(); g.stroke();
      }
      const outline = [a, bq, c, dq].map((p) => proj(...p));
      if (!outline.some((p) => !p) && b.d < 30) {
        g.beginPath(); g.moveTo(...outline[0]); for (let q = 1; q < 4; q++) g.lineTo(...outline[q]); g.closePath();
        g.strokeStyle = `rgba(10,14,30,${0.14 * (1 - t)})`; g.lineWidth = 1; g.stroke();
      }
    }
    if (def.glow) { const p = proj(b.x + 0.5, b.y + 0.5, b.z + 0.5); if (p) glows.push([p, b.d, def.glow]); }
  }
  g.globalCompositeOperation = "lighter";
  for (const [[x, y], d, col] of glows) {
    const rad = (f * 2.2) / d, gr = g.createRadialGradient(x, y, 0, x, y, rad);
    gr.addColorStop(0, col + "88"); gr.addColorStop(1, col + "00");
    g.fillStyle = gr; g.fillRect(x - rad, y - rad, rad * 2, rad * 2);
  }
  g.globalCompositeOperation = "source-over";
  // where the player stands, for the overlay to line up with
  const feet = proj(cx + 0.5, 1, 9);
  return feet ? { x: feet[0] / dpr, y: feet[1] / dpr } : null;
}
