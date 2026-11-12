const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const state = {
  instances: [], modules: [], versions: [], latest: "", kind: "release", loader: "fabric",
  view: "home", instId: null, tab: "mod", content: [], query: "",
  busy: new Set(), running: new Set(), ctl: new Map(),
  acct: { configured: false, selected: null, accounts: [] },
  featured: null, seq: 0,
  browse: { kind: "mod", target: null, offset: 0, total: 0, loading: false },
};
if (/Mac/i.test(navigator.platform) || /Mac OS X/.test(navigator.userAgent)) document.documentElement.classList.add("mac");

// ---------- helpers ----------
function el(tag, props = {}, ...kids) {
  const e = Object.assign(document.createElement(tag), props);
  kids.forEach((k) => k != null && e.append(k));
  return e;
}
function toast(msg, bad = false) {
  const t = $("toast");
  t.textContent = msg; t.className = "toast" + (bad ? " bad" : ""); t.hidden = false;
  clearTimeout(toast.timer); toast.timer = setTimeout(() => (t.hidden = true), 5500);
}
function busy(text) {
  $("busy-text").textContent = text; $("busy").hidden = false;
  return () => { $("busy").hidden = true; };
}
function hash(s) { let h = 5381; for (const c of String(s)) h = ((h << 5) + h + c.charCodeAt(0)) | 0; return Math.abs(h); }
function gradient(seed) {
  const h = hash(seed) % 360;
  return `linear-gradient(135deg, hsl(${h} 72% 46%), hsl(${(h + 55) % 360} 70% 34%))`;
}
function ago(secs) {
  if (!secs) return "Never played";
  const d = secs - Date.now() / 1000, rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  for (const [u, s] of [["day", 86400], ["hour", 3600], ["minute", 60]]) if (Math.abs(d) >= s) return "Played " + rtf.format(Math.round(d / s), u);
  return "Played just now";
}
const gb = (mb) => (mb / 1024).toFixed(mb % 1024 ? 1 : 0) + " GB";
const compact = new Intl.NumberFormat(undefined, { notation: "compact" });
const cap = (s) => s.charAt(0).toUpperCase() + s.slice(1);
const loaderLabel = (l) => ({ vanilla: "Vanilla", fabric: "Fabric", quilt: "Quilt" }[l] || l);
function projectIcon(url, title, cls = "icon") {
  const ph = () => { const d = el("div", { className: cls + " ph", textContent: (title || "?")[0].toUpperCase() }); d.style.background = gradient(title); return d; };
  if (!url) return ph();
  const img = el("img", { className: cls, src: url, alt: "", loading: "lazy" });
  img.addEventListener("error", () => img.replaceWith(ph()));
  return img;
}
const inst = (id) => state.instances.find((i) => i.id === id);

// ---------- accounts ----------
let offlineName = "Player";
try { offlineName = localStorage.getItem("username") || "Player"; } catch {}

function currentAccount() {
  const sel = state.acct.accounts.find((x) => x.uuid === state.acct.selected);
  return sel ? { name: sel.username, kind: "Microsoft account", seed: sel.uuid } : { name: offlineName.trim() || "Player", kind: "Offline account", seed: offlineName };
}
function avatar(name, seed, node = el("div", { className: "avatar" })) {
  node.textContent = (name.trim()[0] || "P").toUpperCase(); node.style.background = gradient(seed || name); return node;
}
function paintAccount() {
  const a = currentAccount();
  $("acct-name").textContent = a.name; $("acct-kind").textContent = a.kind; avatar(a.name, a.seed, $("avatar"));
}
async function loadAccounts() { state.acct = await invoke("accounts_state"); paintAccount(); }

function renderAccountList() {
  const { accounts, selected } = state.acct, rows = [];
  const off = el("input", { value: offlineName, maxLength: 16, spellcheck: false, ariaLabel: "Offline username" });
  off.addEventListener("input", () => { offlineName = off.value; try { localStorage.setItem("username", off.value); } catch {} paintAccount(); });
  off.addEventListener("click", (e) => e.stopPropagation());
  const offRadio = el("input", { type: "radio", name: "acct", checked: !selected });
  offRadio.addEventListener("change", async () => { await invoke("select_account", { uuid: null }); await loadAccounts(); renderAccountList(); });
  rows.push(el("label", { className: "acct" + (!selected ? " on" : "") }, offRadio, avatar(offlineName, offlineName),
    el("div", { className: "who" }, off, el("small", { textContent: "Offline · can't join online servers" }))));
  for (const a of accounts) {
    const radio = el("input", { type: "radio", name: "acct", checked: selected === a.uuid });
    const rm = el("button", { className: "x", type: "button", textContent: "Remove" });
    rm.addEventListener("click", async (e) => { e.preventDefault(); e.stopPropagation(); await invoke("remove_account", { uuid: a.uuid }); await loadAccounts(); renderAccountList(); });
    radio.addEventListener("change", async () => { await invoke("select_account", { uuid: a.uuid }); await loadAccounts(); renderAccountList(); });
    rows.push(el("label", { className: "acct" + (selected === a.uuid ? " on" : "") }, radio, avatar(a.username, a.uuid),
      el("div", { className: "who" }, el("strong", { textContent: a.username }), el("small", { textContent: "Microsoft account" })), rm));
  }
  $("acct-list").replaceChildren(...rows);
}
$("account-btn").addEventListener("click", () => { renderAccountList(); $("accounts-dialog").showModal(); });
$("accounts-close").addEventListener("click", () => $("accounts-dialog").close());
function openUrl(url) {
  if (!/^https:\/\//.test(url)) return;
  const o = window.__TAURI__.opener;
  if (o && o.openUrl) o.openUrl(url).catch(() => window.open(url)); else window.open(url);
}
$("add-ms").addEventListener("click", async () => {
  if (!state.acct.configured) { $("accounts-dialog").close(); go("settings"); return toast("Add your Azure client ID first to enable Microsoft sign-in", true); }
  try {
    const dc = await invoke("ms_login_start"); state.dc = dc;
    $("code").textContent = dc.user_code; $("signin-status").textContent = "Waiting for you to approve in your browser…";
    $("accounts-dialog").close(); $("signin-dialog").showModal(); openUrl(dc.verification_uri);
  } catch (e) { toast(String(e), true); }
});
$("open-link").addEventListener("click", () => state.dc && openUrl(state.dc.verification_uri));
$("copy-code").addEventListener("click", () => { navigator.clipboard?.writeText($("code").textContent).then(() => toast("Code copied")).catch(() => {}); });
$("signin-cancel").addEventListener("click", async () => { await invoke("ms_login_cancel"); $("signin-dialog").close(); });
$("signin-dialog").addEventListener("cancel", () => invoke("ms_login_cancel"));
listen("ms-login", async ({ payload: p }) => {
  if (p.ok) { $("signin-dialog").close(); await loadAccounts(); toast(`Signed in as ${p.username}`); }
  else { $("signin-status").textContent = p.error || "Sign-in failed"; toast(p.error || "Sign-in failed", true); }
});

// ---------- navigation ----------
const VIEWS = ["home", "library", "browse", "instance", "settings"];
function go(view) {
  state.view = view;
  VIEWS.forEach((v) => ($("view-" + v).hidden = v !== view));
  const navView = view === "instance" ? "library" : view;
  document.querySelectorAll(".nav").forEach((n) => n.classList.toggle("active", n.dataset.view === navView));
  $("main").scrollTop = 0;
  renderCurrent();
}
function renderCurrent() {
  state.ctl.clear();
  ({ home: renderHome, library: renderLibrary, browse: renderBrowse, instance: renderInstance, settings: loadInfo })[state.view]();
}
document.querySelectorAll(".nav").forEach((b) => b.addEventListener("click", () => go(b.dataset.view)));
function openInstance(id, tab = "mod") { state.instId = id; state.tab = tab; go("instance"); }

async function refresh() {
  state.instances = await invoke("list_instances");
  renderCurrent();
}

// ---------- play controller (shared by hero, cards, instance header) ----------
function controller(i, { bar, status, play }) {
  const sync = () => {
    const running = state.running.has(i.id), working = state.busy.has(i.id);
    play.disabled = working || running;
    play.textContent = running ? "Running…" : working ? "Working…" : i.installed ? "Play" : "Install & play";
    if (running) status.textContent = "Game is running";
  };
  play.addEventListener("click", async (e) => {
    e.stopPropagation();
    state.busy.add(i.id); sync(); status.textContent = "Preparing…";
    try {
      await invoke("launch_instance", { id: i.id, username: offlineName });
      state.running.add(i.id); i.installed = true; status.textContent = "Game is running";
    } catch (err) { toast(String(err), true); status.textContent = ""; }
    finally { state.busy.delete(i.id); bar.style.width = "0"; sync(); }
  });
  const c = { bar, status, play, sync };
  if (!state.ctl.has(i.id)) state.ctl.set(i.id, new Set());
  state.ctl.get(i.id).add(c);
  sync();
}
const STAGES = { "version manifest": "Fetching versions", libraries: "Downloading libraries", assets: "Downloading assets", "java runtime": "Installing Java", modpack: "Fetching modpack", "modpack files": "Downloading modpack files" };
listen("progress", ({ payload: p }) => {
  const text = `${STAGES[p.stage] || p.stage} · ${p.done}/${p.total}`;
  if (p.id === "__modpack") { $("busy-text").textContent = "Installing modpack — " + text; return; }
  for (const c of state.ctl.get(p.id) || []) { c.bar.style.width = (p.total ? (p.done / p.total) * 100 : 0) + "%"; c.status.textContent = text; }
});
listen("game-exit", ({ payload: p }) => {
  state.running.delete(p.id);
  for (const c of state.ctl.get(p.id) || []) { c.status.textContent = p.code ? `Exited with code ${p.code}` : ""; c.sync(); }
  if (p.code) toast(`Game exited with code ${p.code}`, true);
  refresh().catch(() => {});
});

// ---------- cards ----------
function badgesFor(i) {
  return [loaderLabel(i.loader), i.installed ? "Installed" : "Not installed", ...(i.modules.length ? [`${i.modules.length} module${i.modules.length > 1 ? "s" : ""}`] : [])]
    .map((t) => el("span", { className: "badge", textContent: t }));
}
function renderCard(i, n) {
  const bar = el("i"), status = el("div", { className: "status" }), play = el("button", { className: "primary play" });
  const banner = el("div", { className: "banner" }, el("div", { className: "badges" }, ...badgesFor(i)),
    el("span", { className: "ver" + (i.mc_version.length > 8 ? " long" : ""), textContent: i.mc_version }));
  banner.style.background = gradient(i.id);
  const card = el("div", { className: "card", tabIndex: 0 }, banner,
    el("div", { className: "body" },
      el("div", {}, el("h3", { textContent: i.name, title: i.name }), el("div", { className: "meta", textContent: `${gb(i.max_memory_mb)} · ${ago(i.last_played)}` })),
      el("div", { className: "bar" }, bar), status, play));
  card.style.animationDelay = Math.min(n, 8) * 40 + "ms";
  card.addEventListener("click", () => openInstance(i.id));
  card.addEventListener("keydown", (e) => { if (e.key === "Enter" && e.target === card) openInstance(i.id); });
  controller(i, { bar, status, play });
  return card;
}
function renderHeroFor(i) {
  const bar = el("i"), status = el("div", { className: "status" }), play = el("button", { className: "play" });
  const hero = el("div", { className: "hero" },
    el("div", {}, el("div", { className: "eyebrow", textContent: "Jump back in" }), el("h2", { textContent: i.name }),
      el("div", { className: "sub", textContent: `${loaderLabel(i.loader)} ${i.mc_version} · ${ago(i.last_played)}` })),
    el("div", { className: "cta" }, play, el("div", { className: "bar" }, bar), status));
  hero.style.background = gradient(i.id);
  controller(i, { bar, status, play });
  return hero;
}

// ---------- home ----------
async function renderHome() {
  const h = new Date().getHours(), name = currentAccount().name;
  $("greeting").textContent = `${h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening"}, ${name}`;
  const byRecent = [...state.instances].sort((a, b) => (b.last_played || 0) - (a.last_played || 0));
  const hero = $("hero");
  if (byRecent.length) hero.replaceChildren(renderHeroFor(byRecent[0]));
  else {
    const go1 = el("button", { className: "play", textContent: "Create an instance" });
    go1.addEventListener("click", openNew);
    const c = el("div", { className: "hero" }, el("div", {}, el("div", { className: "eyebrow", textContent: "Get started" }),
      el("h2", { textContent: "Welcome to Pristine" }), el("div", { className: "sub", textContent: "Create an instance or install a modpack to start playing." })),
      el("div", { className: "cta" }, go1));
    c.style.background = gradient("pristine"); hero.replaceChildren(c);
  }
  const rest = byRecent.slice(1, 4);
  $("recent-title").hidden = !rest.length;
  $("recent").replaceChildren(...rest.map(renderCard));

  if (!state.featured) {
    try {
      const r = await invoke("search_content", { params: { query: "", kind: "modpack", loader: "fabric", offset: 0, index: "downloads" } });
      state.featured = r.hits.slice(0, 6);
    } catch { state.featured = []; }
  }
  if (state.view !== "home") return;
  $("featured").replaceChildren(...(state.featured.length ? state.featured.map((p) => {
    const btn = el("button", { textContent: "Install" });
    btn.addEventListener("click", () => installModpack(p, btn));
    return el("div", { className: "pack" }, projectIcon(p.icon_url, p.title),
      el("div", { className: "txt" }, el("strong", { textContent: p.title }), el("small", { textContent: `${compact.format(p.downloads)} downloads · ${p.author}` })), btn);
  }) : [el("div", { className: "muted", textContent: "Couldn't reach Modrinth. Check your connection." })]));
}
$("home-new").addEventListener("click", openNew);

// ---------- library ----------
function renderLibrary() {
  const q = state.query.trim().toLowerCase(), all = state.instances;
  const list = q ? all.filter((i) => (i.name + " " + i.mc_version + " " + i.loader).toLowerCase().includes(q)) : all;
  $("empty").hidden = all.length > 0;
  $("no-match").hidden = !(all.length && !list.length);
  $("lib-sub").textContent = all.length ? `${all.length} instance${all.length === 1 ? "" : "s"}` : "";
  document.querySelector("#view-library .tools").hidden = !all.length;
  $("grid").replaceChildren(...list.map(renderCard));
}
$("search").addEventListener("input", (e) => { state.query = e.target.value; renderLibrary(); });
$("empty-new").addEventListener("click", openNew);
$("empty-browse").addEventListener("click", () => { state.browse.kind = "modpack"; go("browse"); });

// ---------- modpacks ----------
async function installModpack(p, btn) {
  const end = busy("Installing modpack…");
  if (btn) btn.disabled = true;
  try {
    const i = await invoke("install_modpack", { project: p.slug });
    await refresh();
    toast(`Created "${i.name}" — press Play to download the game and start`);
    openInstance(i.id, "mod");
  } catch (e) { toast(String(e), true); if (btn) btn.disabled = false; }
  finally { end(); }
}

// ---------- browse ----------
const KINDS = [["mod", "Mods"], ["modpack", "Modpacks"], ["resourcepack", "Resource packs"], ["shader", "Shaders"], ["datapack", "Data packs"]];
function renderBrowse() {
  const b = state.browse;
  $("kinds").replaceChildren(...KINDS.map(([k, label]) => {
    const btn = el("button", { textContent: label, className: b.kind === k ? "on" : "" });
    btn.addEventListener("click", () => { b.kind = k; renderBrowse(); });
    return btn;
  }));
  const target = $("target"), cur = b.target && inst(b.target) ? b.target : state.instances[0]?.id;
  b.target = cur || null;
  target.replaceChildren(...state.instances.map((i) => el("option", { value: i.id, textContent: `${i.name} (${loaderLabel(i.loader)} ${i.mc_version})`, selected: i.id === cur })));
  $("target-row").hidden = b.kind === "modpack";
  runSearch(true);
}
$("target").addEventListener("change", (e) => { state.browse.target = e.target.value; runSearch(true); });
$("compat").addEventListener("change", () => runSearch(true));
$("sort").addEventListener("change", () => runSearch(true));
let qTimer;
$("q").addEventListener("input", () => { clearTimeout(qTimer); qTimer = setTimeout(() => runSearch(true), 300); });
$("more").addEventListener("click", () => runSearch(false));

async function runSearch(reset) {
  const b = state.browse, mine = ++state.seq;
  if (reset) { b.offset = 0; $("results").replaceChildren(el("p", { className: "muted center", textContent: "Searching…" })); }
  const i = b.target ? inst(b.target) : null, compat = $("compat").checked && i && b.kind !== "modpack";
  let r;
  try {
    r = await invoke("search_content", { params: {
      query: $("q").value.trim(), kind: b.kind, offset: b.offset, index: $("sort").value,
      mc_version: compat ? i.mc_version : null,
      loader: compat && b.kind === "mod" && i.loader !== "vanilla" ? i.loader : null,
    } });
  } catch (e) { if (mine === state.seq) $("results").replaceChildren(el("p", { className: "muted center", textContent: "Couldn't reach Modrinth: " + e })); return; }
  if (mine !== state.seq) return;
  const rows = r.hits.map(renderHit);
  if (reset) $("results").replaceChildren(...(rows.length ? rows : [el("p", { className: "muted center", textContent: "Nothing found." })]));
  else $("results").append(...rows);
  b.offset += r.hits.length; b.total = r.total;
  $("more").hidden = b.offset >= r.total || !r.hits.length;
}

function renderHit(h) {
  const b = state.browse, btn = el("button", { className: "primary", textContent: b.kind === "modpack" ? "Install" : "Add" });
  btn.addEventListener("click", () => (b.kind === "modpack" ? installModpack(h, btn) : installHit(h, btn)));
  return el("div", { className: "hit" }, projectIcon(h.icon_url, h.title),
    el("div", { className: "txt" },
      el("div", { className: "t" }, el("strong", { textContent: h.title }), el("span", { textContent: "by " + h.author })),
      el("p", { textContent: h.description }),
      el("div", { className: "stats" }, el("span", { textContent: compact.format(h.downloads) + " downloads" }),
        ...h.categories.slice(0, 4).map((c) => el("span", { className: "tag", textContent: c })))), btn);
}

function pickWorld(id) {
  return new Promise(async (resolve) => {
    const worlds = await invoke("list_worlds", { id }).catch(() => []);
    if (!worlds.length) { toast("This instance has no worlds yet. Launch the game and create one first.", true); return resolve(null); }
    $("world-select").replaceChildren(...worlds.map((w) => el("option", { value: w, textContent: w })));
    const d = $("world-dialog"), done = (v) => { d.close(); resolve(v); };
    $("world-ok").onclick = () => done($("world-select").value);
    $("world-cancel").onclick = () => done(null);
    d.addEventListener("cancel", () => resolve(null), { once: true });
    d.showModal();
  });
}
async function installHit(h, btn) {
  const id = state.browse.target, kind = state.browse.kind;
  if (!id) return toast("Create an instance first", true);
  let world = null;
  if (kind === "datapack") { world = await pickWorld(id); if (!world) return; }
  btn.disabled = true; btn.textContent = "Adding…";
  try {
    const names = await invoke("install_content", { id, project: h.slug, kind, world });
    btn.textContent = names.length ? "Added ✓" : "Already added";
    toast(names.length ? `Added ${names.join(", ")} to ${inst(id).name}` : `${h.title} is already in ${inst(id).name}`);
  } catch (e) { toast(String(e), true); btn.disabled = false; btn.textContent = "Add"; }
}

// ---------- instance page ----------
const TABS = [["mod", "Mods", "mods"], ["resourcepack", "Resource packs", "resourcepacks"], ["shader", "Shaders", "shaders"], ["datapack", "Data packs", "datapacks"], ["modules", "Modules", "modules"], ["settings", "Settings", "settings"]];

async function renderInstance() {
  const i = inst(state.instId);
  if (!i) return go("library");
  const sec = $("view-instance");
  state.content = await invoke("list_content", { id: i.id }).catch(() => []);
  if (state.view !== "instance") return;

  const back = el("button", { className: "back", textContent: "← Library" });
  back.addEventListener("click", () => go("library"));
  const bar = el("i"), status = el("div", { className: "status" }), play = el("button", { className: "play" });
  const head = el("div", { className: "ihead" },
    el("div", { className: "top" }, back),
    el("div", { className: "hrow" },
      el("div", {}, el("h1", { textContent: i.name }), el("div", { className: "badges" }, ...badgesFor(i), el("span", { className: "badge", textContent: `MC ${i.mc_version}` }))),
      el("div", { className: "cta" }, play, el("div", { className: "bar" }, bar), status)));
  head.style.background = gradient(i.id);
  controller(i, { bar, status, play });

  const tabs = el("div", { className: "tabs", role: "tablist" }, ...TABS.map(([key, label]) => {
    const n = state.content.filter((c) => c.kind === key).length;
    const b = el("button", { className: state.tab === key ? "on" : "" }, label, n ? el("span", { className: "count", textContent: n }) : null);
    b.addEventListener("click", () => { state.tab = key; renderInstance(); });
    return b;
  }));
  const body = el("div");
  sec.replaceChildren(head, tabs, body);
  if (state.tab === "modules") renderModules(body, i);
  else if (state.tab === "settings") renderInstSettings(body, i);
  else renderContentTab(body, i, state.tab);
}

function renderContentTab(body, i, kind) {
  const items = state.content.filter((c) => c.kind === kind);
  const label = TABS.find((t) => t[0] === kind)[1].toLowerCase();
  if (kind === "mod" && i.loader === "vanilla") {
    const b = el("button", { className: "primary", textContent: "Switch to Fabric" });
    b.addEventListener("click", async () => { await invoke("set_loader", { id: i.id, loader: "fabric" }); await refresh(); toast("Now using Fabric. Press Play to install it."); });
    body.append(el("div", { className: "notice" }, el("span", { textContent: "Mods need Fabric or Quilt. This instance is vanilla." }), b));
  }
  const add = el("button", { className: "primary", textContent: `Add ${label}` });
  add.addEventListener("click", () => { state.browse.kind = kind; state.browse.target = i.id; go("browse"); });
  if (!items.length) {
    body.append(el("div", { className: "empty" }, el("h2", { textContent: `No ${label} yet` }), el("p", { className: "muted", textContent: "Find some on Modrinth, or turn on a module." }), add));
    return;
  }
  const list = el("div", { className: "list" }, ...items.map((c) => {
    const sw = el("input", { type: "checkbox", checked: c.enabled, ariaLabel: "Enabled" });
    sw.addEventListener("change", async () => {
      try { await invoke("toggle_content", { id: i.id, file: c.file }); renderInstance(); } catch (e) { toast(String(e), true); sw.checked = !sw.checked; }
    });
    const rm = el("button", { className: "ghost danger", textContent: "Remove" });
    rm.addEventListener("click", async () => {
      if (!confirm(`Remove ${c.title}?`)) return;
      try { await invoke("remove_content", { id: i.id, file: c.file }); renderInstance(); } catch (e) { toast(String(e), true); }
    });
    const world = c.kind === "datapack" ? c.file.split("/")[1] : null;
    return el("div", { className: "row" + (c.enabled ? "" : " off") }, projectIcon(c.icon_url, c.title, "icon sm"),
      el("div", { className: "txt" }, el("strong", { textContent: c.title }),
        el("small", {}, c.version_number || (c.managed ? "" : "Added manually"),
          ...(world ? [el("span", { className: "tag", textContent: "World: " + world })] : []),
          ...c.modules.map((m) => el("span", { className: "tag", textContent: state.modules.find((x) => x.id === m)?.name || m })))),
      el("label", { className: "switch" }, sw, el("span")), rm);
  }));
  body.append(el("div", { className: "actions" }, add), list);
  list.style.marginTop = "14px";
}

function renderModules(body, i) {
  const grid = el("div", { className: "mods" });
  for (const m of state.modules) {
    const on = i.modules.includes(m.id), needsLoader = m.projects.length > 0 && i.loader === "vanilla";
    const sw = el("input", { type: "checkbox", checked: on, disabled: needsLoader, ariaLabel: m.name });
    sw.addEventListener("change", async () => {
      sw.disabled = true;
      const end = busy(sw.checked ? `Installing ${m.name}…` : `Removing ${m.name}…`);
      try {
        if (sw.checked) {
          const r = await invoke("enable_module", { id: i.id, module: m.id });
          if (r.skipped.length) console.warn(r.skipped);
          toast(`${m.name}: ${r.installed.length} mod${r.installed.length === 1 ? "" : "s"} added` + (r.skipped.length ? `, ${r.skipped.length} not available for ${i.mc_version}` : ""));
        } else await invoke("disable_module", { id: i.id, module: m.id });
        await refresh();
      } catch (e) { toast(String(e), true); sw.checked = !sw.checked; sw.disabled = false; }
      finally { end(); }
    });
    grid.append(el("div", { className: "module" + (on ? " on" : "") },
      el("div", { className: "h" }, el("h3", { textContent: m.name }), el("label", { className: "switch" }, sw, el("span"))),
      el("p", { textContent: m.description }),
      el("small", { textContent: needsLoader ? "Needs Fabric or Quilt" : m.projects.length ? `${m.projects.length} mod${m.projects.length === 1 ? "" : "s"} + dependencies` : "Launch option" })));
  }
  body.append(grid);
}

function renderInstSettings(body, i) {
  const seg = el("div", { className: "seg" }, ...["fabric", "quilt", "vanilla"].map((l) => {
    const b = el("button", { type: "button", textContent: loaderLabel(l), className: i.loader === l ? "on" : "" });
    b.addEventListener("click", async () => {
      if (l === i.loader) return;
      if (state.content.length && !confirm("Changing the loader can break mods installed for the current one. Continue?")) return;
      try { await invoke("set_loader", { id: i.id, loader: l }); await refresh(); toast(`Now using ${loaderLabel(l)}. Press Play to install it.`); } catch (e) { toast(String(e), true); }
    });
    return b;
  }));
  const val = el("b", { textContent: gb(i.max_memory_mb) });
  const mem = el("input", { type: "range", min: 1024, max: 16384, step: 512, value: i.max_memory_mb });
  mem.addEventListener("input", () => (val.textContent = gb(+mem.value)));
  mem.addEventListener("change", async () => { try { await invoke("set_memory", { id: i.id, mb: +mem.value }); i.max_memory_mb = +mem.value; } catch (e) { toast(String(e), true); } });
  const open = el("button", { textContent: "Open game folder" });
  open.addEventListener("click", () => invoke("open_instance_folder", { id: i.id }).catch((e) => toast(String(e), true)));
  const del = el("button", { className: "danger", textContent: "Delete instance" });
  del.addEventListener("click", async () => {
    if (state.running.has(i.id)) return toast("Close the game before deleting this instance", true);
    if (!confirm(`Delete "${i.name}" and all its worlds and settings? This can't be undone.`)) return;
    try { await invoke("delete_instance", { id: i.id }); await refresh(); go("library"); } catch (e) { toast(String(e), true); }
  });
  body.append(el("div", { className: "settings-grid" },
    el("div", { className: "panel" }, el("strong", { textContent: "Mod loader" }), seg, el("small", { className: "muted", textContent: `Minecraft ${i.mc_version}${i.loader_version ? ` · ${loaderLabel(i.loader)} ${i.loader_version}` : ""}` })),
    el("div", { className: "panel" }, el("strong", {}, "Memory ", val), mem, el("small", { className: "muted", textContent: "Maximum heap given to the game. 4–6 GB suits most modpacks." })),
    el("div", { className: "panel" }, el("div", { className: "actions" }, open, del))));
}

// ---------- new instance ----------
function fillVersions() {
  const sel = $("new-version"), items = state.versions.filter((v) => v.kind === state.kind);
  sel.replaceChildren(...items.map((v) => el("option", { value: v.id, textContent: v.id + (v.id === state.latest ? "  ·  latest" : "") })));
  if (items.length) sel.selectedIndex = 0;
}
function fillModuleChips() {
  const vanilla = state.loader === "vanilla";
  $("new-module-list").replaceChildren(...state.modules.map((m) => {
    const needs = m.projects.length > 0 && vanilla;
    const box = el("input", { type: "checkbox", value: m.id, disabled: needs, checked: !needs && ["performance", "quality-of-life", "jvm-tuning"].includes(m.id) });
    return el("label", { className: "chip", title: m.description }, box, m.name);
  }));
}
$("loader-seg").querySelectorAll("button").forEach((b) => b.addEventListener("click", () => {
  state.loader = b.dataset.loader;
  $("loader-seg").querySelectorAll("button").forEach((x) => x.classList.toggle("on", x === b));
  fillModuleChips();
}));
$("kind-seg").querySelectorAll("button").forEach((b) => b.addEventListener("click", () => {
  state.kind = b.dataset.kind;
  $("kind-seg").querySelectorAll("button").forEach((x) => x.classList.toggle("on", x === b));
  fillVersions();
}));
async function openNew() {
  $("new-error").hidden = true; $("new-name").value = "";
  fillModuleChips();
  if (!$("new-dialog").open) $("new-dialog").showModal();
  $("new-name").focus();
  if (!state.versions.length) {
    $("new-create").disabled = true;
    try { const v = await invoke("list_versions"); state.versions = v.versions; state.latest = v.latest_release; fillVersions(); }
    catch (e) { $("new-error").textContent = "Couldn't load versions: " + e; $("new-error").hidden = false; }
    $("new-create").disabled = false;
  }
}
$("new-btn").addEventListener("click", openNew);
$("new-cancel").addEventListener("click", () => $("new-dialog").close());
$("new-form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const version = $("new-version").value;
  if (!version) { $("new-error").textContent = "Pick a version"; $("new-error").hidden = false; return; }
  const mods = [...$("new-module-list").querySelectorAll("input:checked")].map((x) => x.value);
  let created;
  try { created = await invoke("create_instance", { name: $("new-name").value, version, loader: state.loader }); }
  catch (e) { $("new-error").textContent = String(e); $("new-error").hidden = false; return; }
  $("new-dialog").close();
  await refresh();
  const notes = [];
  if (mods.length) {
    const end = busy("Setting up modules…");
    for (const m of mods) {
      try { await invoke("enable_module", { id: created.id, module: m }); }
      catch (e) { notes.push(String(e)); }
    }
    end();
    await refresh();
  }
  if (notes.length) toast(notes[0], true);
  openInstance(created.id, "modules");
});
window.addEventListener("keydown", (e) => { if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "n") { e.preventDefault(); openNew(); } });

// ---------- settings ----------
async function loadInfo() {
  const i = await invoke("app_info"), names = { macos: "macOS", windows: "Windows", linux: "Linux" };
  $("client-id").value = (await invoke("get_client_id")) || "";
  const rows = [["Data folder", i.data_dir], ["Platform", `${names[i.os] || i.os} · ${i.arch}`], ["Pristine", "v" + i.version]];
  $("info").replaceChildren(...rows.map(([k, v]) => el("div", {}, el("dt", { textContent: k }), el("dd", { textContent: v }))));
}
$("cid-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  try { await invoke("set_client_id", { id: $("client-id").value }); await loadAccounts(); toast("Saved"); } catch (err) { toast(String(err), true); }
});

// ---------- boot ----------
(async () => {
  try { state.modules = await invoke("list_modules"); } catch {}
  await Promise.allSettled([loadAccounts(), refresh()]);
  go("home");
})();
