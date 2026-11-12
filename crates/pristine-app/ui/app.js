const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const state = {
  instances: [], versions: [], latest: "", kind: "release", query: "",
  busy: new Set(), running: new Set(), ctl: new Map(), editing: null,
};

if (/Mac/i.test(navigator.platform) || /Mac OS X/.test(navigator.userAgent)) document.documentElement.classList.add("mac");

function el(tag, props = {}, ...kids) {
  const e = Object.assign(document.createElement(tag), props);
  kids.forEach((k) => k != null && e.append(k));
  return e;
}

function toast(msg, bad = false) {
  const t = $("toast");
  t.textContent = msg;
  t.className = "toast" + (bad ? " bad" : "");
  t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => (t.hidden = true), 5000);
}

function hash(s) { let h = 5381; for (const c of s) h = ((h << 5) + h + c.charCodeAt(0)) | 0; return Math.abs(h); }
function gradient(seed) {
  const h = hash(seed) % 360;
  return `linear-gradient(135deg, hsl(${h} 72% 46%), hsl(${(h + 55) % 360} 70% 34%))`;
}
function ago(secs) {
  if (!secs) return "Never played";
  const d = secs - Date.now() / 1000;
  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  const units = [["day", 86400], ["hour", 3600], ["minute", 60]];
  for (const [u, s] of units) if (Math.abs(d) >= s) return "Played " + rtf.format(Math.round(d / s), u);
  return "Played just now";
}
const gb = (mb) => (mb / 1024).toFixed(mb % 1024 ? 1 : 0) + " GB";

// ---------- accounts ----------
let offlineName = "Player";
try { offlineName = localStorage.getItem("username") || "Player"; } catch {}
state.acct = { configured: false, selected: null, accounts: [] };

function currentAccount() {
  const sel = state.acct.accounts.find((x) => x.uuid === state.acct.selected);
  return sel ? { name: sel.username, kind: "Microsoft account", seed: sel.uuid } : { name: offlineName.trim() || "Player", kind: "Offline account", seed: offlineName };
}
function avatar(name, seed, node = el("div", { className: "avatar" })) {
  node.textContent = (name.trim()[0] || "P").toUpperCase();
  node.style.background = gradient(seed || name);
  return node;
}
function paintAccount() {
  const a = currentAccount();
  $("acct-name").textContent = a.name;
  $("acct-kind").textContent = a.kind;
  avatar(a.name, a.seed, $("avatar"));
}
async function loadAccounts() {
  state.acct = await invoke("accounts_state");
  paintAccount();
}

function renderAccountList() {
  const { accounts, selected } = state.acct;
  const rows = [];
  const off = el("input", { value: offlineName, maxLength: 16, spellcheck: false, ariaLabel: "Offline username" });
  off.addEventListener("input", () => { offlineName = off.value; try { localStorage.setItem("username", off.value); } catch {} paintAccount(); });
  off.addEventListener("click", (e) => e.stopPropagation());
  const offRadio = el("input", { type: "radio", name: "acct", checked: !selected });
  const offRow = el("label", { className: "acct" + (!selected ? " on" : "") }, offRadio,
    avatar(offlineName, offlineName), el("div", { className: "who" }, off, el("small", { textContent: "Offline · can't join online servers" })));
  offRadio.addEventListener("change", async () => { await invoke("select_account", { uuid: null }); await loadAccounts(); renderAccountList(); });
  rows.push(offRow);
  for (const a of accounts) {
    const radio = el("input", { type: "radio", name: "acct", checked: selected === a.uuid });
    const rm = el("button", { className: "x", type: "button", textContent: "Remove", title: "Sign out of this account" });
    rm.addEventListener("click", async (e) => {
      e.preventDefault(); e.stopPropagation();
      await invoke("remove_account", { uuid: a.uuid }); await loadAccounts(); renderAccountList();
    });
    radio.addEventListener("change", async () => { await invoke("select_account", { uuid: a.uuid }); await loadAccounts(); renderAccountList(); });
    rows.push(el("label", { className: "acct" + (selected === a.uuid ? " on" : "") }, radio,
      avatar(a.username, a.uuid), el("div", { className: "who" }, el("strong", { textContent: a.username }), el("small", { textContent: "Microsoft account" })), rm));
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
  if (!state.acct.configured) {
    $("accounts-dialog").close();
    showView("settings");
    return toast("Add your Azure client ID first to enable Microsoft sign-in", true);
  }
  try {
    const dc = await invoke("ms_login_start");
    state.dc = dc;
    $("code").textContent = dc.user_code;
    $("signin-status").textContent = "Waiting for you to approve in your browser…";
    $("accounts-dialog").close();
    $("signin-dialog").showModal();
    openUrl(dc.verification_uri);
  } catch (e) { toast(String(e), true); }
});
$("open-link").addEventListener("click", () => state.dc && openUrl(state.dc.verification_uri));
$("copy-code").addEventListener("click", () => { navigator.clipboard?.writeText($("code").textContent).then(() => toast("Code copied")).catch(() => {}); });
$("signin-cancel").addEventListener("click", async () => { await invoke("ms_login_cancel"); $("signin-dialog").close(); });
$("signin-dialog").addEventListener("cancel", () => invoke("ms_login_cancel"));
listen("ms-login", async ({ payload: p }) => {
  if (p.ok) {
    $("signin-dialog").close();
    await loadAccounts();
    toast(`Signed in as ${p.username}`);
  } else {
    $("signin-status").textContent = p.error || "Sign-in failed";
    toast(p.error || "Sign-in failed", true);
  }
});

// ---------- navigation ----------
function showView(v) {
  document.querySelectorAll(".nav").forEach((n) => n.classList.toggle("active", n.dataset.view === v));
  $("view-library").hidden = v !== "library";
  $("view-settings").hidden = v !== "settings";
}
document.querySelectorAll(".nav").forEach((b) => b.addEventListener("click", () => showView(b.dataset.view)));

// ---------- play controller shared by hero + cards ----------
function controller(inst, { bar, status, play }) {
  const sync = () => {
    const running = state.running.has(inst.id), busy = state.busy.has(inst.id);
    play.disabled = busy || running;
    play.textContent = running ? "Running…" : busy ? "Working…" : inst.installed ? "Play" : "Install & play";
    if (running) status.textContent = "Game is running";
  };
  play.addEventListener("click", async () => {
    state.busy.add(inst.id); sync();
    status.textContent = "Preparing…";
    try {
      await invoke("launch_instance", { id: inst.id, username: offlineName });
      state.running.add(inst.id);
      inst.installed = true;
      status.textContent = "Game is running";
    } catch (e) {
      toast(String(e), true);
      status.textContent = "";
    } finally {
      state.busy.delete(inst.id);
      bar.style.width = "0";
      sync();
    }
  });
  const c = { bar, status, play, sync, inst };
  state.ctl.set(inst.id, c);
  sync();
  return c;
}

function renderHero(inst) {
  const bar = el("i"), status = el("div", { className: "status" });
  const play = el("button", { className: "play" });
  const hero = el("div", { className: "hero" },
    el("div", {},
      el("div", { className: "eyebrow", textContent: "Jump back in" }),
      el("h2", { textContent: inst.name }),
      el("div", { className: "sub", textContent: `Minecraft ${inst.mc_version} · ${ago(inst.last_played)}` })),
    el("div", { className: "cta" }, play, el("div", { className: "bar" }, bar), status));
  hero.style.background = gradient(inst.id);
  controller(inst, { bar, status, play });
  return hero;
}

function renderCard(inst, i) {
  const bar = el("i"), status = el("div", { className: "status" });
  const play = el("button", { className: "primary play" });
  const kebab = el("button", { className: "kebab", textContent: "⋯", title: "Instance settings" });
  kebab.addEventListener("click", () => openEdit(inst));
  const banner = el("div", { className: "banner" },
    el("span", { className: "badge", textContent: inst.installed ? "Installed" : "Not installed" }),
    kebab,
    el("span", { className: "ver" + (inst.mc_version.length > 8 ? " long" : ""), textContent: inst.mc_version }));
  banner.style.background = gradient(inst.id);
  const card = el("div", { className: "card" }, banner,
    el("div", { className: "body" },
      el("div", {},
        el("h3", { textContent: inst.name, title: inst.name }),
        el("div", { className: "meta", textContent: `${gb(inst.max_memory_mb)} · ${ago(inst.last_played)}` })),
      el("div", { className: "bar" }, bar), status, play));
  card.style.animationDelay = Math.min(i, 8) * 40 + "ms";
  controller(inst, { bar, status, play });
  return card;
}

function render() {
  state.ctl.clear();
  const q = state.query.trim().toLowerCase();
  const all = state.instances;
  const list = q ? all.filter((i) => (i.name + " " + i.mc_version).toLowerCase().includes(q)) : all;
  const byRecent = [...all].sort((a, b) => (b.last_played || 0) - (a.last_played || 0));
  const hero = !q && all.length ? byRecent[0] : null;

  $("empty").hidden = all.length > 0;
  $("no-match").hidden = !(all.length && !list.length);
  $("subtitle").textContent = all.length ? `${all.length} instance${all.length === 1 ? "" : "s"}` : "";
  document.querySelector(".tools").hidden = !all.length && !state.query;

  const heroEl = $("hero");
  heroEl.hidden = !hero;
  heroEl.replaceChildren();
  if (hero) { const h = renderHero(hero); heroEl.replaceWith(h); h.id = "hero"; }

  const rest = list.filter((i) => !hero || i.id !== hero.id);
  $("grid").replaceChildren(...rest.map(renderCard));
}

async function refresh() {
  state.instances = await invoke("list_instances");
  render();
}

// ---------- backend events ----------
const STAGES = { "version manifest": "Fetching versions", libraries: "Downloading libraries", assets: "Downloading assets", "java runtime": "Installing Java" };
listen("progress", ({ payload: p }) => {
  const c = state.ctl.get(p.id);
  if (!c) return;
  c.bar.style.width = (p.total ? (p.done / p.total) * 100 : 0) + "%";
  c.status.textContent = `${STAGES[p.stage] || p.stage} · ${p.done}/${p.total}`;
});
listen("game-exit", ({ payload: p }) => {
  state.running.delete(p.id);
  const c = state.ctl.get(p.id);
  if (c) { c.status.textContent = p.code ? `Exited with code ${p.code}` : ""; c.sync(); }
  if (p.code) toast(`Game exited with code ${p.code}`, true);
  refresh().catch(() => {});
});

// ---------- search / shortcuts ----------
$("search").addEventListener("input", (e) => { state.query = e.target.value; render(); });
window.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "n") { e.preventDefault(); openNew(); }
});

// ---------- new instance ----------
function fillVersions() {
  const sel = $("new-version");
  const items = state.versions.filter((v) => v.kind === state.kind);
  sel.replaceChildren(...items.map((v) => el("option", { value: v.id, textContent: v.id + (v.id === state.latest ? "  ·  latest" : "") })));
  if (items.length) sel.selectedIndex = 0;
}
document.querySelectorAll(".seg button").forEach((b) => b.addEventListener("click", () => {
  state.kind = b.dataset.kind;
  document.querySelectorAll(".seg button").forEach((x) => x.classList.toggle("on", x === b));
  fillVersions();
}));
async function openNew() {
  $("new-error").hidden = true;
  $("new-name").value = "";
  if (!$("new-dialog").open) $("new-dialog").showModal();
  $("new-name").focus();
  if (!state.versions.length) {
    $("new-create").disabled = true;
    try {
      const v = await invoke("list_versions");
      state.versions = v.versions; state.latest = v.latest_release;
      fillVersions();
    } catch (e) { $("new-error").textContent = "Couldn't load versions: " + e; $("new-error").hidden = false; }
    $("new-create").disabled = false;
  }
}
$("new-btn").addEventListener("click", openNew);
$("empty-new").addEventListener("click", openNew);
$("new-cancel").addEventListener("click", () => $("new-dialog").close());
$("new-form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const version = $("new-version").value;
  if (!version) { $("new-error").textContent = "Pick a version"; $("new-error").hidden = false; return; }
  try {
    await invoke("create_instance", { name: $("new-name").value, version });
    $("new-dialog").close();
    await refresh();
  } catch (e) { $("new-error").textContent = String(e); $("new-error").hidden = false; }
});

// ---------- edit instance ----------
function openEdit(inst) {
  state.editing = inst;
  $("edit-title").textContent = inst.name;
  $("mem").value = inst.max_memory_mb;
  $("mem-val").textContent = gb(inst.max_memory_mb);
  $("edit-dialog").showModal();
}
$("mem").addEventListener("input", (e) => ($("mem-val").textContent = gb(+e.target.value)));
$("edit-cancel").addEventListener("click", () => $("edit-dialog").close());
$("edit-form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  try {
    await invoke("set_memory", { id: state.editing.id, mb: +$("mem").value });
    $("edit-dialog").close();
    await refresh();
  } catch (e) { toast(String(e), true); }
});
$("edit-delete").addEventListener("click", async () => {
  const inst = state.editing;
  if (state.running.has(inst.id)) return toast("Close the game before deleting this instance", true);
  if (!confirm(`Delete "${inst.name}" and all its worlds and settings? This can't be undone.`)) return;
  try { await invoke("delete_instance", { id: inst.id }); $("edit-dialog").close(); await refresh(); }
  catch (e) { toast(String(e), true); }
});

// ---------- settings ----------
async function loadInfo() {
  const i = await invoke("app_info");
  const names = { macos: "macOS", windows: "Windows", linux: "Linux" };
  const rows = [["Data folder", i.data_dir], ["Platform", `${names[i.os] || i.os} · ${i.arch}`], ["Pristine", "v" + i.version]];
  $("client-id").value = (await invoke("get_client_id")) || "";
  $("info").replaceChildren(...rows.map(([k, v]) => el("div", {}, el("dt", { textContent: k }), el("dd", { textContent: v }))));
}

$("cid-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  try { await invoke("set_client_id", { id: $("client-id").value }); await loadAccounts(); toast("Saved"); }
  catch (err) { toast(String(err), true); }
});

refresh().catch((e) => toast(String(e), true));
loadAccounts().catch(() => {});
loadInfo().catch(() => {});
