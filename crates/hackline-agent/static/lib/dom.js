// DOM helpers — tiny element builder used by all views.

export function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else if (k === "html") node.innerHTML = v;
    else if (k.startsWith("on") && typeof v === "function") {
      node.addEventListener(k.slice(2), v);
    } else {
      node.setAttribute(k, v);
    }
  }
  for (const c of children) {
    if (c == null) continue;
    node.appendChild(typeof c === "string" ? document.createTextNode(c) : c);
  }
  return node;
}

export function card(title, body, extraClass = "") {
  return el(
    "section",
    { class: `card card-section ${extraClass}` },
    title ? el("div", { class: "card-header" }, title) : null,
    el("div", { class: "card-body" }, body),
  );
}

export function kv(pairs) {
  const dl = el("dl", { class: "kv mono" });
  for (const [k, v] of pairs) {
    dl.appendChild(el("dt", {}, k));
    const dd = el("dd", {});
    if (v === null || v === undefined || v === "") {
      dd.appendChild(el("span", { class: "empty" }, "—"));
    } else if (Array.isArray(v)) {
      if (!v.length) {
        dd.appendChild(el("span", { class: "empty" }, "—"));
      } else {
        dd.textContent = v.join(", ");
      }
    } else {
      dd.textContent = String(v);
    }
    dl.appendChild(dd);
  }
  return dl;
}

export function codeBlock(text) {
  return el("pre", { class: "code-block mono" }, el("code", {}, text));
}

export function helpTip(text) {
  const body = el("div", { class: "help-body" },
    el("p", { class: "mb-0" }, text));
  const tip = el("div", { class: "help-tip" },
    el("button", {
      class: "help-toggle",
      type: "button",
      title: "Help",
      onclick: () => tip.classList.toggle("open"),
    }, "?"),
    body,
  );
  return tip;
}
