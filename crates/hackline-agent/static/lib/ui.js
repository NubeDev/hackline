// Shared UI components: toast notifications and connection badge.

import { el } from "./dom.js";

let toastHost;
let connBadge;

export function initUI() {
  toastHost = document.getElementById("toast-host");
  connBadge = document.getElementById("conn-badge");
}

export function toast(msg, kind = "info") {
  const cls =
    kind === "error" ? "badge-danger" :
    kind === "ok" ? "badge-success" :
    "badge-secondary";
  const t = el("div", { class: `toast ${cls}` },
    el("span", {}, msg),
    el("button", {
      type: "button",
      class: "btn-close",
      onclick: () => t.remove(),
    }, "×"),
  );
  toastHost.appendChild(t);
  setTimeout(() => t.remove(), 4000);
}

export function setConnBadge(info) {
  if (!connBadge) return;
  if (!info) {
    connBadge.textContent = "…";
    connBadge.className = "badge badge-secondary";
    return;
  }
  if (info.gateway.connected) {
    connBadge.textContent = `connected · ${info.gateway.peer_count} peer${info.gateway.peer_count === 1 ? "" : "s"}`;
    connBadge.className = "badge badge-success";
  } else {
    connBadge.textContent = "disconnected";
    connBadge.className = "badge badge-danger";
  }
}
