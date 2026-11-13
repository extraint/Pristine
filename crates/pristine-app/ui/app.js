const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const state = {
  instances: [], modules: [], versions: [], latest: "", kind: "release", loader: "fabric",
  view: "home", instId: null, selected: null, tab: "mod", content: [], query: "",
  busy: new Set(), running: new Set(), ctl: new Map(), notes: [], unread: 0,
  acct: { configured: false, selected: null, accounts: [] }, featured: null,
  logs: new Map(), logFilter: "all", logQuery: "",
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
  state.notes.unshift({ msg, bad, at: new Date() }); state.notes.length = Math.min(state.notes.length, 50);
  state.unread++; $("notif-dot").hidden = false;
  if (typeof paintNotifs === "function") paintNotifs();
}
function busy(text) { $("busy-text").textContent = text; $("busy").hidden = false; return () => { $("busy").hidden = true; }; }
const PALETTE = [["#6366f1", "#a855f7"], ["#0ea5e9", "#6366f1"], ["#ec4899", "#f97316"], ["#10b981", "#0ea5e9"], ["#8b5cf6", "#ec4899"], ["#f59e0b", "#ef4444"]];
function gradient(seed) { const [a, b] = PALETTE[hash(seed) % PALETTE.length]; return `linear-gradient(135deg, ${a}, ${b})`; }
function setGlow(seed) {
  const bg = $("bg");
  if (!seed || !matchMedia("(prefers-color-scheme: dark)").matches) { bg.style.removeProperty("--g1"); bg.style.removeProperty("--g2"); return; }
  const [a, b] = PALETTE[hash(seed) % PALETTE.length]; bg.style.setProperty("--g1", a); bg.style.setProperty("--g2", b);
}
function ago(secs) {
  if (!secs) return "Never played";
  const d = secs - Date.now() / 1000, rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  for (const [u, s] of [["day", 86400], ["hour", 3600], ["minute", 60]]) if (Math.abs(d) >= s) return "Played " + rtf.format(Math.round(d / s), u);
  return "Played just now";
}
const gb = (mb) => (mb / 1024).toFixed(mb % 1024 ? 1 : 0) + " GB";
const compact = new Intl.NumberFormat(undefined, { notation: "compact" });
const loaderLabel = (l) => ({ vanilla: "Vanilla", fabric: "Fabric", quilt: "Quilt" }[l] || l);
const vname = (i) => (i.loader === "vanilla" ? i.mc_version : `${i.mc_version} ${loaderLabel(i.loader)}`);
const inst = (id) => state.instances.find((i) => i.id === id);
function projectIcon(url, title, cls = "icon") {
  const ph = () => { const d = el("div", { className: cls + " ph", textContent: (title || "?")[0].toUpperCase() }); d.style.background = gradient(title); return d; };
  if (!url) return ph();
  const img = el("img", { className: cls, src: url, alt: "", loading: "lazy" });
  img.addEventListener("error", () => img.replaceWith(ph()));
  return img;
}
function art(seed, mode = "card") { return el("div", { className: "art", innerHTML: sceneSVG(seed, mode) }); }

// ---------- accounts ----------
let offlineName = "Player";
try { offlineName = localStorage.getItem("username") || "Player"; } catch {}
function currentAccount() {
  const sel = state.acct.accounts.find((x) => x.uuid === state.acct.selected);
  return sel ? { name: sel.username, kind: "Microsoft account", seed: sel.uuid, online: true } : { name: offlineName.trim() || "Player", kind: "Offline account", seed: offlineName, online: false };
}
function pix(seed, cls = "pix") { return el("img", { className: cls, src: pixelAvatar(seed), alt: "" }); }
function paintAccount() {
  const a = currentAccount();
  $("avatar").src = state.head || pixelAvatar(a.seed); $("account-btn").title = `${a.name} · ${a.kind}`; $("acct-name").textContent = a.name;
  if (typeof paintSocial === "function") paintSocial();
}
async function loadAccounts() { state.acct = await invoke("accounts_state"); paintAccount(); }

function renderAccountList() {
  const { accounts, selected } = state.acct, rows = [];
  const off = el("input", { value: offlineName, maxLength: 16, spellcheck: false, ariaLabel: "Offline username" });
  off.addEventListener("input", () => { offlineName = off.value; try { localStorage.setItem("username", off.value); } catch {} paintAccount(); });
  off.addEventListener("click", (e) => e.stopPropagation());
  const offRadio = el("input", { type: "radio", name: "acct", checked: !selected });
  offRadio.addEventListener("change", async () => { await invoke("select_account", { uuid: null }); await loadAccounts(); renderAccountList(); });
  rows.push(el("label", { className: "acct" + (!selected ? " on" : "") }, offRadio, pix(offlineName),
    el("div", { className: "who2" }, off, el("small", { textContent: "Offline · can't join online servers" }))));
  for (const a of accounts) {
    const radio = el("input", { type: "radio", name: "acct", checked: selected === a.uuid });
    const rm = el("button", { className: "x", type: "button", textContent: "Remove" });
    rm.addEventListener("click", async (e) => { e.preventDefault(); e.stopPropagation(); await invoke("remove_account", { uuid: a.uuid }); await loadAccounts(); renderAccountList(); });
    radio.addEventListener("change", async () => { await invoke("select_account", { uuid: a.uuid }); await loadAccounts(); renderAccountList(); });
    rows.push(el("label", { className: "acct" + (selected === a.uuid ? " on" : "") }, radio, pix(a.uuid),
      el("div", { className: "who2" }, el("strong", { textContent: a.username }), el("small", { textContent: "Microsoft account" })), rm));
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
const VIEWS = ["home", "versions", "browse", "wardrobe", "instance", "settings", "social", "stats", "news"];
function go(view) {
  state.view = view;
  document.body.dataset.view = view;
  VIEWS.forEach((v) => ($("view-" + v).hidden = v !== view));
  const navView = view === "instance" ? "versions" : view;
  document.querySelectorAll(".nav").forEach((n) => n.classList.toggle("active", n.dataset.view === navView));
  $("main").scrollTop = 0;
  if (typeof closePanels === "function") closePanels();
  renderCurrent();
}
function renderCurrent() {
  state.ctl.clear();
  setGlow(null);
  const noop = () => {};
  ({ home: renderHome, versions: renderVersions, browse: renderBrowse, wardrobe: renderWardrobe, instance: renderInstance, settings: loadInfo, social: noop, stats: noop, news: noop })[state.view]();
}
document.querySelectorAll(".nav").forEach((b) => b.addEventListener("click", () => go(b.dataset.view)));
$("btn-settings").addEventListener("click", () => go("settings"));
function openInstance(id, tab = "mod") { state.instId = id; state.tab = tab; go("instance"); }
async function refresh() { state.instances = await invoke("list_instances"); renderCurrent(); }

// ---------- play controller ----------
function controller(i, { bar, status, play, stop, quick }) {
  const sync = () => {
    const running = state.running.has(i.id), working = state.busy.has(i.id);
    play.disabled = working || running;
    play.classList.toggle("running", running);
    play.textContent = running ? "Running" : working ? "Working…" : i.installed ? "Play" : "Install & play";
    if (stop) stop.hidden = !running;
    if (running) status.textContent = "Game is running";
  };
  play.addEventListener("click", async (e) => {
    e.stopPropagation();
    state.busy.add(i.id); sync(); status.textContent = "Preparing…";
    try {
      await invoke("launch_instance", { id: i.id, username: offlineName, quick: (quick && quick()) || null });
      state.running.add(i.id); i.installed = true; status.textContent = "Game is running";
      state.logs.set(i.id, []);
      if (state.view === "home" && typeof paintPills === "function") paintPills();
    } catch (err) { toast(String(err), true); status.textContent = ""; }
    finally { state.busy.delete(i.id); bar.style.width = "0"; sync(); }
  });
  if (stop) stop.addEventListener("click", (e) => { e.stopPropagation(); invoke("stop_instance", { id: i.id }).catch((err) => toast(String(err), true)); });
  const c = { bar, status, play, stop, sync };
  if (!state.ctl.has(i.id)) state.ctl.set(i.id, new Set());
  const set = state.ctl.get(i.id);
  // Cards get rebuilt often (the home card on every ping), so drop ones no longer on screen.
  for (const old of set) if (!old.play.isConnected) set.delete(old);
  set.add(c);
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
listen("game-log", ({ payload: p }) => {
  const buf = state.logs.get(p.id) || []; buf.push(...p.lines);
  if (buf.length > 5000) buf.splice(0, buf.length - 5000);
  state.logs.set(p.id, buf);
  if (state.view === "instance" && state.tab === "log" && state.instId === p.id) appendLog(p.lines);
});

// ---------- cards & home ----------
function badgesFor(i) {
  return [loaderLabel(i.loader), ...(i.installed ? [] : ["Not installed"]), ...(i.modules.length ? [`${i.modules.length} module${i.modules.length > 1 ? "s" : ""}`] : [])]
    .map((t) => el("span", { className: "badge", textContent: t }));
}
function renderCard(i, n) {
  const bar = el("i"), status = el("div", { className: "status" }), play = el("button", { className: "play" });
  status.dataset.meta = ago(i.last_played);
  const card = el("div", { className: "card", tabIndex: 0 }, art(i.id),
    el("div", { className: "badges" }, ...badgesFor(i), el("span", { className: "badge", textContent: i.mc_version })),
    el("div", { className: "foot" }, el("div", { className: "txt" }, el("h3", { textContent: i.name, title: i.name }), status), play),
    el("div", { className: "bar" }, bar));
  card.style.animationDelay = Math.min(n, 8) * 45 + "ms";
  card.addEventListener("click", () => openInstance(i.id));
  card.addEventListener("keydown", (e) => { if (e.key === "Enter" && e.target === card) openInstance(i.id); });
  controller(i, { bar, status, play });
  return card;
}

function renderVersions() {
  const q = state.query.trim().toLowerCase(), all = state.instances;
  const list = q ? all.filter((i) => (i.name + " " + vname(i)).toLowerCase().includes(q)) : all;
  $("empty").hidden = all.length > 0;
  $("no-match").hidden = !(all.length && !list.length);
  $("ver-sub").textContent = all.length ? `${all.length} version${all.length === 1 ? "" : "s"}` : "";
  document.querySelector("#view-versions .tools").hidden = !all.length;
  $("grid").replaceChildren(...list.map(renderCard));
}
$("search").addEventListener("input", (e) => { state.query = e.target.value; renderVersions(); });
$("empty-new").addEventListener("click", () => openNew());
$("empty-browse").addEventListener("click", () => { browse.kind = "modpack"; go("browse"); });

async function installModpack(p, btn) {
  const end = busy("Installing modpack…");
  if (btn) btn.disabled = true;
  try {
    const i = await invoke("install_modpack", { project: p.slug });
    await refresh();
    toast(`Created "${i.name}". Press Play to download the game and start.`);
    openInstance(i.id, "mod");
  } catch (e) { toast(String(e), true); if (btn) btn.disabled = false; }
  finally { end(); }
}

// ---------- version page ----------
const TABS = [["mod", "Mods"], ["resourcepack", "Textures"], ["shader", "Shaders"], ["datapack", "Data packs"], ["modules", "Modules"], ["log", "Live log"], ["settings", "Settings"]];

async function renderInstance() {
  const i = inst(state.instId);
  if (!i) return go("versions");
  setGlow(i.id);
  const sec = $("view-instance");
  state.content = await invoke("list_content", { id: i.id }).catch(() => []);
  if (state.view !== "instance") return;

  const back = el("button", { className: "back", textContent: "← Versions" });
  back.addEventListener("click", () => go("versions"));
  const bar = el("i"), status = el("div", { className: "status" }), play = el("button", { className: "play" }), stop = el("button", { className: "stop", textContent: "Stop" });
  stop.hidden = true;
  const head = el("div", { className: "ihead" }, art(i.id, "banner"), back,
    el("div", { className: "hrow" },
      el("div", {}, el("h1", { textContent: i.name }), el("div", { className: "badges" }, ...badgesFor(i), el("span", { className: "badge", textContent: `Minecraft ${i.mc_version}` }))),
      el("div", { className: "cta" }, el("div", { className: "btns" }, play, stop), el("div", { className: "bar" }, bar), status)));
  controller(i, { bar, status, play, stop });

  const tabs = el("div", { className: "tabs", role: "tablist" }, ...TABS.map(([key, label]) => {
    const n = state.content.filter((c) => c.kind === key).length;
    const b = el("button", { className: state.tab === key ? "on" : "" }, label, n ? el("span", { className: "count", textContent: n }) : null);
    b.addEventListener("click", () => { state.tab = key; renderInstance(); });
    return b;
  }));
  const body = el("div");
  sec.replaceChildren(el("div", { className: "wrap" }, head, el("div", { style: "height:22px" }), tabs, body));
  if (state.tab === "modules") renderModules(body, i);
  else if (state.tab === "settings") renderInstSettings(body, i);
  else if (state.tab === "log") renderLog(body, i);
  else renderContentTab(body, i, state.tab);
}

function renderContentTab(body, i, kind) {
  const items = state.content.filter((c) => c.kind === kind);
  const label = TABS.find((t) => t[0] === kind)[1].toLowerCase();
  if (kind === "mod" && i.loader === "vanilla") {
    const b = el("button", { className: "primary", textContent: "Switch to Fabric" });
    b.addEventListener("click", async () => { await invoke("set_loader", { id: i.id, loader: "fabric" }); await refresh(); toast("Now using Fabric. Press Play to install it."); });
    body.append(el("div", { className: "notice" }, el("span", { textContent: "Mods need Fabric or Quilt. This version is vanilla." }), b));
  }
  const add = el("button", { className: "primary", textContent: `Add ${label}` });
  add.addEventListener("click", () => { browse.kind = kind; browse.target = i.id; browse.page = 0; browse.cats.clear(); go("browse"); });
  if (!items.length) {
    body.append(el("div", { className: "empty" }, el("h2", { textContent: `No ${label} yet` }), el("p", { className: "muted", textContent: "Find some on Modrinth, or turn on a module." }), add));
    return;
  }
  const list = el("div", { className: "list" }, ...items.map((c) => {
    const sw = el("input", { type: "checkbox", checked: c.enabled, ariaLabel: "Enabled" });
    sw.addEventListener("change", async () => { try { await invoke("toggle_content", { id: i.id, file: c.file }); renderInstance(); } catch (e) { toast(String(e), true); sw.checked = !sw.checked; } });
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
          ...c.modules.map((m) => el("span", { className: "tag accent", textContent: state.modules.find((x) => x.id === m)?.name || m })))),
      el("label", { className: "switch" }, sw, el("span")), rm);
  }));
  body.append(el("div", { className: "actions", style: "margin-bottom:14px" }, add), list);
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
  const del = el("button", { className: "danger", textContent: "Delete version" });
  del.addEventListener("click", async () => {
    if (state.running.has(i.id)) return toast("Close the game before deleting this version", true);
    if (!confirm(`Delete "${i.name}" and all its worlds and settings? This can't be undone.`)) return;
    try { await invoke("delete_instance", { id: i.id }); await refresh(); go("versions"); } catch (e) { toast(String(e), true); }
  });
  body.append(el("div", { className: "settings-grid" },
    el("div", { className: "panel" }, el("strong", { textContent: "Mod loader" }), seg, el("small", { className: "muted", textContent: `Minecraft ${i.mc_version}${i.loader_version ? ` · ${loaderLabel(i.loader)} ${i.loader_version}` : ""}` })),
    el("div", { className: "panel" }, el("strong", {}, "Memory ", val), mem, el("small", { className: "muted", textContent: "Maximum heap given to the game. 4–6 GB suits most modpacks." })),
    el("div", { className: "panel" }, el("div", { className: "actions" }, open, del))));
}

// ---------- live log ----------
const LOG_LEVELS = [["all", "All"], ["error", "Errors"], ["warn", "Warnings"], ["info", "Info"], ["debug", "Debug"]];
const MAX_ROWS = 1500;
let logEls = null;

function logRow(l) {
  return el("div", { className: "ln " + l.level }, el("span", { className: "n", textContent: l.n }), el("span", { className: "t", textContent: l.ts || "" }), el("span", { className: "x", textContent: l.text }));
}
const logMatch = (l) => (state.logFilter === "all" || l.level === state.logFilter) && (!state.logQuery || l.text.toLowerCase().includes(state.logQuery.toLowerCase()));

async function renderLog(body, i) {
  if (!state.logs.has(i.id) || !state.logs.get(i.id).length) state.logs.set(i.id, await invoke("get_log", { id: i.id }).catch(() => []));
  const q = el("input", { type: "search", placeholder: "Search log", value: state.logQuery, spellcheck: false });
  const pills = el("div", { className: "pills" });
  const con = el("div", { className: "console" });
  const paint = () => {
    pills.replaceChildren(...LOG_LEVELS.map(([k, label]) => {
      const n = k === "all" ? 0 : (state.logs.get(i.id) || []).filter((l) => l.level === k).length;
      const b = el("button", { className: (state.logFilter === k ? "on " : "") + k, textContent: label + (n && (k === "error" || k === "warn") ? ` ${n}` : "") });
      b.addEventListener("click", () => { state.logFilter = k; paint(); });
      return b;
    }));
    const rows = (state.logs.get(i.id) || []).filter(logMatch).slice(-MAX_ROWS);
    con.replaceChildren(...(rows.length ? rows.map(logRow) : [el("div", { className: "none", textContent: (state.logs.get(i.id) || []).length ? "No lines match." : "No output yet. Launch the game and its log will stream in here." })]));
    con.scrollTop = con.scrollHeight;
  };
  q.addEventListener("input", () => { state.logQuery = q.value; paint(); });
  const copy = el("button", { textContent: "Copy" });
  copy.addEventListener("click", () => { navigator.clipboard?.writeText((state.logs.get(i.id) || []).map((l) => l.text).join("\n")).then(() => toast("Log copied")).catch(() => {}); });
  const clear = el("button", { textContent: "Clear" });
  clear.addEventListener("click", async () => { await invoke("clear_log", { id: i.id }).catch(() => {}); state.logs.set(i.id, []); paint(); });
  body.append(el("div", { className: "logpanel" }, el("div", { className: "lh" }, el("h3", { textContent: "Game output" }), el("div", { className: "acts" }, copy, clear)), q, pills, con));
  logEls = { id: i.id, con, paint };
  paint();
}
function appendLog(lines) {
  if (!logEls) return;
  const { con, paint } = logEls, near = con.scrollHeight - con.scrollTop - con.clientHeight < 60;
  const rows = lines.filter(logMatch);
  if (con.querySelector(".none")) return paint();
  con.append(...rows.map(logRow));
  while (con.childElementCount > MAX_ROWS) con.firstElementChild.remove();
  if (near) con.scrollTop = con.scrollHeight;
}

// ---------- new version ----------
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
    for (const m of mods) { try { await invoke("enable_module", { id: created.id, module: m }); } catch (e) { notes.push(String(e)); } }
    end(); await refresh();
  }
  if (notes.length) toast(notes[0], true);
  state.selected = created.id;
  openInstance(created.id, "modules");
});

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
