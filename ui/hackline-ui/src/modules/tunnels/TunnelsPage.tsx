import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { EmptyState, ErrorBox, PageBody, PageHeader } from "@/components/PageChrome";
import { useApi } from "@/lib/api";
import type { Tunnel } from "@/lib/api";
import { relTime } from "@/lib/utils";

export function TunnelsPage() {
  const api = useApi();
  const [rows, setRows] = useState<Tunnel[] | null>(null);
  const [error, setError] = useState<unknown>(null);

  const refresh = async () => {
    try {
      setRows(await api.listTunnels());
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
        title="Tunnels"
        description="Per-port HTTP / TCP / SSH tunnels published by hackline-agent."
        actions={
          <Button variant="outline" size="sm" onClick={refresh}>
            Refresh
          </Button>
        }
      />
      <PageBody>
        {error ? <ErrorBox error={error} /> : null}
        {rows == null ? (
          <div className="text-xs text-muted-foreground">loading…</div>
        ) : rows.length === 0 ? (
          <EmptyState
            title="No tunnels"
            description="Create one from a device's detail page or POST /v1/tunnels."
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
