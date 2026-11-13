// Home: a full-screen world with the player in the middle and a play card that goes
// straight into a server or a world.

const home = {
  mode: null, servers: [], worlds: [], status: new Map(), list: false, adding: false,
  hero: null, skinFor: null, skin: null, worldSeed: null, feet: null, loadedFor: null,
};
const ICONS = {
  chat: '<svg viewBox="0 0 24 24"><path d="M5 5h14a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H10l-4 3v-3H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z"/><path d="M8 9.5h8M8 12.5h5"/></svg>',
  addFriend: '<svg viewBox="0 0 24 24"><circle cx="10" cy="8" r="3.4"/><path d="M3.5 19.5c.6-3.3 3.2-5.2 6.5-5.2 1.5 0 2.9.4 4 1.1M18 13v6M15 16h6"/></svg>',
  gauge: '<svg viewBox="0 0 24 24"><rect x="3.5" y="4.5" width="17" height="15" rx="2.5"/><path d="M7 15l3-4 3 2.5 4-5"/></svg>',
  arrow: '<svg viewBox="0 0 24 24"><path d="M5 12h13M13 6l6 6-6 6"/></svg>',
  plus: '<svg viewBox="0 0 24 24"><path d="M12 5v14M5 12h14"/></svg>',
  x: '<svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18"/></svg>',
};
const svgEl = (name, cls = "ico") => el("span", { className: cls, innerHTML: ICONS[name] });
const store = {
  get(k) { try { return JSON.parse(localStorage.getItem("home." + k)); } catch { return null; } },
  set(k, v) { try { localStorage.setItem("home." + k, JSON.stringify(v)); } catch {} },
};

// ---------- the player's skin ----------
async function heroSkin() {
  const a = currentAccount();
  const id = a.online ? "ms:" + a.seed : "off:" + a.name;
  if (home.skinFor === id && home.skin) return home.skin;
  let skin = null;
  if (a.online) {
    try { const s = await invoke("player_skin", { uuid: a.seed }); skin = { url: "data:image/png;base64," + s.data, slim: s.slim }; } catch {}
  }
  if (!skin) {
    const saved = await invoke("list_skins").catch(() => []);
    skin = saved.length ? { url: "data:image/png;base64," + saved[0].data, slim: false } : { url: sampleSkin("skin-" + a.name), slim: false };
  }
  home.skinFor = id; home.skin = skin;
  state.head = await skinHead(skin.url);
  paintAccount();
  return skin;
}

// ---------- data for the selected version ----------
async function loadPlayData(i) {
  if (!i) { home.servers = []; home.worlds = []; return; }
  const [servers, worlds] = await Promise.all([
    invoke("list_servers", { id: i.id }).catch(() => []),
    invoke("world_list", { id: i.id }).catch(() => []),
  ]);
  home.servers = servers; home.worlds = worlds; home.loadedFor = i.id;
  home.mode = store.get("mode." + i.id) || (servers.length || !worlds.length ? "multi" : "single");
}
const selServer = (i) => home.servers.find((s) => s.ip === store.get("server." + i.id)) || home.servers[0] || null;
const selWorld = (i) => home.worlds.find((w) => w.folder === store.get("world." + i.id)) || home.worlds[0] || null;

async function pingServer(ip, force = false) {
  const cur = home.status.get(ip);
  if (cur && !force && Date.now() - cur.at < 60000) return;
  home.status.set(ip, { at: Date.now(), pending: true });
  let st;
  try { st = { ...(await invoke("ping_server", { address: ip })), at: Date.now() }; } catch { st = { at: Date.now(), offline: true }; }
  home.status.set(ip, st);
  if (state.view === "home") paintCard();
}

// ---------- render ----------
async function renderHome() {
  const sec = $("view-home");
  const byRecent = [...state.instances].sort((a, b) => (b.last_played || 0) - (a.last_played || 0));
  const sel = inst(state.selected) || byRecent[0] || null;
  state.selected = sel ? sel.id : null;
  if (!sec.querySelector(".world")) buildHome(sec);
  paintWorld(sel ? sel.id : "pristine");
  if (sel && home.loadedFor !== sel.id) await loadPlayData(sel);
  if (state.view !== "home") return;
  paintCard(); paintPills(); paintFoot();
  heroSkin().then((s) => { if (home.hero) home.hero.set(s.url, s.slim); }).catch(() => {});
  $("hero-name").textContent = currentAccount().name;
  if (sel && home.mode === "multi") home.servers.slice(0, 4).forEach((s) => pingServer(s.ip));
}

function buildHome(sec) {
  const canvas = el("canvas", { className: "world-canvas" });
  const hero = el("div", { className: "hero", id: "hero" },
    el("div", { className: "nametag" }, el("b", { id: "hero-name" }), el("span", { textContent: "You" })),
    el("div", { className: "hero3d", id: "hero3d" }), el("div", { className: "hero-shadow" }));
  sec.replaceChildren(
    el("div", { className: "world" }, canvas, el("div", { className: "world-fx" })),
    hero,
    el("div", { className: "playcard", id: "playcard" }),
    el("div", { className: "pills", id: "pills" }),
    el("footer", { className: "homefoot", id: "homefoot" }));
  home.hero = skin3d($("hero3d"), { size: 8, spin: false, rotY: -18, rotX: -6 });
  let t;
  new ResizeObserver(() => { clearTimeout(t); t = setTimeout(() => { home.worldSeed = null; if (state.view === "home") paintWorld(home.lastSeed); }, 150); }).observe(canvas);
}

function paintWorld(seed) {
  home.lastSeed = seed;
  if (home.worldSeed === seed) return;
  const canvas = document.querySelector("#view-home .world-canvas");
  if (!canvas || !canvas.clientWidth) return;
  home.worldSeed = seed;
  const feet = renderWorld(canvas, seed);
  const hero = $("hero");
  if (feet && hero) { hero.style.left = feet.x + "px"; hero.style.top = feet.y + "px"; }
}

function versionPicker(sel) {
  const s = el("select", { className: "verpick", ariaLabel: "Version" }, ...state.instances.map((i) => el("option", { value: i.id, textContent: `${vname(i)} · ${i.name}`, selected: i.id === sel.id })));
  s.addEventListener("change", () => { state.selected = s.value; home.loadedFor = null; home.list = false; renderHome(); });
  return s;
}

function serverIcon(s, cls = "sicon") {
  const st = home.status.get(s.ip), src = (st && st.favicon) || (s.icon ? "data:image/png;base64," + s.icon : null);
  if (src) return el("img", { className: cls, src, alt: "" });
  const d = el("div", { className: cls + " ph", textContent: (s.name || s.ip)[0].toUpperCase() }); d.style.background = gradient(s.ip); return d;
}
function worldIcon(w, cls = "sicon") {
  if (w.icon) return el("img", { className: cls, src: "data:image/png;base64," + w.icon, alt: "" });
  return el("div", { className: cls + " art", innerHTML: sceneSVG("w-" + w.folder, "card") });
}
function playedAgo(ms) { return ms ? ago(Math.round(ms / 1000)) : "Never played"; }

function serverLine(s) {
  const st = home.status.get(s.ip);
  if (!st || st.pending) return el("div", { className: "sline" }, el("i", { className: "live wait" }), el("span", { textContent: "Checking…" }));
  if (st.offline) return el("div", { className: "sline" }, el("i", { className: "live" }), el("span", { textContent: "Can't reach server" }));
  return el("div", { className: "sline" }, el("i", { className: "live on" }),
    el("span", { textContent: st.online != null ? `${compact.format(st.online)} online` : "Online" }),
    st.ping_ms != null ? el("span", { className: "ms", textContent: `${st.ping_ms} ms` }) : null);
}

function paintCard() {
  const card = $("playcard"), sel = inst(state.selected);
  if (!card) return;
  if (!sel) {
    const b = el("button", { className: "bigplay", textContent: "Create a version" });
    b.addEventListener("click", openNew);
    card.replaceChildren(el("div", { className: "pc-empty" }, el("strong", { textContent: "Welcome to Pristine" }),
      el("p", { textContent: "Create a version, or grab a modpack from Browse, to start playing." })), b);
    return;
  }
  const multi = home.mode === "multi";
  const seg = el("div", { className: "pc-seg" }, ...[["single", "Singleplayer"], ["multi", "Multiplayer"]].map(([m, label]) => {
    const b = el("button", { className: home.mode === m ? "on" : "", textContent: label });
    b.addEventListener("click", () => { home.mode = m; home.list = false; home.adding = false; store.set("mode." + sel.id, m); paintCard(); if (m === "multi") home.servers.slice(0, 4).forEach((s) => pingServer(s.ip)); });
    return b;
  }));

  const server = selServer(sel), world = selWorld(sel);
  if (multi) [server, ...recentItems(sel, true).slice(0, 3)].forEach((s) => s && !home.status.has(s.ip) && pingServer(s.ip));
  let target;
  if (multi && server) target = el("div", { className: "target" }, serverIcon(server, "sicon big"),
    el("div", { className: "tt" }, el("strong", { textContent: server.name, title: server.ip }), serverLine(server)));
  else if (multi) target = el("div", { className: "target none" }, el("div", { className: "tt" }, el("strong", { textContent: "No servers yet" }), el("small", { textContent: "Add one to join it straight from here." })));
  else if (world) target = el("div", { className: "target" }, worldIcon(world, "sicon big"),
    el("div", { className: "tt" }, el("strong", { textContent: world.name, title: world.folder }), el("small", { textContent: playedAgo(world.last_played) })));
  else target = el("div", { className: "target none" }, el("div", { className: "tt" }, el("strong", { textContent: "No worlds yet" }), el("small", { textContent: "Play opens the game so you can make one." })));

  const bar = el("i"), status = el("div", { className: "status" }), play = el("button", { className: "play bigplay" }), stop = el("button", { className: "stop", textContent: "Stop" });
  stop.hidden = true;
  controller(sel, {
    bar, status, play, stop,
    quick: () => {
      const q = multi ? (server ? { kind: "server", target: server.ip } : null) : (world ? { kind: "world", target: world.folder } : null);
      if (q) remember(sel, multi, q.target);
      return q;
    },
  });
  if (multi && !server) { play.disabled = true; play.textContent = "Add a server first"; }

  const items = multi ? home.servers : home.worlds;
  const recent = el("div", { className: "recent" }, el("span", { className: "lbl", textContent: "Recent" }),
    el("div", { className: "ricons" }, ...recentItems(sel, multi).slice(0, 3).map((it) => {
      const on = multi ? server && it.ip === server.ip : world && it.folder === world.folder;
      const b = el("button", { className: "ricon" + (on ? " on" : ""), title: it.name }, multi ? serverIcon(it) : worldIcon(it));
      b.addEventListener("click", () => pick(sel, it));
      return b;
    })));
  const all = el("button", { className: "allbtn" }, el("span", { textContent: items.length ? "All" : multi ? "Add" : "All" }), items.length ? el("b", { textContent: items.length }) : null, svgEl(items.length || !multi ? "arrow" : "plus"));
  all.addEventListener("click", () => { home.list = !home.list; home.adding = multi && !items.length; paintCard(); });
  if (!multi && !items.length) all.disabled = true;
  recent.append(all);

  card.replaceChildren(seg, el("div", { className: "pc-ver" }, versionPicker(sel)), target,
    el("div", { className: "pc-play" }, el("div", { className: "btns" }, play, stop), el("div", { className: "bar" }, bar), status),
    recent, ...(home.list ? [listPanel(sel, multi)] : []));
}

// Most recently picked first (worlds also by when they were last played), then the rest in list order.
function recentItems(sel, multi) {
  const keyOf = (it) => (multi ? it.ip : it.folder);
  const order = store.get(`recent.${multi ? "m" : "s"}.${sel.id}`) || [];
  const items = multi ? home.servers : home.worlds;
  const rank = (it) => { const n = order.indexOf(keyOf(it)); return n < 0 ? 1e6 + items.indexOf(it) : n; };
  return multi ? [...items].sort((a, b) => rank(a) - rank(b)) : items;
}
function remember(sel, multi, key) {
  const k = `recent.${multi ? "m" : "s"}.${sel.id}`;
  store.set(k, [key, ...(store.get(k) || []).filter((x) => x !== key)].slice(0, 12));
}

function pick(sel, it) {
  if (home.mode === "multi") { store.set("server." + sel.id, it.ip); remember(sel, true, it.ip); pingServer(it.ip); }
  else { store.set("world." + sel.id, it.folder); remember(sel, false, it.folder); }
  home.list = false; paintCard();
}

function listPanel(sel, multi) {
  const items = multi ? home.servers : home.worlds;
  const close = el("button", { className: "icon-btn sm", ariaLabel: "Close", innerHTML: ICONS.x });
  close.addEventListener("click", () => { home.list = false; home.adding = false; paintCard(); });
  const rows = items.map((it) => {
    const row = el("div", { className: "lrow", tabIndex: 0 }, multi ? serverIcon(it) : worldIcon(it),
      el("div", { className: "tt" }, el("strong", { textContent: it.name }), multi ? serverLine(it) : el("small", { textContent: playedAgo(it.last_played) })));
    row.addEventListener("click", () => pick(sel, it));
    row.addEventListener("keydown", (e) => { if (e.key === "Enter") pick(sel, it); });
    if (multi) {
      const rm = el("button", { className: "icon-btn sm rm", title: "Remove from list", ariaLabel: "Remove " + it.name, innerHTML: ICONS.x });
      rm.addEventListener("click", async (e) => {
        e.stopPropagation();
        if (!confirm(`Remove ${it.name} from your server list?`)) return;
        try { await invoke("remove_server", { id: sel.id, ip: it.ip }); await loadPlayData(sel); paintCard(); } catch (err) { toast(String(err), true); }
      });
      row.append(rm);
      if (!home.status.has(it.ip)) pingServer(it.ip);
    }
    return row;
  });
  const panel = el("div", { className: "lpanel" }, el("div", { className: "lh" }, el("strong", { textContent: multi ? "Servers" : "Worlds" }), close),
    el("div", { className: "lrows" }, ...(rows.length ? rows : [el("div", { className: "muted small", textContent: multi ? "No servers yet." : "No worlds yet." })])));
  if (multi) {
    if (home.adding) {
      const name = el("input", { placeholder: "Name (optional)", maxLength: 40, spellcheck: false });
      const ip = el("input", { placeholder: "Server address", spellcheck: false, autocomplete: "off" });
      const save = el("button", { className: "primary", type: "submit", textContent: "Add" });
      const cancel = el("button", { type: "button", textContent: "Cancel" });
      cancel.addEventListener("click", () => { home.adding = false; paintCard(); });
      const form = el("form", { className: "addform" }, ip, name, el("div", { className: "row2" }, cancel, save));
      form.addEventListener("submit", async (e) => {
        e.preventDefault();
        try {
          await invoke("add_server", { id: sel.id, name: name.value, ip: ip.value });
          store.set("server." + sel.id, ip.value.trim()); remember(sel, true, ip.value.trim());
          home.adding = false; home.list = false; await loadPlayData(sel); home.mode = "multi"; paintCard(); pingServer(ip.value.trim());
        } catch (err) { toast(String(err), true); }
      });
      panel.append(form);
      setTimeout(() => ip.focus(), 0);
    } else {
      const add = el("button", { className: "addbtn" }, svgEl("plus"), "Add server");
      add.addEventListener("click", () => { home.adding = true; paintCard(); });
      panel.append(add);
    }
    panel.append(el("small", { className: "muted note", textContent: "This is the same list as the game's Multiplayer screen." }));
  }
  return panel;
}

function paintPills() {
  const pills = $("pills"); if (!pills) return;
  const a = currentAccount(), sel = inst(state.selected);
  const social = el("button", { className: "pill" }, svgEl("chat"), el("span", { textContent: "Press Enter to open Social" }));
  social.addEventListener("click", (e) => { e.stopPropagation(); openSocial(); });
  const head = el("img", { className: "phead", src: state.head || pixelAvatar(a.seed), alt: "" });
  const add = el("button", { className: "icon-btn sm", title: "Add friend", ariaLabel: "Add friend", innerHTML: ICONS.addFriend });
  add.addEventListener("click", (e) => { e.stopPropagation(); openSocial(); });
  const me = el("div", { className: "pill me" }, head, el("div", { className: "two" }, el("strong", { textContent: a.name }), el("small", { textContent: a.online ? "You" : "You · offline" })), add);
  const running = sel && state.running.has(sel.id);
  const perf = el("div", { className: "pill perf" }, svgEl("gauge"), el("div", { className: "two r" },
    el("strong", { textContent: running ? "Running" : sel ? (sel.installed ? "Ready" : "Not installed") : "No version" }),
    el("small", { textContent: sel ? `${gb(sel.max_memory_mb)} RAM` : "—" })));
  pills.replaceChildren(social, me, perf);
}

function paintFoot() {
  const f = $("homefoot"), sel = inst(state.selected); if (!f) return;
  const mark = el("span", { className: "fbrand", innerHTML: '<svg class="mark" viewBox="0 0 32 32" aria-hidden="true"><path d="M16 3l11 6.5v13L16 29 5 22.5v-13z"/><path d="M10.5 18.5a6.5 6.5 0 0 1 11 0"/></svg>PRISTINE' });
  const bits = sel ? [`${loaderLabel(sel.loader)}${sel.loader_version ? " " + sel.loader_version : ""}`, `Minecraft ${sel.mc_version}`] : [];
  f.replaceChildren(el("div", { className: "fl" }, mark, ...bits.map((b) => el("span", { textContent: b }))),
    el("div", { className: "fr", textContent: "Not an official Minecraft product. Not affiliated with Mojang or Microsoft." }));
}

function openSocial() { closePanels(); $("social").hidden = false; paintSocial(); }

window.addEventListener("keydown", (e) => {
  if (e.key !== "Enter" || state.view !== "home" || e.metaKey || e.ctrlKey || e.altKey) return;
  const t = e.target;
  if (t && (t.closest("input, select, textarea, button, dialog, .lrow, [contenteditable]"))) return;
  if (document.querySelector("dialog[open]")) return;
  e.preventDefault(); openSocial();
});
// Keep the pills honest when a game starts or stops.
listen("game-exit", () => { if (state.view === "home") setTimeout(paintPills, 50); });
