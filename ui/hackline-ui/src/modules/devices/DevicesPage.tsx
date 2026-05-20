import { useEffect, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { EmptyState, ErrorBox, PageBody, PageHeader } from "@/components/PageChrome";
import { useApi } from "@/lib/api";
import type { Device, DeviceHealthEntry } from "@/lib/api";
import { navigate } from "@/lib/route";
import { relTime, shortId } from "@/lib/utils";

// Cadence for the in-page health refresh. Picked to track
// liveliness changes within human-noticeable time without making
// the per-row RTT cache (1 s TTL, see goal 27) miss too often:
// at 5 s the cache hits ~80% in steady state.
const HEALTH_POLL_MS = 5_000;

export function DevicesPage() {
  const api = useApi();
  const [devices, setDevices] = useState<Device[] | null>(null);
  const [health, setHealth] = useState<Map<number, DeviceHealthEntry> | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [creating, setCreating] = useState(false);
  const [zid, setZid] = useState("");
  const [label, setLabel] = useState("");

  // The two reads have no dependency on each other and the page is
  // useless without both, so fan them out and resolve into a single
  // state pair. This avoids a "list visible, dots loading" flash
  // and keeps the error path single-handler.
  const refresh = async () => {
    try {
      const [list, healthList] = await Promise.all([
        api.listDevices(),
        api.getDevicesHealth(),
      ]);
      setDevices(list);
      setHealth(new Map(healthList.map((h) => [h.device_id, h])));
      setError(null);
    } catch (e) {
      setError(e);
    }
  };

  useEffect(() => {
    void refresh();
    const id = window.setInterval(() => {
      void refresh();
    }, HEALTH_POLL_MS);
    return () => window.clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!zid.trim()) return;
    try {
      await api.createDevice({ zid: zid.trim(), label: label.trim() || null });
      setZid("");
      setLabel("");
      setCreating(false);
      void refresh();
    } catch (err) {
      setError(err);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Devices"
        description="Every box on the fabric. Click a row for tunnel + message-plane detail."
        actions={
          <Button size="sm" onClick={() => setCreating((v) => !v)}>
            {creating ? "Cancel" : "Add device"}
          </Button>
        }
      />
      <PageBody>
        {creating ? (
          <form
            onSubmit={onCreate}
            className="mb-4 flex flex-wrap items-end gap-2 rounded-lg border bg-card p-3"
          >
            <div className="flex flex-col gap-1">
              <label className="text-[11px] text-muted-foreground">ZID</label>
              <Input
                value={zid}
                onChange={(e) => setZid(e.target.value)}
                placeholder="01HG3K2P0Z…"
                className="w-[28ch] font-mono"
              />
            </div>
            <div className="flex flex-col gap-1">
              <label className="text-[11px] text-muted-foreground">Label</label>
              <Input
                value={label}
                onChange={(e) => setLabel(e.target.value)}
                placeholder="rack-7-a"
                className="w-48"
              />
            </div>
            <Button type="submit" size="sm">
              Create
            </Button>
          </form>
        ) : null}

        {error ? <ErrorBox error={error} /> : null}

        {devices == null ? (
          <div className="text-xs text-muted-foreground">loading…</div>
        ) : devices.length === 0 ? (
          <EmptyState
            title="No devices yet"
            description="Run hackline-agent on a device, or register a device manually with the button above."
          />
        ) : (
          <div className="overflow-hidden rounded-lg border">
            <table className="w-full text-sm">
              <thead className="bg-muted/40 text-xs text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 text-left font-medium">Status</th>
                  <th className="px-3 py-2 text-left font-medium">Label</th>
                  <th className="px-3 py-2 text-left font-medium">ZID</th>
                  <th className="px-3 py-2 text-left font-medium">RTT</th>
                  <th className="px-3 py-2 text-left font-medium">Last seen</th>
                </tr>
              </thead>
              <tbody>
                {devices.map((d) => {
                  // `health` is null until the first refresh resolves;
                  // showing "offline" before then would be a lie (the
                  // device might be perfectly online). The neutral
                  // placeholder distinguishes loading from offline,
                  // which matters because offline is the actionable
                  // state.
                  const h = health?.get(d.id);
                  return (
                    <tr
                      key={d.id}
                      className="cursor-pointer border-t hover:bg-accent/40"
                      onClick={() => navigate({ name: "device", id: d.id })}
                    >
                      <td className="px-3 py-2">
                        {h == null ? (
                          <Badge variant="outline">—</Badge>
                        ) : (
                          <Badge variant={h.online ? "ok" : "err"}>
                            {h.online ? "online" : "offline"}
                          </Badge>
                        )}
                      </td>
                      <td className="px-3 py-2">{d.label ?? <span className="text-muted-foreground">—</span>}</td>
                      <td className="px-3 py-2 font-mono text-xs">{shortId(d.zid, 14)}</td>
                      <td className="px-3 py-2 text-xs text-muted-foreground">
                        {h?.online && h.rtt_ms != null ? `${h.rtt_ms} ms` : "—"}
                      </td>
                      <td className="px-3 py-2 text-xs text-muted-foreground">
                        {relTime(d.last_seen_at)}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </PageBody>
    </div>
  );
}
