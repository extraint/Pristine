// Boot.
window.addEventListener("keydown", (e) => { if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "n") { e.preventDefault(); openNew(); } });
(async () => {
  try { state.modules = await invoke("list_modules"); } catch {}
  paintNotifs();
  await Promise.allSettled([loadAccounts(), refresh()]);
  go("home");
})();
