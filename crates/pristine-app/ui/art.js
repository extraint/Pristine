// Procedural art: blocky scenery, pixel avatars, sample skins and a CSS-3D skin renderer.
// Everything here is generated, original artwork.

function hash(s) { let h = 5381; for (const c of String(s)) h = ((h << 5) + h + c.charCodeAt(0)) | 0; return Math.abs(h); }
function rngFor(seed) {
  let a = hash(seed) || 1;
  return () => { a |= 0; a = (a + 0x6D2B79F5) | 0; let t = Math.imul(a ^ (a >>> 15), 1 | a); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
}

const THEMES = [
  { sky: ["#6aa8ff", "#f0d9ff"], sun: "#ffe066", far: "#9db6f2", mid: "#5bc08a", grass: "#3fae55", dirt: "#8a5a3c", leaf: "#2f9d4b", trunk: "#7b4a2a" },
  { sky: ["#ff9e7a", "#ffd9a8"], sun: "#fff3b0", far: "#e7a6a6", mid: "#cf8f5a", grass: "#8bc34a", dirt: "#7b4b32", leaf: "#6aa43a", trunk: "#6b3f24" },
  { sky: ["#8f7bff", "#ffc2e8"], sun: "#fff6c9", far: "#b5a4ff", mid: "#7ab7e8", grass: "#58c4a0", dirt: "#6c4e6f", leaf: "#3aa889", trunk: "#5c3d57" },
  { sky: ["#4ec3d1", "#c8f7d6"], sun: "#ffe27a", far: "#86d6c8", mid: "#4fbf86", grass: "#2fa56a", dirt: "#7a5a3a", leaf: "#2b8f5f", trunk: "#6e4a2b" },
  { sky: ["#2b2f77", "#d977c7"], sun: "#ffe9a8", far: "#5a54b8", mid: "#7a5fc4", grass: "#4aa3a0", dirt: "#4a3a63", leaf: "#3c8d8a", trunk: "#3d2f55" },
];

/** A blocky landscape as an SVG string; same seed, same scene. */
let sceneCount = 0;
const SCENE_DIMS = { card: [48, 27], wide: [96, 40], banner: [128, 26] };
function sceneSVG(seed, mode = "card") {
  if (mode === true) mode = "wide";
  const [W, H] = SCENE_DIMS[mode] || SCENE_DIMS.card, gid = `sc${++sceneCount}`;
  const r = rngFor(seed), T = THEMES[hash(seed) % THEMES.length], B = 4, k = H / 27;
  const hm = (base, amp, p1, p2) => Array.from({ length: W }, (_, x) => Math.round(base + amp * Math.sin(x * 0.17 + p1) + amp * 0.55 * Math.sin(x * 0.43 + p2)));
  const far = hm(14 * k, 3 * k, r() * 6, r() * 6), mid = hm(17 * k, 2.4 * k, r() * 6, r() * 6), front = hm(20 * k, 1.8 * k, r() * 6, r() * 6);
  let o = "";
  const rect = (x, y, w, h, f, op) => { o += `<rect x="${x * B}" y="${y * B}" width="${w * B}" height="${h * B}" fill="${f}"${op ? ` opacity="${op}"` : ""}/>`; };

  // critters (flatten the ground beneath them first)
  const critters = [];
  const sc = W / 48, spots = [4 + Math.floor(r() * 5), 15 + Math.floor(r() * 4), 25 + Math.floor(r() * 4), 36 + Math.floor(r() * 5)].map((c) => Math.floor(c * sc));
  const kinds = ["pig", "villager", "cube", "pig"].sort(() => r() - 0.5);
  spots.forEach((c, i) => { const w = kinds[i] === "pig" ? 6 : 3; for (let x = c; x < c + w && x < W; x++) front[x] = front[c]; critters.push({ c, kind: kinds[i], gy: front[c] }); });

  o += `<defs><linearGradient id="${gid}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${T.sky[0]}"/><stop offset="1" stop-color="${T.sky[1]}"/></linearGradient></defs>`;
  o += `<rect width="${W * B}" height="${H * B}" fill="url(#${gid})"/>`;
  const sx = 6 + Math.floor(r() * (W - 14)); rect(sx, 3, 3, 3, T.sun);
  for (let i = 0; i < (W > 48 ? 8 : 4); i++) { const cx = Math.floor(r() * (W - 8)), cy = 3 + Math.floor(r() * 7), cw = 4 + Math.floor(r() * 3); rect(cx, cy, cw, 1, "#fff", .85); rect(cx + 1, cy - 1, cw - 2, 1, "#fff", .85); }
  for (let x = 0; x < W; x++) rect(x, far[x], 1, H - far[x], T.far);
  for (let x = 0; x < W; x++) rect(x, mid[x], 1, H - mid[x], T.mid);
  for (let x = 0; x < W; x++) { rect(x, front[x], 1, H - front[x], T.dirt); rect(x, front[x], 1, 1, T.grass); if ((x * 7 + hash(seed)) % 5 === 0) rect(x, front[x] + 2, 1, 1, "#000", .12); }

  // trees on columns not used by critters
  const used = new Set(); critters.forEach((k) => { for (let x = k.c - 1; x < k.c + 7; x++) used.add(x); });
  for (let n = 0, placed = 0; n < 80 && placed < (W > 48 ? 10 : 5); n++) {
    const x = 2 + Math.floor(r() * (W - 5)); if (used.has(x) || used.has(x + 1)) continue;
    const g = front[x]; for (let k = x - 1; k <= x + 1; k++) used.add(k);
    rect(x, g - 4, 1, 4, T.trunk); rect(x - 1, g - 7, 3, 3, T.leaf); rect(x, g - 8, 1, 1, T.leaf); placed++;
  }
  // generic blocky creatures
  for (const k of critters) {
    const { c, gy } = k;
    if (k.kind === "pig") { rect(c, gy - 3, 4, 2, "#f4a6b8"); rect(c + 4, gy - 3, 2, 2, "#f7b6c6"); rect(c + 5, gy - 2, 1, 1, "#e98aa1"); rect(c + 4, gy - 3, 1, 1, "#2a1d2b"); [c, c + 1, c + 2, c + 3].forEach((x) => rect(x, gy - 1, 1, 1, "#e68ba0")); }
    else if (k.kind === "cube") { rect(c, gy - 3, 3, 3, "#ffd23f"); rect(c, gy - 3, 3, 1, "#ffe27a"); rect(c, gy - 2, 1, 1, "#2a2230"); rect(c + 2, gy - 2, 1, 1, "#2a2230"); rect(c + 1, gy - 1, 1, 1, "#d99a1c"); }
    else { rect(c, gy - 3, 2, 3, "#8b5e3c"); rect(c, gy - 5, 2, 2, "#e0b08a"); rect(c + 1, gy - 4, 1, 1, "#b98862"); rect(c, gy - 4, 1, 1, "#2f6b45"); rect(c, gy - 3, 2, 1, "#6e482c"); }
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W * B} ${H * B}" preserveAspectRatio="${mode === "banner" ? "xMidYMid" : "xMidYMax"} slice" shape-rendering="crispEdges">${o}</svg>`;
}

/** 8x8 pixel-art face for an account, as a PNG data URL. */
function pixelAvatar(seed) {
  const r = rngFor(seed), c = document.createElement("canvas"); c.width = c.height = 8;
  const g = c.getContext("2d"), skins = ["#f2c9a0", "#d9a577", "#b27a52", "#f7d9bd"], hairs = ["#3b2a1d", "#1f1a2b", "#a0522d", "#e8c15a", "#7a3b8f", "#2a6f9e"];
  const skin = skins[Math.floor(r() * skins.length)], hair = hairs[Math.floor(r() * hairs.length)], eye = ["#2c3e8f", "#2b6b3a", "#3b2a1d"][Math.floor(r() * 3)];
  g.fillStyle = skin; g.fillRect(0, 0, 8, 8); g.fillStyle = hair; g.fillRect(0, 0, 8, 3); g.fillRect(0, 3, 1, 1); g.fillRect(7, 3, 1, 1);
  g.fillStyle = "#fff"; g.fillRect(1, 4, 2, 1); g.fillRect(5, 4, 2, 1); g.fillStyle = eye; g.fillRect(2, 4, 1, 1); g.fillRect(5, 4, 1, 1);
  g.fillStyle = "#8a4f3a"; g.fillRect(3, 6, 2, 1);
  return c.toDataURL();
}

/** A simple original 64x64 sample skin, as a PNG data URL. */
function sampleSkin(seed) {
  const r = rngFor(seed), c = document.createElement("canvas"); c.width = c.height = 64; const g = c.getContext("2d");
  const pick = (a) => a[Math.floor(r() * a.length)];
  const skin = pick(["#f2c9a0", "#d9a577", "#b27a52"]), hair = pick(["#3b2a1d", "#1f1a2b", "#a0522d", "#e8c15a", "#7a3b8f"]);
  const shirt = pick(["#4f6df6", "#e5484d", "#16a34a", "#f59e0b", "#8b5cf6", "#0ea5e9"]), pants = pick(["#27324d", "#3a2f55", "#2d3b2f", "#4a3a2a"]), shoe = "#1c1f2e";
  const f = (col, x, y, w, h) => { g.fillStyle = col; g.fillRect(x, y, w, h); };
  f(skin, 0, 0, 32, 16); f(hair, 8, 0, 8, 8); f(hair, 0, 8, 32, 3); f(hair, 24, 8, 8, 8);            // head
  f("#fff", 9, 12, 2, 1); f("#fff", 13, 12, 2, 1); f("#2c3e8f", 10, 12, 1, 1); f("#2c3e8f", 13, 12, 1, 1); f("#8a4f3a", 11, 14, 2, 1); // face
  f(shirt, 16, 16, 24, 16);                                                                            // body
  [[40, 16], [32, 48]].forEach(([x, y]) => { f(skin, x, y, 16, 16); f(shirt, x, y, 16, 6); });         // arms
  [[0, 16], [16, 48]].forEach(([x, y]) => { f(pants, x, y, 16, 16); f(shoe, x, y + 12, 16, 4); });     // legs
  return c.toDataURL();
}

/**
 * Build a rotatable 3D player from a 64x64 / 64x32 skin using CSS transforms.
 * Returns { set(url, slim), destroy() }.
 */
function skin3d(root, opts = {}) {
  const S = opts.size || 9, spin = opts.spin !== false, base = opts.rotY ?? -28, rig = document.createElement("div"); rig.className = "rig"; root.replaceChildren(rig);
  let rotY = base, rotX = opts.rotX ?? -8, drag = null, stopped = false, last = performance.now(), released = 0;
  const apply = () => { rig.style.transform = `rotateX(${rotX}deg) rotateY(${rotY}deg)`; };
  root.addEventListener("pointerdown", (e) => { drag = { x: e.clientX, y: e.clientY, ry: rotY, rx: rotX }; root.setPointerCapture(e.pointerId); });
  root.addEventListener("pointermove", (e) => { if (!drag) return; rotY = drag.ry + (e.clientX - drag.x) * 0.6; rotX = Math.max(-40, Math.min(40, drag.rx - (e.clientY - drag.y) * 0.4)); apply(); });
  root.addEventListener("pointerup", () => { drag = null; released = performance.now(); });
  const tick = (t) => {
    if (stopped || !root.isConnected) return;
    if (!drag && root.offsetParent) {
      if (spin) rotY += (t - last) * 0.02;
      else if (t - released > 1500) { const want = base + 9 * Math.sin(t / 1600); rotY += (want - rotY) * Math.min(1, (t - last) / 400); rotX += ((opts.rotX ?? -8) - rotX) * Math.min(1, (t - last) / 400); }
      apply();
    }
    last = t; requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick); apply();

  function part(url, legacy, texH, [W, H, D], [u, v], [x, y, z], grow) {
    const g = grow, w = (W + 2 * g) * S, h = (H + 2 * g) * S, d = (D + 2 * g) * S;
    const wrap = document.createElement("div"); wrap.className = "part"; wrap.style.transform = `translate3d(${x * S}px,${y * S}px,${z * S}px)`;
    const face = (fw, fh, [fu, fv, tw, th], tf) => {
      const el = document.createElement("div"); el.className = "face";
      Object.assign(el.style, { width: fw + "px", height: fh + "px", marginLeft: -fw / 2 + "px", marginTop: -fh / 2 + "px", transform: tf,
        backgroundImage: `url(${url})`, backgroundSize: `${64 * S * (fw / (tw * S))}px ${texH * S * (fh / (th * S))}px`,
        backgroundPosition: `${-fu * S * (fw / (tw * S))}px ${-fv * S * (fh / (th * S))}px` });
      wrap.append(el);
    };
    face(w, h, [u + D, v + D, W, H], `translateZ(${d / 2}px)`);                              // front
    face(w, h, [u + 2 * D + W, v + D, W, H], `rotateY(180deg) translateZ(${d / 2}px)`);      // back
    face(d, h, [u, v + D, D, H], `rotateY(-90deg) translateZ(${w / 2}px)`);                  // character's right
    face(d, h, [u + D + W, v + D, D, H], `rotateY(90deg) translateZ(${w / 2}px)`);           // character's left
    face(w, d, [u + D, v, W, D], `rotateX(90deg) translateZ(${h / 2}px)`);                   // top
    face(w, d, [u + D + W, v, W, D], `rotateX(-90deg) translateZ(${h / 2}px)`);              // bottom
    return wrap;
  }

  return {
    set(url, slim) {
      const img = new Image();
      img.onload = () => {
        const legacy = img.naturalHeight === 32, th = legacy ? 32 : 64, aw = slim ? 3 : 4, ax = 4 + aw / 2;
        const parts = [];
        const add = (dim, uv, pos, grow = 0) => parts.push(part(url, legacy, th, dim, uv, pos, grow));
        add([8, 8, 8], [0, 0], [0, -10, 0]); add([8, 8, 8], [32, 0], [0, -10, 0], 0.5);                     // head + hat
        add([8, 12, 4], [16, 16], [0, 0, 0]); if (!legacy) add([8, 12, 4], [16, 32], [0, 0, 0], 0.5);        // body + jacket
        add([aw, 12, 4], [40, 16], [-ax, 0, 0]); if (!legacy) add([aw, 12, 4], [40, 32], [-ax, 0, 0], 0.5);  // right arm
        add([aw, 12, 4], legacy ? [40, 16] : [32, 48], [ax, 0, 0]); if (!legacy) add([aw, 12, 4], [48, 48], [ax, 0, 0], 0.5); // left arm
        add([4, 12, 4], [0, 16], [-2, 12, 0]); if (!legacy) add([4, 12, 4], [0, 32], [-2, 12, 0], 0.5);       // right leg
        add([4, 12, 4], legacy ? [0, 16] : [16, 48], [2, 12, 0]); if (!legacy) add([4, 12, 4], [0, 48], [2, 12, 0], 0.5); // left leg
        rig.replaceChildren(...parts);
      };
      img.src = url;
    },
    destroy() { stopped = true; },
  };
}

/** The face (plus hat layer) of a skin, as a PNG data URL. */
function skinHead(url) {
  return new Promise((res) => {
    const img = new Image();
    img.onload = () => {
      const c = document.createElement("canvas"); c.width = c.height = 8; const g = c.getContext("2d");
      g.drawImage(img, 8, 8, 8, 8, 0, 0, 8, 8); if (img.naturalHeight >= 32) g.drawImage(img, 40, 8, 8, 8, 0, 0, 8, 8);
      res(c.toDataURL());
    };
    img.onerror = () => res(null);
    img.src = url;
  });
}

/** Flat front-view thumbnail of a skin on a canvas. */
function skinThumb(url) {
  const c = document.createElement("canvas"); c.width = 16; c.height = 32; const g = c.getContext("2d"); g.imageSmoothingEnabled = false;
  const img = new Image();
  img.onload = () => {
    const legacy = img.naturalHeight === 32, d = (sx, sy, w, h, dx, dy) => g.drawImage(img, sx, sy, w, h, dx, dy, w, h);
    d(8, 8, 8, 8, 4, 0); d(40, 8, 8, 8, 4, 0); d(20, 20, 8, 12, 4, 8); if (!legacy) d(20, 36, 8, 12, 4, 8);
    d(44, 20, 4, 12, 0, 8); if (!legacy) d(44, 36, 4, 12, 0, 8); d(legacy ? 44 : 36, legacy ? 20 : 52, 4, 12, 12, 8); if (!legacy) d(52, 52, 4, 12, 12, 8);
    d(4, 20, 4, 12, 4, 20); if (!legacy) d(4, 36, 4, 12, 4, 20); d(legacy ? 4 : 20, legacy ? 20 : 52, 4, 12, 8, 20); if (!legacy) d(4, 52, 4, 12, 8, 20);
  };
  img.src = url;
  return c;
}
