// Wardrobe: a local skin library with a 3D preview and "apply to my account".
const wd = { skins: [], sel: 0, view: null, samples: null };
const SAMPLE_NAMES = ["Sky", "Ember", "Moss", "Dusk"];
const b64 = (url) => url.slice(url.indexOf(",") + 1);

async function loadSkins() {
  wd.samples ||= SAMPLE_NAMES.map((n) => ({ name: n, url: sampleSkin("skin-" + n), builtin: true }));
  const saved = await invoke("list_skins").catch(() => []);
  wd.skins = [...saved.map((s) => ({ name: s.name, file: s.file, url: "data:image/png;base64," + s.data })), ...wd.samples];
  wd.sel = Math.min(wd.sel, wd.skins.length - 1);
}

async function renderWardrobe() {
  await loadSkins();
  if (state.view !== "wardrobe") return;
  wd.view ||= skin3d($("stage3d"));
  paintSkins();
  selectSkin(wd.sel);
}

function paintSkins() {
  $("skin-grid").replaceChildren(...wd.skins.map((s, idx) => {
    const c = el("button", { className: "scard" + (idx === wd.sel ? " sel" : "") },
      el("div", { className: "th" }, skinThumb(s.url)),
      el("div", { className: "meta" }, el("strong", { textContent: s.name }), el("span", { className: "tag" + (s.builtin ? "" : " accent"), textContent: s.builtin ? "Sample" : "Saved" })));
    c.addEventListener("click", () => selectSkin(idx));
    return c;
  }));
}

function selectSkin(idx) {
  if (!wd.skins.length) return;
  wd.sel = (idx + wd.skins.length) % wd.skins.length;
  const s = wd.skins[wd.sel];
  document.querySelectorAll(".scard").forEach((c, k) => c.classList.toggle("sel", k === wd.sel));
  wd.view.set(s.url, $("skin-slim").checked);
  $("skin-name").textContent = s.name;
  $("skin-delete").disabled = !!s.builtin;
  const online = !!state.acct.selected;
  $("skin-apply").disabled = !online;
  $("skin-hint").textContent = online ? "Applies to your selected Microsoft account." : "Sign in with a Microsoft account (top right) to apply a skin.";
}
$("skin-prev").addEventListener("click", () => selectSkin(wd.sel - 1));
$("skin-next").addEventListener("click", () => selectSkin(wd.sel + 1));
$("skin-slim").addEventListener("change", () => selectSkin(wd.sel));

$("skin-import").addEventListener("click", () => $("skin-file").click());
$("skin-file").addEventListener("change", async (e) => {
  const f = e.target.files[0]; e.target.value = "";
  if (!f) return;
  const url = await new Promise((res) => { const r = new FileReader(); r.onload = () => res(r.result); r.readAsDataURL(f); });
  try {
    const file = await invoke("save_skin", { name: f.name.replace(/\.png$/i, ""), data: b64(url) });
    await loadSkins(); wd.sel = Math.max(0, wd.skins.findIndex((s) => s.file === file));
    paintSkins(); selectSkin(wd.sel); toast("Skin imported");
  } catch (err) { toast(String(err), true); }
});

$("skin-delete").addEventListener("click", async () => {
  const s = wd.skins[wd.sel];
  if (!s || s.builtin || !confirm(`Delete "${s.name}"?`)) return;
  try { await invoke("delete_skin", { file: s.file }); await loadSkins(); paintSkins(); selectSkin(0); } catch (e) { toast(String(e), true); }
});

$("skin-apply").addEventListener("click", async () => {
  const s = wd.skins[wd.sel];
  const end = busy("Applying skin…");
  try {
    const file = s.file || (await invoke("save_skin", { name: s.name, data: b64(s.url) }));
    await invoke("apply_skin", { file, slim: $("skin-slim").checked });
    toast(`Applied "${s.name}" to your account`);
    await loadSkins(); paintSkins();
  } catch (e) { toast(String(e), true); }
  finally { end(); }
});
