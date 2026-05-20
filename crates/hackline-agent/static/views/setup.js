// Setup recipes view — edge device + PC proxy tabs.

import { el, card, codeBlock, helpTip } from "../lib/dom.js";

export function renderSetup(view) {
  const tabs = el("ul", { class: "nav nav-tabs mb-3", id: "setupTabs", role: "tablist" });
  const content = el("div", { class: "tab-content" });

  const tabSpec = [
    {
      id: "edge",
      label: "Edge device (systemd)",
      body: [
        el("p", {}, "Run the agent as a systemd service on a Raspberry Pi, Rubix gateway, or any Linux box."),
        el("ol", { class: "mb-3" },
          el("li", {}, "Copy the ", el("code", {}, "hackline-agent"), " binary to ", el("code", {}, "/usr/local/bin/"), "."),
          el("li", {}, "Write ", el("code", {}, "/etc/hackline/agent.toml"), " (see template below)."),
          el("li", {}, "Install the unit file and enable: ", el("code", {}, "systemctl enable --now hackline-agent"), "."),
        ),
        codeBlock(`# /etc/hackline/agent.toml
zid   = "device-01"
org   = "default"
label = "rack-A pi-01"
allowed_ports = [22, 8080]

[zenoh]
mode    = "client"
connect = ["tcp/gateway.example.com:7447"]

[diag]
enabled = true
bind    = "127.0.0.1:9999"
`),
        codeBlock(`# /etc/systemd/system/hackline-agent.service
[Unit]
Description=hackline agent
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/hackline-agent /etc/hackline/agent.toml
Restart=always
RestartSec=5
User=hackline

[Install]
WantedBy=multi-user.target
`),
      ],
    },
    {
      id: "pc",
      label: "PC proxy (quick start)",
      body: [
        el("p", {}, "Expose local dev services (", el("code", {}, "localhost:3000"), ", etc.) through a hackline gateway — self-hosted ngrok over Zenoh."),
        el("ol", { class: "mb-3" },
          el("li", {}, "Download or build ", el("code", {}, "hackline-agent"), " for your OS."),
          el("li", {}, "Save the minimal config below as ", el("code", {}, "agent.toml"), "."),
          el("li", {}, "Run: ", el("code", {}, "hackline-agent agent.toml"), "."),
          el("li", {}, "Open ", el("a", { href: "http://127.0.0.1:9999", target: "_blank" }, "http://127.0.0.1:9999"), " and use the ports tab to add/remove exposed ports on the fly."),
          el("li", {}, "Create tunnels in the gateway UI pointed at your device ZID + port."),
        ),
        codeBlock(`# agent.toml
zid = "laptop-mine"
org = "default"
allowed_ports = [3000]

[zenoh]
mode    = "client"
connect = ["tcp/gateway.example.com:7447"]

[diag]
enabled = true
`),
      ],
    },
  ];

  tabSpec.forEach((t, i) => {
    const active = i === 0;
    tabs.appendChild(el("li", { class: "nav-item", role: "presentation" },
      el("button", {
        class: `nav-link ${active ? "active" : ""}`,
        id: `${t.id}-tab`,
        type: "button",
        role: "tab",
        onclick: (ev) => {
          ev.preventDefault();
          document.querySelectorAll("#setupTabs .nav-link").forEach((b) => b.classList.remove("active"));
          ev.currentTarget.classList.add("active");
          document.querySelectorAll(".tab-pane").forEach((p) => p.classList.remove("show", "active"));
          document.getElementById(`${t.id}-pane`).classList.add("show", "active");
        },
      }, t.label)));
    content.appendChild(el("div", {
      class: `tab-pane fade ${active ? "show active" : ""}`,
      id: `${t.id}-pane`,
      role: "tabpanel",
    }, ...t.body));
  });

  const help = helpTip("Copy-paste config templates for deploying this agent. 'Edge device' is for headless Linux boxes (Raspberry Pi, etc.) that run 24/7. 'PC proxy' is for developers who want to expose local services temporarily — like ngrok but over your own gateway.");

  view.replaceChildren(help, card("Setup recipes", el("div", {}, tabs, content)));
}
