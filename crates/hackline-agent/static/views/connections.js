// Connections (recent bridge events) view.

import { el, card, helpTip } from "../lib/dom.js";
import { getJSON } from "../lib/api.js";
import { fmtTs } from "../lib/fmt.js";

export async function renderConnections(view) {
  view.replaceChildren(el("div", { class: "text-secondary" }, "Loading…"));
  const c = await getJSON("/api/v1/connections");
  const help = helpTip("Each time someone opens a tunnel to this agent, a bridge event is recorded here. 'ok' means the local port was reachable and data flowed. Errors usually mean the local service is not running on that port. Use this to debug tunnel failures.");

  if (!c.entries.length) {
    view.replaceChildren(help, card("Recent bridge events",
      el("div", { class: "empty" }, "No bridge events recorded yet.")));
    return;
  }
  const tbody = el("tbody");
  for (const ev of c.entries) {
    const outcomeClass = ev.outcome === "ok" ? "outcome-ok" : "outcome-error";
    tbody.appendChild(el("tr", {},
      el("td", { class: "mono" }, fmtTs(ev.at_unix)),
      el("td", { class: "mono" }, String(ev.port)),
      el("td", { class: `mono ${outcomeClass}` }, ev.outcome),
      el("td", { class: "mono text-secondary" }, ev.request_id),
    ));
  }
  const table = el("table", { class: "table table-sm table-striped events mono" },
    el("thead", {}, el("tr", {},
      el("th", {}, "time (utc)"), el("th", {}, "port"),
      el("th", {}, "outcome"), el("th", {}, "selector"))),
    tbody,
  );
  view.replaceChildren(help, card("Recent bridge events (newest first)", table));
}
