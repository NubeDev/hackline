import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { EmptyState, ErrorBox, PageBody, PageHeader } from "@/components/PageChrome";
import { useApi } from "@/lib/api";
import type { AuditEntry } from "@/lib/api";
import { relTime } from "@/lib/utils";

export function AuditPage() {
  const api = useApi();
  const [entries, setEntries] = useState<AuditEntry[] | null>(null);
  const [error, setError] = useState<unknown>(null);

  const refresh = async () => {
    try {
      const page = await api.listAudit({ limit: 200 });
      setEntries(page.items);
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
        title="Audit log"
        description="Append-only record of every privileged action."
        actions={
          <Button size="sm" variant="outline" onClick={refresh}>
            Refresh
          </Button>
        }
      />
      <PageBody>
        {error ? <ErrorBox error={error} /> : null}
        {entries == null ? (
          <div className="text-xs text-muted-foreground">loading…</div>
        ) : entries.length === 0 ? (
          <EmptyState title="No audit entries" />
        ) : (
          <div className="overflow-hidden rounded-lg border">
            <table className="w-full text-sm">
              <thead className="bg-muted/40 text-xs text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 text-left font-medium">Time</th>
                  <th className="px-3 py-2 text-left font-medium">Actor</th>
                  <th className="px-3 py-2 text-left font-medium">Action</th>
                  <th className="px-3 py-2 text-left font-medium">Subject</th>
                  <th className="px-3 py-2 text-left font-medium">Detail</th>
                </tr>
              </thead>
              <tbody>
                {entries.map((e) => (
                  <tr key={e.id} className="border-t align-top">
                    <td className="px-3 py-1.5 text-xs text-muted-foreground">{relTime(e.at)}</td>
                    <td className="px-3 py-1.5 text-xs">{e.actor_user_id != null ? `user:${e.actor_user_id}` : "—"}</td>
                    <td className="px-3 py-1.5 font-mono text-xs">{e.action}</td>
                    <td className="px-3 py-1.5 font-mono text-xs">{e.subject || "—"}</td>
                    <td className="px-3 py-1.5 font-mono text-[11px] text-muted-foreground">
                      {e.detail ? JSON.stringify(e.detail) : "—"}
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
