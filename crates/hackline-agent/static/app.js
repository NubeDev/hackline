// hackline-agent diag UI — entry point and hash router.
// Views and helpers live in ./views/ and ./lib/ respectively.

import { getJSON } from "./lib/api.js";
import { initUI, setConnBadge } from "./lib/ui.js";
import { renderOverview } from "./views/overview.js";
import { renderPorts } from "./views/ports.js";
import { renderZenoh } from "./views/zenoh.js";
import { renderConnections } from "./views/connections.js";
import { renderSetup } from "./views/setup.js";

const view = document.getElementById("view");
const zidBadge = document.getElementById("zid-badge");

initUI();

const routes = {
  "/": () => renderOverview(view, zidBadge),
  "/ports": () => renderPorts(view),
  "/zenoh": () => renderZenoh(view),
  "/connections": () => renderConnections(view),
  "/setup": () => renderSetup(view),
};

async function route() {
  const hash = location.hash.replace(/^#/, "") || "/";
  for (const a of document.querySelectorAll("a[data-route]")) {
    a.classList.toggle("active", a.dataset.route === hash);
  }
  const handler = routes[hash] ?? routes["/"];
  try {
    await handler();
  } catch (e) {
    const { el } = await import("./lib/dom.js");
    view.replaceChildren(el("div", { class: "alert alert-danger mono" },
      `Failed to load: ${e.message ?? e}`));
  }
}

async function pollBadge() {
  try {
    const info = await getJSON("/api/v1/info");
    if (info.zid) zidBadge.textContent = info.zid;
    setConnBadge(info);
  } catch {
    setConnBadge(null);
  }
}

window.addEventListener("hashchange", route);
route();
pollBadge();
setInterval(pollBadge, 5000);
