// hackline-agent diag UI — vanilla ES module, no build step.
// Mirrors the rubixd UI pattern: hash router, fetch JSON, render
// Bootstrap markup. Read-only by design (SCOPE.md §3.6).

const view = document.getElementById("view");
const badge = document.getElementById("zid-badge");

async function getJSON(path) {
  const res = await fetch(path, { headers: { accept: "application/json" } });
  if (!res.ok) throw new Error(`${path}: ${res.status} ${res.statusText}`);
  return res.json();
}

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else if (k === "html") node.innerHTML = v;
    else node.setAttribute(k, v);
  }
  for (const c of children) {
    if (c == null) continue;
    node.appendChild(typeof c === "string" ? document.createTextNode(c) : c);
  }
  return node;
}

function card(title, body) {
  return el(
    "section",
    { class: "card card-section" },
    el("div", { class: "card-header" }, title),
    el("div", { class: "card-body" }, body),
  );
}

function kv(pairs) {
  const dl = el("dl", { class: "kv mono" });
  for (const [k, v] of pairs) {
    dl.appendChild(el("dt", {}, k));
    const dd = el("dd", {});
    if (v === null || v === undefined || v === "") {
      dd.appendChild(el("span", { class: "empty" }, "—"));
    } else if (Array.isArray(v)) {
      dd.textContent = v.length ? v.join(", ") : "";
      if (!v.length) {
        dd.innerHTML = "";
        dd.appendChild(el("span", { class: "empty" }, "—"));
      }
    } else {
      dd.textContent = String(v);
    }
    dl.appendChild(dd);
  }
  return dl;
}

function fmtUptime(s) {
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (d) return `${d}d ${h}h ${m}m`;
  if (h) return `${h}h ${m}m ${sec}s`;
  if (m) return `${m}m ${sec}s`;
  return `${sec}s`;
}

function fmtTs(unix) {
  const d = new Date(unix * 1000);
  return d.toISOString().replace("T", " ").replace(/\..+/, "");
}

async function renderStatus() {
  view.replaceChildren(el("div", { class: "text-secondary" }, "Loading…"));
  const info = await getJSON("/api/v1/info");
  badge.textContent = info.zid;
  view.replaceChildren(
    card(
      "Identity",
      kv([
        ["zid", info.zid],
        ["label", info.label],
        ["org", info.org],
        ["version", info.version],
        ["uptime", fmtUptime(info.uptime_s)],
      ]),
    ),
    card(
      "Allowed ports",
      kv([["tcp", info.allowed_ports]]),
    ),
  );
}

async function renderZenoh() {
  view.replaceChildren(el("div", { class: "text-secondary" }, "Loading…"));
  const z = await getJSON("/api/v1/zenoh");
  view.replaceChildren(
    card(
      "Session",
      kv([
        ["session zid", z.session_zid],
        ["mode", z.mode],
      ]),
    ),
    card("Listen endpoints", kv([["endpoints", z.listen]])),
    card("Connect endpoints", kv([["endpoints", z.connect]])),
  );
}

async function renderConnections() {
  view.replaceChildren(el("div", { class: "text-secondary" }, "Loading…"));
  const c = await getJSON("/api/v1/connections");
  if (!c.entries.length) {
    view.replaceChildren(
      card(
        "Recent bridge events",
        el("div", { class: "empty" }, "No bridge events recorded yet."),
      ),
    );
    return;
  }
  const tbody = el("tbody");
  for (const ev of c.entries) {
    const outcomeClass = ev.outcome === "ok" ? "outcome-ok" : "outcome-error";
    tbody.appendChild(
      el(
        "tr",
        {},
        el("td", { class: "mono" }, fmtTs(ev.at_unix)),
        el("td", { class: "mono" }, String(ev.port)),
        el("td", { class: `mono ${outcomeClass}` }, ev.outcome),
        el("td", { class: "mono text-secondary" }, ev.request_id),
      ),
    );
  }
  const table = el(
    "table",
    { class: "table table-sm table-striped events mono" },
    el(
      "thead",
      {},
      el(
        "tr",
        {},
        el("th", {}, "time (utc)"),
        el("th", {}, "port"),
        el("th", {}, "outcome"),
        el("th", {}, "selector"),
      ),
    ),
    tbody,
  );
  view.replaceChildren(card("Recent bridge events (newest first)", table));
}

const routes = {
  "/": renderStatus,
  "/zenoh": renderZenoh,
  "/connections": renderConnections,
};

async function route() {
  const hash = location.hash.replace(/^#/, "") || "/";
  // Highlight the active nav link.
  for (const a of document.querySelectorAll("a[data-route]")) {
    a.classList.toggle("active", a.dataset.route === hash);
  }
  const handler = routes[hash] ?? renderStatus;
  try {
    await handler();
  } catch (e) {
    view.replaceChildren(
      el(
        "div",
        { class: "alert alert-danger mono" },
        `Failed to load: ${e.message ?? e}`,
      ),
    );
  }
}

window.addEventListener("hashchange", route);
route();
