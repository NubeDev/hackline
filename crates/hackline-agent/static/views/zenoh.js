// Zenoh session view.

import { el, card, kv, helpTip } from "../lib/dom.js";
import { getJSON } from "../lib/api.js";

export async function renderZenoh(view) {
  view.replaceChildren(el("div", { class: "text-secondary" }, "Loading…"));
  const z = await getJSON("/api/v1/zenoh");
  const help = helpTip("Zenoh is the mesh network layer under hackline. This page shows the agent's Zenoh session. If 'Connected peers' is empty, the agent cannot reach the gateway — check firewall rules and the connect URL in agent.toml.");

  view.replaceChildren(
    help,
    card("Session", kv([["session zid", z.session_zid], ["mode", z.mode]])),
    card("Connected peers", kv([["zids", z.peers]])),
    card("Listen endpoints", kv([["endpoints", z.listen]])),
    card("Connect endpoints", kv([["endpoints", z.connect]])),
  );
}
