import { useEffect, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { EmptyState, ErrorBox, PageBody, PageHeader } from "@/components/PageChrome";
import { ApiError, useApi } from "@/lib/api";
import type { AgentInfo, Device, DeviceHealth, Tunnel } from "@/lib/api";
import { navigate } from "@/lib/route";
import { relTime } from "@/lib/utils";

// Same cadence as `DevicesPage` so liveliness freshness feels
// consistent when the user navigates between list and detail.
// At 5 s the goal-27 1 s RTT cache misses on every refresh for
// this single device (which is fine: one Zenoh query every 5 s).
const HEALTH_POLL_MS = 5_000;

// Info is identity + policy + uptime — a much slower cadence than
// health is the right tradeoff. 30 s is fast enough that a rolling
// upgrade visibly converges in the card and slow enough that an
// idle detail page costs ~2 Zenoh queries/min. Info generation is
// in-memory on the agent (zero I/O), so the per-query cost is
// negligible; the cadence is bounded by operator-perception, not
// agent load.
const INFO_POLL_MS = 30_000;

export function DeviceDetailPage({ id }: { id: number }) {
  const api = useApi();
  const [device, setDevice] = useState<Device | null>(null);
  const [info, setInfo] = useState<AgentInfo | null>(null);
  const [infoErr, setInfoErr] = useState<ApiError | null>(null);
  const [health, setHealth] = useState<DeviceHealth | null>(null);
  const [tunnels, setTunnels] = useState<Tunnel[]>([]);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    let cancelled = false;
    const pollHealth = () => {
      api
        .getDeviceHealth(id)
        .then((h) => !cancelled && setHealth(h))
        .catch(() => {});
    };
    // Info is best-effort *for server-told failures* (ApiError):
    // 503/504/502 get distinct copy in the card. Any other failure
    // (network, programming error) routes to the page-level
    // ErrorBox via setError so it isn't silently swallowed.
    // Success clears any prior infoErr and vice versa so the card
    // is always a true snapshot of the last poll.
    const pollInfo = () => {
      api
        .getDeviceInfo(id)
        .then((i) => {
          if (cancelled) return;
          setInfo(i);
          setInfoErr(null);
        })
        .catch((e) => {
          if (cancelled) return;
          if (e instanceof ApiError) {
            setInfo(null);
            setInfoErr(e);
          } else {
            setError(e);
          }
        });
    };
    (async () => {
      try {
        const [d, ts] = await Promise.all([api.getDevice(id), api.listTunnels()]);
        if (cancelled) return;
        setDevice(d);
        setTunnels(ts.filter((t) => t.device_id === id));
        // First ticks fire here so health and info both fill in
        // immediately, not after their respective poll intervals.
        pollHealth();
        pollInfo();
      } catch (e) {
        if (!cancelled) setError(e);
      }
    })();
    const healthIntervalId = window.setInterval(pollHealth, HEALTH_POLL_MS);
    const infoIntervalId = window.setInterval(pollInfo, INFO_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(healthIntervalId);
      window.clearInterval(infoIntervalId);
    };
  }, [api, id]);

  if (error) {
    return (
      <div className="flex h-full flex-col">
        <PageHeader title={`Device #${id}`} actions={<Button variant="outline" size="sm" onClick={() => navigate({ name: "devices" })}>Back</Button>} />
        <PageBody>
          <ErrorBox error={error} />
        </PageBody>
      </div>
    );
  }
  if (!device) {
    return (
      <div className="flex h-full flex-col">
        <PageHeader title={`Device #${id}`} />
        <PageBody>
          <div className="text-xs text-muted-foreground">loading…</div>
        </PageBody>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title={device.label ?? `Device #${device.id}`}
        description={device.zid}
        actions={
          <>
            {health == null ? (
              <Badge variant="outline">—</Badge>
            ) : (
              <Badge variant={health.online ? "ok" : "err"}>
                {health.online ? "online" : "offline"}
              </Badge>
            )}
            <Button variant="outline" size="sm" onClick={() => navigate({ name: "devices" })}>
              Back
            </Button>
          </>
        }
      />
      <PageBody className="space-y-4">
        <div className="grid gap-4 md:grid-cols-2">
          <Card>
            <CardHeader>
              <CardTitle>Health</CardTitle>
            </CardHeader>
            <CardContent className="space-y-1 text-xs">
              {/* All rows come from the same probe so they're
                  rendered atomically: either all from `health` or
                  all `—`. Mixing in `device.last_seen_at` as a
                  fallback would let `online` flip while `last seen`
                  lagged a tick — a visible UI lie. */}
              <Row label="online" value={health == null ? "—" : String(health.online)} />
              <Row
                label="last seen"
                value={health == null ? "—" : relTime(health.last_seen_at)}
              />
              <Row
                label="rtt"
                value={health?.rtt_ms != null ? `${health.rtt_ms} ms` : "—"}
              />
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>Agent info</CardTitle>
            </CardHeader>
            <CardContent className="space-y-1 text-xs">
              {info ? (
                <>
                  <Row label="version" value={info.version} />
                  <Row label="uptime" value={`${Math.round(info.uptime_s / 60)} min`} />
                  <Row label="allowed ports" value={info.allowed_ports.join(", ") || "—"} />
                </>
              ) : infoErr ? (
                <div className="text-muted-foreground">{agentInfoErrorCopy(infoErr.status)}</div>
              ) : (
                <div className="text-muted-foreground">live query pending…</div>
              )}
            </CardContent>
          </Card>
        </div>

        <Card>
          <CardHeader>
            <CardTitle>Tunnels</CardTitle>
          </CardHeader>
          <CardContent>
            {tunnels.length === 0 ? (
              <EmptyState
                title="No tunnels"
                description="Add a tunnel from the Tunnels page to expose a local port."
              />
            ) : (
              <table className="w-full text-sm">
                <thead className="text-xs text-muted-foreground">
                  <tr>
                    <th className="px-2 py-1 text-left font-medium">Kind</th>
                    <th className="px-2 py-1 text-left font-medium">Local port</th>
                    <th className="px-2 py-1 text-left font-medium">Public</th>
                    <th className="px-2 py-1 text-left font-medium">Created</th>
                  </tr>
                </thead>
                <tbody>
                  {tunnels.map((t) => (
                    <tr key={t.id} className="border-t">
                      <td className="px-2 py-1.5">{t.kind}</td>
                      <td className="px-2 py-1.5 font-mono text-xs">{t.local_port}</td>
                      <td className="px-2 py-1.5 font-mono text-xs">
                        {t.public_hostname ?? `:${t.public_port ?? "—"}`}
                      </td>
                      <td className="px-2 py-1.5 text-xs text-muted-foreground">
                        {relTime(t.created_at)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </CardContent>
        </Card>
      </PageBody>
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-mono">{value}</span>
    </div>
  );
}

// Status-to-copy mapping matches the gateway's `api/devices/info.rs`
// failure mapping: 504 = `agent_timeout`, 503 = `agent_unreachable`,
// 502 = decode failure. The numeric fallback exists so a future
// status (e.g. 401 from a token that expired between mount and
// fetch) still renders something legible.
function agentInfoErrorCopy(status: number): string {
  switch (status) {
    case 504:
      return "agent did not reply within 1 s";
    case 503:
      return "no agent listening for this device";
    case 502:
      return "agent reply could not be decoded";
    default:
      return `agent info unavailable (HTTP ${status})`;
  }
}
