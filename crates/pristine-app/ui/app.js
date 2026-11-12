const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const state = { versions: [], latest: "", busy: new Set(), running: new Set(), cards: new Map() };

function toast(msg, bad = false) {
  const t = $("toast");
  t.textContent = msg;
  t.className = "toast" + (bad ? " bad" : "");
  t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => (t.hidden = true), 5000);
}

// ---- username (per-viewer convenience; works without storage) ----
const usernameEl = $("username");
try { usernameEl.value = localStorage.getItem("username") || "Player"; } catch { usernameEl.value = "Player"; }
usernameEl.addEventListener("input", () => { try { localStorage.setItem("username", usernameEl.value); } catch {} });

// ---- navigation ----
document.querySelectorAll(".nav").forEach((b) => b.addEventListener("click", () => {
  document.querySelectorAll(".nav").forEach((n) => n.classList.toggle("active", n === b));
  $("view-instances").hidden = b.dataset.view !== "instances";
  $("view-settings").hidden = b.dataset.view !== "settings";
}));

// ---- instances ----
function el(tag, props = {}, ...kids) {
  const e = Object.assign(document.createElement(tag), props);
  kids.forEach((k) => e.append(k));
  return e;
}

function renderCard(inst) {
  const bar = el("i");
  const status = el("div", { className: "status" });
  const play = el("button", { className: "primary", textContent: inst.installed ? "Play" : "Install & play" });
  const del = el("button", { textContent: "Delete", title: "Delete instance" });

  const busy = () => state.busy.has(inst.id);
  const sync = () => {
    play.disabled = busy() || state.running.has(inst.id);
    del.disabled = busy();
    if (state.running.has(inst.id)) { play.textContent = "Running…"; status.textContent = "Game is running"; }
  };

  play.addEventListener("click", async () => {
    state.busy.add(inst.id); sync();
    status.textContent = "Preparing…";
    try {
      await invoke("launch_instance", { id: inst.id, username: usernameEl.value });
      state.running.add(inst.id);
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

  del.addEventListener("click", async () => {
    if (!confirm(`Delete "${inst.name}" and all its worlds and settings?`)) return;
    try { await invoke("delete_instance", { id: inst.id }); await refresh(); }
    catch (e) { toast(String(e), true); }
  });

  const card = el("div", { className: "card" },
    el("h3", { textContent: inst.name }),
    el("div", { className: "meta" },
      el("span", { className: "pill", textContent: "Minecraft " + inst.mc_version }),
      el("span", { className: "pill" + (inst.installed ? " ok" : ""), textContent: inst.installed ? "Installed" : "Not installed" })),
    el("div", { className: "bar" }, bar),
    status,
    el("div", { className: "row" }, play, del));
  state.cards.set(inst.id, { bar, status, play, sync });
  sync();
  return card;
}

async function refresh() {
  const list = await invoke("list_instances");
  state.cards.clear();
  const grid = $("grid");
  grid.replaceChildren(...list.map(renderCard));
  $("empty").hidden = list.length > 0;
}

// ---- events from the backend ----
listen("progress", ({ payload: p }) => {
  const c = state.cards.get(p.id);
  if (!c) return;
  c.bar.style.width = (p.total ? (p.done / p.total) * 100 : 0) + "%";
  c.status.textContent = `${p.stage}: ${p.done}/${p.total}`;
});
listen("game-exit", ({ payload: p }) => {
  state.running.delete(p.id);
  const c = state.cards.get(p.id);
  if (c) { c.play.textContent = "Play"; c.status.textContent = p.code === 0 || p.code == null ? "" : `Game exited with code ${p.code}`; c.sync(); }
  if (p.code) toast(`Game exited with code ${p.code}`, true);
});

// ---- new instance dialog ----
function fillVersions() {
  const snaps = $("show-snapshots").checked;
  const sel = $("new-version");
  sel.replaceChildren(...state.versions
    .filter((v) => snaps || v.kind === "release")
    .map((v) => el("option", { value: v.id, textContent: v.id + (v.id === state.latest ? " (latest)" : "") })));
}
$("show-snapshots").addEventListener("change", fillVersions);
$("new-btn").addEventListener("click", async () => {
  $("new-error").hidden = true;
  $("new-name").value = "";
  $("new-dialog").showModal();
  if (!state.versions.length) {
    $("new-create").disabled = true;
    try {
      const v = await invoke("list_versions");
      state.versions = v.versions; state.latest = v.latest_release;
      fillVersions();
    } catch (e) { $("new-error").textContent = "Couldn't load versions: " + e; $("new-error").hidden = false; }
    $("new-create").disabled = false;
  }
});
$("new-cancel").addEventListener("click", () => $("new-dialog").close());
$("new-form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  try {
    await invoke("create_instance", { name: $("new-name").value, version: $("new-version").value });
    $("new-dialog").close();
    await refresh();
  } catch (e) { $("new-error").textContent = String(e); $("new-error").hidden = false; }
});

refresh().catch((e) => toast(String(e), true));
