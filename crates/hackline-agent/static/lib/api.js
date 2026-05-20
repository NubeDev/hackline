// API fetch helpers.

export async function getJSON(path) {
  const res = await fetch(path, { headers: { accept: "application/json" } });
  if (!res.ok) throw new Error(`${path}: ${res.status} ${res.statusText}`);
  return res.json();
}

export async function postJSON(path, body) {
  const res = await fetch(path, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    let msg = `${path}: ${res.status}`;
    try {
      const e = await res.json();
      if (e?.error) msg = e.error;
    } catch {}
    throw new Error(msg);
  }
  return res.json();
}

export async function del(path) {
  const res = await fetch(path, { method: "DELETE" });
  if (!res.ok && res.status !== 204) {
    let msg = `${path}: ${res.status}`;
    try {
      const e = await res.json();
      if (e?.error) msg = e.error;
    } catch {}
    throw new Error(msg);
  }
}
