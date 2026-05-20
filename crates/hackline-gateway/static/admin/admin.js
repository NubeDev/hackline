// Single-page admin for hackline-gateway. Pure vanilla; no build step.
// Talks to the existing REST + SSE surface — the bundle adds zero new
// wire to the gateway (SCOPE.md §13 Phase 3).

const TOKEN_KEY = "hackline.admin.token";
let token = localStorage.getItem(TOKEN_KEY) || "";
let eventsSource = null;

function $(id) { return document.getElementById(id); }

function setStatus(msg) { $("status").textContent = msg; }

function authHeader() {
  return token ? { "Authorization": "Bearer " + token } : {};
}

async function api(path) {
  const res = await fetch(path, { headers: authHeader() });
  if (!res.ok) throw new Error(path + ": " + res.status);
  const ct = res.headers.get("content-type") || "";
  return ct.includes("json") ? res.json() : res.text();
}

function tableFromRows(rows, columns) {
  if (!rows || rows.length === 0) return "<p>(no rows)</p>";
  const head = "<tr>" + columns.map(c => "<th>" + c + "</th>").join("") + "</tr>";
  const body = rows.map(r =>
    "<tr>" + columns.map(c => "<td>" + format(r[c]) + "</td>").join("") + "</tr>"
  ).join("");
  return "<table>" + head + body + "</table>";
}

function format(v) {
  if (v === null || v === undefined) return "";
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}

async function loadDevices() {
  try {
    const rows = await api("/v1/devices");
    $("devices-out").innerHTML = tableFromRows(
      rows,
      ["id", "zid", "label", "customer_id", "last_seen_at", "created_at"],
    );
  } catch (e) { $("devices-out").textContent = e.message; }
}

async function loadTunnels() {
  try {
    const rows = await api("/v1/audit?limit=200");
    const sessions = rows.filter(r => r.action === "tunnel.session");
    $("tunnels-out").innerHTML = tableFromRows(
      sessions,
      ["id", "device_id", "tunnel_id", "request_id", "peer", "ts", "ts_close", "bytes_up", "bytes_down"],
    );
  } catch (e) { $("tunnels-out").textContent = e.message; }
}

async function loadCmd() {
  const dev = $("cmd-device").value;
  const status = $("cmd-status").value;
  if (!dev) { $("cmd-out").textContent = "device id required"; return; }
  try {
    const q = status ? "?status=" + encodeURIComponent(status) : "";
    const data = await api("/v1/devices/" + encodeURIComponent(dev) + "/cmd" + q);
    const rows = (data && data.entries) || data;
    $("cmd-out").innerHTML = tableFromRows(
      rows,
      ["cmd_id", "topic", "status", "enqueued_at", "delivered_at", "ack_at", "ack_result", "attempts"],
    );
  } catch (e) { $("cmd-out").textContent = e.message; }
}

async function loadAudit() {
  try {
    const rows = await api("/v1/audit?limit=200");
    $("audit-out").innerHTML = tableFromRows(
      rows,
      ["id", "ts", "action", "user_id", "device_id", "tunnel_id", "request_id", "peer", "bytes_up", "bytes_down", "detail"],
    );
  } catch (e) { $("audit-out").textContent = e.message; }
}

async function loadMetrics() {
  try {
    const text = await api("/metrics");
    $("metrics-out").textContent = text;
  } catch (e) { $("metrics-out").textContent = e.message; }
}

function startEvents() {
  const dev = $("events-device").value;
  if (!dev) return;
  stopEvents();
  // SSE in browsers can't send an Authorization header, so the token
  // rides as a query param. The server already accepts ?token=… on
  // SSE endpoints; if the operator's proxy strips query strings,
  // they should switch to a cookie at the reverse-proxy layer.
  const url = "/v1/devices/" + encodeURIComponent(dev) + "/msg/events/stream"
            + (token ? "?token=" + encodeURIComponent(token) : "");
  eventsSource = new EventSource(url);
  eventsSource.onmessage = (e) => {
    $("events-out").textContent = e.data + "\n" + $("events-out").textContent;
  };
  eventsSource.onerror = () => { setStatus("events stream error"); };
  $("events-start").disabled = true;
  $("events-stop").disabled = false;
}

function stopEvents() {
  if (eventsSource) { eventsSource.close(); eventsSource = null; }
  $("events-start").disabled = false;
  $("events-stop").disabled = true;
}

function activateTab(name) {
  document.querySelectorAll("nav button").forEach(b =>
    b.classList.toggle("active", b.dataset.tab === name));
  document.querySelectorAll(".tab").forEach(s =>
    s.classList.toggle("active", s.id === "tab-" + name));
  if (name === "devices") loadDevices();
  if (name === "tunnels") loadTunnels();
  if (name === "audit") loadAudit();
  if (name === "metrics") loadMetrics();
}

document.addEventListener("DOMContentLoaded", () => {
  $("token").value = token;
  $("save-token").addEventListener("click", () => {
    token = $("token").value.trim();
    localStorage.setItem(TOKEN_KEY, token);
    setStatus("token saved");
    activateTab("devices");
  });
  document.querySelectorAll("nav button").forEach(b => {
    b.addEventListener("click", () => activateTab(b.dataset.tab));
  });
  $("cmd-refresh").addEventListener("click", loadCmd);
  $("events-start").addEventListener("click", startEvents);
  $("events-stop").addEventListener("click", stopEvents);
  activateTab("devices");
});
