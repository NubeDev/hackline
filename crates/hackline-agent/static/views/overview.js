// Overview (landing) view — hero connection card + exposed ports + about.

import { el, card, kv, helpTip } from "../lib/dom.js";
import { getJSON } from "../lib/api.js";
import { fmtUptime } from "../lib/fmt.js";
import { setConnBadge } from "../lib/ui.js";

export async function renderOverview(view, zidBadge) {
  view.replaceChildren(el("div", { class: "text-secondary" }, "Loading…"));
  const info = await getJSON("/api/v1/info");
  zidBadge.textContent = info.zid;
  setConnBadge(info);

  const heroClass = info.gateway.connected ? "border-success" : "border-danger";
  const heroBadge = info.gateway.connected
    ? el("span", { class: "badge badge-success" }, "connected")
    : el("span", { class: "badge badge-danger" }, "disconnected");

  const hero = card(
    null,
    el("div", { class: "hero" },
      el("div", { class: "hero-top" },
        el("h1", { class: "h3 mb-0" }, info.label || info.zid),
        heroBadge,
      ),
      el("p", { class: "text-secondary mb-3 mono small" },
        info.label ? `${info.zid} · org ${info.org}` : `org ${info.org}`),
      kv([
        ["gateway", info.gateway.configured.length ? info.gateway.configured.join(", ") : "(none configured — peer mode)"],
        ["peers", info.gateway.peer_count],
        ["uptime", fmtUptime(info.uptime_s)],
        ["version", info.version],
      ]),
    ),
    heroClass,
  );

  const portsBody = info.ports.length
    ? el("div", { class: "port-chips" },
        ...info.ports.map((p) => el("span", {
          class: `badge ${p.from_config ? "badge-primary" : "badge-warning"} mono`,
          title: p.from_config ? "from agent.toml" : "added at runtime (lost on restart)",
        }, String(p.port))))
    : el("div", { class: "empty" }, "No ports exposed. Open the ports tab to add one.");

  const portsCard = card("Exposed TCP ports", portsBody);

  const about = card(
    "What this agent does",
    el("div", {},
      el("p", { class: "mb-2" },
        "This agent bridges local TCP services to the hackline gateway over Zenoh. Remote users reach your services through tunnels created in the gateway UI — no port forwarding or VPN required."),
      el("p", { class: "mb-0 small text-secondary" },
        "Each ", el("code", {}, "allowed_ports"), " entry is published as a Zenoh queryable at ",
        el("code", {}, `hackline/${info.org}/${info.zid}/tcp/<port>/connect`),
        ". When the gateway opens a tunnel, it queries that key, the agent dials ",
        el("code", {}, "127.0.0.1:<port>"), ", and bytes flow both ways."),
    ),
  );

  const help = helpTip("This is the agent overview. Green badge = connected to gateway, red = disconnected. Check the exposed ports list to confirm your services are registered. If disconnected, verify the gateway URL in your agent.toml and that the gateway is running.");

  view.replaceChildren(help, hero, portsCard, about);
}
