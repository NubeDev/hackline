// Ports view — add/remove exposed ports at runtime.

import { el, card, helpTip } from "../lib/dom.js";
import { getJSON, postJSON, del } from "../lib/api.js";
import { toast } from "../lib/ui.js";

export async function renderPorts(view) {
  view.replaceChildren(el("div", { class: "text-secondary" }, "Loading…"));
  const data = await getJSON("/api/v1/ports");

  const input = el("input", {
    type: "number",
    min: "1",
    max: "65535",
    class: "form-control mono",
    placeholder: "e.g. 3000",
    id: "port-input",
  });

  const form = el("form", {
    class: "row g-2 align-items-end mb-3",
    onsubmit: async (ev) => {
      ev.preventDefault();
      const port = parseInt(input.value, 10);
      if (!Number.isFinite(port) || port < 1 || port > 65535) {
        toast("port must be 1..65535", "error");
        return;
      }
      try {
        await postJSON("/api/v1/ports", { port });
        toast(`exposed port ${port}`, "ok");
        input.value = "";
        await renderPorts(view);
      } catch (e) {
        toast(e.message ?? String(e), "error");
      }
    },
  },
    el("div", { class: "col-auto" },
      el("label", { class: "form-label small text-secondary mb-1", for: "port-input" }, "Local TCP port"),
      input,
    ),
    el("div", { class: "col-auto" },
      el("button", { type: "submit", class: "btn btn-primary" }, "Expose"),
    ),
    el("div", { class: "col" },
      el("div", { class: "small text-secondary" },
        "Adds a Zenoh queryable for this port. The change applies immediately but is ",
        el("strong", {}, "not"), " written back to agent.toml — it is lost on restart."),
    ),
  );

  let body;
  if (!data.ports.length) {
    body = el("div", { class: "empty" }, "No ports exposed yet.");
  } else {
    const tbody = el("tbody");
    for (const p of data.ports) {
      const removeBtn = el("button", {
        class: "btn btn-sm btn-outline-danger",
        onclick: async () => {
          if (!confirm(`Stop serving port ${p.port}?`)) return;
          try {
            await del(`/api/v1/ports/${p.port}`);
            toast(`removed port ${p.port}`, "ok");
            await renderPorts(view);
          } catch (e) {
            toast(e.message ?? String(e), "error");
          }
        },
      }, "Remove");
      tbody.appendChild(el("tr", {},
        el("td", { class: "mono" }, String(p.port)),
        el("td", {},
          p.from_config
            ? el("span", { class: "badge badge-primary" }, "from config")
            : el("span", { class: "badge badge-warning" }, "runtime"),
        ),
        el("td", { class: "text-end" }, removeBtn),
      ));
    }
    body = el("table", { class: "table table-sm mono align-middle" },
      el("thead", {}, el("tr", {},
        el("th", {}, "port"), el("th", {}, "source"), el("th", { class: "text-end" }, ""))),
      tbody,
    );
  }

  const help = helpTip("Manage which local TCP ports this agent exposes to the gateway. Enter a port number and click Expose to make it reachable via tunnels. 'from config' ports are set in agent.toml and persist across restarts. 'runtime' ports are temporary and lost when the agent stops.");

  view.replaceChildren(help, card("Add port", form), card("Active ports", body));
}
