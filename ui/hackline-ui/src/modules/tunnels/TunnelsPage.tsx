import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { EmptyState, ErrorBox, PageBody, PageHeader } from "@/components/PageChrome";
import { useApi } from "@/lib/api";
import type { Device, Tunnel, TunnelKind } from "@/lib/api";
import { relTime } from "@/lib/utils";

interface CreateFormState {
  device_id: string;
  kind: TunnelKind;
  local_port: string;
  public_hostname: string;
  public_port: string;
}

const EMPTY_FORM: CreateFormState = {
  device_id: "",
  kind: "http",
  local_port: "",
  public_hostname: "",
  public_port: "",
};

export function TunnelsPage() {
  const api = useApi();
  const [rows, setRows] = useState<Tunnel[] | null>(null);
  const [devices, setDevices] = useState<Device[]>([]);
  const [error, setError] = useState<unknown>(null);
  const [creating, setCreating] = useState(false);
  const [form, setForm] = useState<CreateFormState>(EMPTY_FORM);
  const [submitting, setSubmitting] = useState(false);

  // Both reads are independent and the form needs the device list
  // to populate its select, so fan them out and resolve into a
  // single state pair with one error sink.
  const refresh = async () => {
    try {
      const [tunnels, devs] = await Promise.all([
        api.listTunnels(),
        api.listDevices(),
      ]);
      setRows(tunnels);
      setDevices(devs);
      setError(null);
    } catch (e) {
      setError(e);
    }
  };

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Default the form's device_id to the first device whenever the
  // form opens so the operator doesn't have to pick on the common
  // single-device path.
  useEffect(() => {
    if (creating && !form.device_id && devices.length > 0) {
      setForm((f) => ({ ...f, device_id: String(devices[0].id) }));
    }
  }, [creating, devices, form.device_id]);

  const onCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    const device_id = Number(form.device_id);
    const local_port = Number(form.local_port);
    if (!device_id || !local_port) return;
    setSubmitting(true);
    try {
      await api.createTunnel({
        device_id,
        kind: form.kind,
        local_port,
        // Empty strings collapse to null. The gateway accepts either
        // a hostname (http) or a port (tcp); leaving the port blank
        // for tcp lets the gateway pick one.
        public_hostname:
          form.kind === "http" && form.public_hostname.trim()
            ? form.public_hostname.trim()
            : null,
        public_port:
          form.kind === "tcp" && form.public_port.trim()
            ? Number(form.public_port)
            : null,
      });
      setForm(EMPTY_FORM);
      setCreating(false);
      await refresh();
    } catch (err) {
      setError(err);
    } finally {
      setSubmitting(false);
    }
  };

  const onDelete = async (t: Tunnel) => {
    // Destructive and not undoable; gate behind a confirm so a
    // misclick on a busy fleet doesn't drop a live listener.
    const ok = window.confirm(
      `Delete tunnel #${t.id} (device #${t.device_id}, ${t.kind} :${t.local_port})?`,
    );
    if (!ok) return;
    try {
      await api.deleteTunnel(t.id);
      await refresh();
    } catch (e) {
      setError(e);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Tunnels"
        description="Per-port HTTP / TCP tunnels published by hackline-agent."
        help={
          <div className="space-y-2">
            <p>A tunnel exposes a port on a device through the gateway. The gateway forwards traffic over the Zenoh fabric to the agent, which connects to <code>127.0.0.1:&lt;local_port&gt;</code> on the device.</p>
            <p><strong>Kinds:</strong> <em>http</em> registers a public hostname (host-routed by the gateway); <em>tcp</em> opens a public port on the gateway.</p>
            <p><strong>Debug:</strong> if a tunnel returns errors, confirm the device is online (Devices page), the local port is listening on the device, and the port is in the agent’s <code>allowed_ports</code>.</p>
          </div>
        }
        actions={
          <>
            <Button variant="outline" size="sm" onClick={refresh}>
              Refresh
            </Button>
            <Button
              size="sm"
              onClick={() => {
                setCreating((v) => !v);
                if (creating) setForm(EMPTY_FORM);
              }}
            >
              {creating ? "Cancel" : "Add tunnel"}
            </Button>
          </>
        }
      />
      <PageBody>
        {creating ? (
          <form
            onSubmit={onCreate}
            className="mb-4 flex flex-wrap items-end gap-2 rounded-lg border bg-card p-3"
          >
            <div className="flex flex-col gap-1">
              <label className="text-[11px] text-muted-foreground">Device</label>
              <select
                value={form.device_id}
                onChange={(e) => setForm({ ...form, device_id: e.target.value })}
                className="h-9 rounded-md border border-input bg-transparent px-2 text-sm"
              >
                {devices.length === 0 ? (
                  <option value="">(no devices)</option>
                ) : null}
                {devices.map((d) => (
                  <option key={d.id} value={d.id}>
                    #{d.id} · {d.label ?? d.zid}
                  </option>
                ))}
              </select>
            </div>
            <div className="flex flex-col gap-1">
              <label className="text-[11px] text-muted-foreground">Kind</label>
              <select
                value={form.kind}
                onChange={(e) =>
                  setForm({ ...form, kind: e.target.value as TunnelKind })
                }
                className="h-9 rounded-md border border-input bg-transparent px-2 text-sm"
              >
                <option value="http">http</option>
                <option value="tcp">tcp</option>
              </select>
            </div>
            <div className="flex flex-col gap-1">
              <label className="text-[11px] text-muted-foreground">Local port</label>
              <Input
                type="number"
                min={1}
                max={65535}
                value={form.local_port}
                onChange={(e) => setForm({ ...form, local_port: e.target.value })}
                placeholder="9998"
                className="w-28 font-mono"
              />
            </div>
            {form.kind === "http" ? (
              <div className="flex flex-col gap-1">
                <label className="text-[11px] text-muted-foreground">
                  Public hostname
                </label>
                <Input
                  value={form.public_hostname}
                  onChange={(e) =>
                    setForm({ ...form, public_hostname: e.target.value })
                  }
                  placeholder="dev.example.com (optional)"
                  className="w-64"
                />
              </div>
            ) : (
              <div className="flex flex-col gap-1">
                <label className="text-[11px] text-muted-foreground">
                  Public port
                </label>
                <Input
                  type="number"
                  min={1}
                  max={65535}
                  value={form.public_port}
                  onChange={(e) =>
                    setForm({ ...form, public_port: e.target.value })
                  }
                  placeholder="auto"
                  className="w-28 font-mono"
                />
              </div>
            )}
            <Button type="submit" size="sm" disabled={submitting}>
              {submitting ? "Creating…" : "Create"}
            </Button>
          </form>
        ) : null}

        {error ? <ErrorBox error={error} /> : null}
        {rows == null ? (
          <div className="text-xs text-muted-foreground">loading…</div>
        ) : rows.length === 0 ? (
          <EmptyState
            title="No tunnels"
            description="Click “Add tunnel” to publish a device port."
          />
        ) : (
          <div className="overflow-hidden rounded-lg border">
            <table className="w-full text-sm">
              <thead className="bg-muted/40 text-xs text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 text-left font-medium">ID</th>
                  <th className="px-3 py-2 text-left font-medium">Device</th>
                  <th className="px-3 py-2 text-left font-medium">Kind</th>
                  <th className="px-3 py-2 text-left font-medium">Local port</th>
                  <th className="px-3 py-2 text-left font-medium">Public</th>
                  <th className="px-3 py-2 text-left font-medium">Created</th>
                  <th className="px-3 py-2 text-right font-medium" />
                </tr>
              </thead>
              <tbody>
                {rows.map((t) => (
                  <tr key={t.id} className="border-t">
                    <td className="px-3 py-2 font-mono text-xs">#{t.id}</td>
                    <td className="px-3 py-2 font-mono text-xs">#{t.device_id}</td>
                    <td className="px-3 py-2">{t.kind}</td>
                    <td className="px-3 py-2 font-mono text-xs">{t.local_port}</td>
                    <td className="px-3 py-2 font-mono text-xs">
                      {t.public_hostname ?? (t.public_port != null ? `:${t.public_port}` : "—")}
                    </td>
                    <td className="px-3 py-2 text-xs text-muted-foreground">
                      {relTime(t.created_at)}
                    </td>
                    <td className="px-3 py-2 text-right">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => onDelete(t)}
                      >
                        Delete
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </PageBody>
    </div>
  );
}
