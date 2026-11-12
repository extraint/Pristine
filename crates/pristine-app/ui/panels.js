// Floating panels: social and notifications.
function closePanels() { $("social").hidden = true; $("notif").hidden = true; }

function paintSocial() {
  const a = currentAccount();
  $("social-name").textContent = a.name; $("social-avatar").src = pixelAvatar(a.seed);
  $("social-dot").classList.toggle("on", a.online); $("social-status").textContent = a.online ? "Signed in" : "Offline";
  const tab = document.querySelector("#social .tabs button.on")?.dataset.st, seg = document.querySelector("#social-seg button.on")?.dataset.s || "Chats";
  $("social-empty").textContent = tab === "party"
    ? "Parties aren't part of Pristine yet."
    : `No ${seg.toLowerCase()} yet. Friends, chat and groups aren't part of Pristine yet; they'll show up here when they are.`;
}
document.querySelectorAll("#social .tabs button").forEach((b) => b.addEventListener("click", () => {
  document.querySelectorAll("#social .tabs button").forEach((x) => x.classList.toggle("on", x === b)); paintSocial();
}));
document.querySelectorAll("#social-seg button").forEach((b) => b.addEventListener("click", () => {
  document.querySelectorAll("#social-seg button").forEach((x) => x.classList.toggle("on", x === b)); paintSocial();
}));

function paintNotifs() {
  const list = $("notif-list");
  list.replaceChildren(...(state.notes.length
    ? state.notes.map((n) => el("div", { className: "nitem" + (n.bad ? " bad" : "") }, n.msg, el("small", { textContent: n.at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) })))
    : [el("div", { className: "none-note", textContent: "No notifications" })]));
  $("notif-clear").disabled = !state.notes.length;
}
$("notif-clear").addEventListener("click", () => { state.notes = []; state.unread = 0; $("notif-dot").hidden = true; paintNotifs(); });

$("btn-social").addEventListener("click", (e) => { e.stopPropagation(); const open = $("social").hidden; closePanels(); $("social").hidden = !open; paintSocial(); });
$("btn-notif").addEventListener("click", (e) => {
  e.stopPropagation(); const open = $("notif").hidden; closePanels(); $("notif").hidden = !open;
  if (open) { state.unread = 0; $("notif-dot").hidden = true; paintNotifs(); }
});
document.addEventListener("click", (e) => { if (!e.target.closest(".float")) closePanels(); });
window.addEventListener("keydown", (e) => { if (e.key === "Escape") closePanels(); });
