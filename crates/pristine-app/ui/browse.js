// Browse Modrinth: mods, textures, shaders, data packs and modpacks.
const browse = { kind: "mod", target: null, page: 0, total: 0, cats: new Set(), owned: new Set(), seq: 0 };
const PAGE = 24;
const KINDS = [["mod", "Mods"], ["resourcepack", "Textures"], ["shader", "Shaders"], ["datapack", "Data packs"], ["modpack", "Modpacks"]];
const MOD_CATS = ["adventure", "decoration", "economy", "equipment", "food", "game-mechanics", "library", "magic", "management", "mobs", "optimization", "social", "storage", "technology", "transportation", "utility", "worldgen"];
const CATS = {
  mod: MOD_CATS,
  resourcepack: ["combat", "cursed", "decoration", "modded", "realistic", "simplistic", "themed", "tweaks", "utility", "vanilla-like"],
  shader: ["cartoon", "cursed", "fantasy", "realistic", "semi-realistic", "vanilla-like"],
  datapack: ["adventure", "decoration", "equipment", "food", "game-mechanics", "library", "magic", "management", "mobs", "optimization", "storage", "technology", "transportation", "utility", "worldgen"],
  modpack: ["adventure", "challenging", "combat", "kitchen-sink", "lightweight", "magic", "multiplayer", "optimization", "quests", "technology"],
};

const bundledSlugs = () => new Set(state.modules.flatMap((m) => m.projects));

async function renderBrowse() {
  const b = browse, label = KINDS.find((k) => k[0] === b.kind)[1];
  $("b-title").textContent = "Browse " + label;
  const cur = b.target && inst(b.target) ? b.target : inst(state.selected) ? state.selected : state.instances[0]?.id;
  b.target = cur || null;
  const target = $("b-target");
  target.replaceChildren(...state.instances.map((i) => el("option", { value: i.id, textContent: `${vname(i)} · ${i.name}`, selected: i.id === cur })));
  target.hidden = b.kind === "modpack" || !state.instances.length;

  $("b-tabs").replaceChildren(...KINDS.map(([k, name]) => {
    const btn = el("button", { textContent: name, className: b.kind === k ? "on" : "" });
    btn.addEventListener("click", () => { b.kind = k; b.page = 0; b.cats.clear(); renderBrowse(); });
    return btn;
  }));
  $("b-cats").replaceChildren(el("h4", { textContent: "Categories" }), ...CATS[b.kind].map((c) => {
    const btn = el("button", { textContent: c.replace(/-/g, " "), className: b.cats.has(c) ? "on" : "" });
    btn.addEventListener("click", () => { b.cats.has(c) ? b.cats.delete(c) : b.cats.add(c); b.page = 0; renderBrowse(); });
    return btn;
  }));
  b.owned = new Set();
  if (b.target && b.kind !== "modpack") {
    const items = await invoke("list_content", { id: b.target }).catch(() => []);
    b.owned = new Set(items.map((c) => c.project_id).filter(Boolean));
  }
  if (state.view === "browse") runSearch();
}
$("b-target").addEventListener("change", (e) => { browse.target = e.target.value; state.selected = e.target.value; browse.page = 0; renderBrowse(); });
$("b-sort").addEventListener("change", () => { browse.page = 0; runSearch(); });
let bTimer;
$("b-q").addEventListener("input", () => { clearTimeout(bTimer); bTimer = setTimeout(() => { browse.page = 0; runSearch(); }, 300); });

async function runSearch() {
  const b = browse, mine = ++b.seq, i = b.target ? inst(b.target) : null, compat = i && b.kind !== "modpack";
  $("b-grid").replaceChildren(el("p", { className: "muted center", textContent: "Searching…", style: "grid-column:1/-1" }));
  let r;
  try {
    r = await invoke("search_content", { params: {
      query: $("b-q").value.trim(), kind: b.kind, offset: b.page * PAGE, index: $("b-sort").value, categories: [...b.cats],
      mc_version: compat ? i.mc_version : null, loader: compat && b.kind === "mod" && i.loader !== "vanilla" ? i.loader : null,
    } });
  } catch (e) { if (mine === b.seq) $("b-grid").replaceChildren(el("p", { className: "muted center", textContent: "Couldn't reach Modrinth: " + e, style: "grid-column:1/-1" })); return; }
  if (mine !== b.seq) return;
  b.total = r.total;
  $("b-grid").replaceChildren(...(r.hits.length ? r.hits.map(mcard) : [el("p", { className: "muted center", textContent: "Nothing found.", style: "grid-column:1/-1" })]));
  renderPager();
}

function mcard(h) {
  const b = browse, bundled = bundledSlugs().has(h.slug), owned = b.owned.has(h.project_id);
  const tag = owned ? el("span", { className: "tag ok", textContent: "Installed" }) : bundled ? el("span", { className: "tag accent", textContent: "Bundled" }) : el("span", { className: "tag", textContent: (h.categories[0] || "").replace(/-/g, " ") });
  const btn = el("button", { className: "primary", textContent: b.kind === "modpack" ? "Install" : owned ? "Added" : "Add", disabled: owned });
  btn.addEventListener("click", () => (b.kind === "modpack" ? installModpack(h, btn) : installHit(h, btn, tag)));
  return el("div", { className: "mcard" },
    el("div", { className: "top1" }, projectIcon(h.icon_url, h.title),
      el("div", { className: "nm" }, el("strong", { textContent: h.title, title: h.title }), el("div", { className: "by", textContent: `${h.author} · ${compact.format(h.downloads)} downloads` }))),
    el("p", { textContent: h.description }), el("div", { className: "bottom" }, tag, btn));
}

function renderPager() {
  const b = browse, pages = Math.max(1, Math.ceil(b.total / PAGE)), cur = b.page, pager = $("b-pager");
  if (pages <= 1) return pager.replaceChildren();
  const go1 = (p) => { b.page = p; runSearch(); $("main").scrollTop = 0; };
  const mk = (label, p, on = false, dis = false) => { const x = el("button", { textContent: label, className: on ? "on" : "", disabled: dis }); x.addEventListener("click", () => go1(p)); return x; };
  const nums = new Set([0, pages - 1, cur - 1, cur, cur + 1].filter((n) => n >= 0 && n < pages));
  const out = [mk("‹", cur - 1, false, cur === 0)];
  let prev = -1;
  [...nums].sort((a, c) => a - c).forEach((n) => { if (n - prev > 1) out.push(el("span", { className: "gap", textContent: "…" })); out.push(mk(String(n + 1), n, n === cur)); prev = n; });
  out.push(mk("›", cur + 1, false, cur >= pages - 1));
  pager.replaceChildren(...out);
}

function pickWorld(id) {
  return new Promise(async (resolve) => {
    const worlds = await invoke("list_worlds", { id }).catch(() => []);
    if (!worlds.length) { toast("This version has no worlds yet. Launch the game and create one first.", true); return resolve(null); }
    $("world-select").replaceChildren(...worlds.map((w) => el("option", { value: w, textContent: w })));
    const d = $("world-dialog"), done = (v) => { d.close(); resolve(v); };
    $("world-ok").onclick = () => done($("world-select").value);
    $("world-cancel").onclick = () => done(null);
    d.addEventListener("cancel", () => resolve(null), { once: true });
    d.showModal();
  });
}

async function installHit(h, btn, tag) {
  const id = browse.target, kind = browse.kind;
  if (!id) return toast("Create a version first", true);
  let world = null;
  if (kind === "datapack") { world = await pickWorld(id); if (!world) return; }
  btn.disabled = true; btn.textContent = "Adding…";
  try {
    const names = await invoke("install_content", { id, project: h.slug, kind, world });
    btn.textContent = "Added"; browse.owned.add(h.project_id);
    tag.className = "tag ok"; tag.textContent = "Installed";
    toast(names.length ? `Added ${names.join(", ")} to ${inst(id).name}` : `${h.title} is already in ${inst(id).name}`);
  } catch (e) { toast(String(e), true); btn.disabled = false; btn.textContent = "Add"; }
}
