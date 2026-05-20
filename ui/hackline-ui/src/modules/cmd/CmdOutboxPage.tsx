import { useEffect, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { EmptyState, ErrorBox, PageBody, PageHeader } from "@/components/PageChrome";
import { useApi } from "@/lib/api";
import type { CmdOutboxRow, CmdStatus } from "@/lib/api";
import { relTime, shortId } from "@/lib/utils";

const STATUSES: (CmdStatus | "")[] = ["", "pending", "delivered", "acked", "expired"];

function statusVariant(s: CmdStatus): "ok" | "warn" | "err" | "secondary" {
  if (s === "acked") return "ok";
  if (s === "delivered") return "secondary";
  if (s === "pending") return "warn";
  return "err";
}

export function CmdOutboxPage() {
  const api = useApi();
  const [deviceId, setDeviceId] = useState(1);
  const [status, setStatus] = useState<CmdStatus | "">("");
  const [rows, setRows] = useState<CmdOutboxRow[] | null>(null);
  const [error, setError] = useState<unknown>(null);

  const refresh = async () => {
    try {
      setRows(null);
      const page = await api.listCmd({
        device_id: deviceId,
        status: status === "" ? undefined : status,
      });
      setRows(page.items);
      setError(null);
    } catch (e) {
      setError(e);
    }
  };

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Cmd outbox"
        description="Durable device-bound commands. Pending entries flush when the device comes back online."
      />
      <PageBody>
        <div className="mb-4 flex flex-wrap items-end gap-2 rounded-lg border bg-card p-3">
          <div className="flex flex-col gap-1">
            <label className="text-[11px] text-muted-foreground">Device ID</label>
            <Input
              type="number"
              min={1}
              value={deviceId}
              onChange={(e) => setDeviceId(Number(e.target.value))}
              className="w-24"
            />
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-[11px] text-muted-foreground">Status</label>
            <select
              value={status}
              onChange={(e) => setStatus(e.target.value as CmdStatus | "")}
              className="h-9 rounded-md border border-input bg-transparent px-2 text-sm"
            >
              {STATUSES.map((s) => (
                <option key={s} value={s}>
                  {s === "" ? "(any)" : s}
                </option>
              ))}
            </select>
          </div>
          <Button size="sm" onClick={refresh}>
            Refresh
          </Button>
        </div>

        {error ? <ErrorBox error={error} /> : null}
        {rows == null ? (
          <div className="text-xs text-muted-foreground">loading…</div>
        ) : rows.length === 0 ? (
          <EmptyState title="No cmd entries" description="Send one via POST /v1/devices/:id/cmd/:topic." />
        ) : (
          <div className="overflow-hidden rounded-lg border">
            <table className="w-full text-sm">
              <thead className="bg-muted/40 text-xs text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 text-left font-medium">cmd_id</th>
                  <th className="px-3 py-2 text-left font-medium">Topic</th>
                  <th className="px-3 py-2 text-left font-medium">Status</th>
                  <th className="px-3 py-2 text-left font-medium">Result</th>
                  <th className="px-3 py-2 text-left font-medium">Enqueued</th>
                  <th className="px-3 py-2 text-left font-medium">Acked</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r) => (
                  <tr key={r.cmd_id} className="border-t">
                    <td className="px-3 py-2 font-mono text-xs">{shortId(r.cmd_id, 12)}</td>
                    <td className="px-3 py-2 font-mono text-xs">{r.topic}</td>
                    <td className="px-3 py-2">
                      <Badge variant={statusVariant(r.status)}>{r.status}</Badge>
                    </td>
                    <td className="px-3 py-2 text-xs">{r.result ?? "—"}</td>
                    <td className="px-3 py-2 text-xs text-muted-foreground">
                      {relTime(r.enqueued_at)}
                    </td>
                    <td className="px-3 py-2 text-xs text-muted-foreground">
                      {relTime(r.acked_at)}
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
