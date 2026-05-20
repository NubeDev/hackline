import { useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { ErrorBox, PageBody, PageHeader } from "@/components/PageChrome";
import { useApi } from "@/lib/api";
import type { GatewayEvent } from "@/lib/api";

interface Entry {
  id: number;
  ts: string;
  event: GatewayEvent;
}

const MAX = 500;

export function EventsPage() {
  const api = useApi();
  const [running, setRunning] = useState(false);
  const [entries, setEntries] = useState<Entry[]>([]);
  const [error, setError] = useState<unknown>(null);
  const counter = useRef(0);
  const unsubRef = useRef<null | (() => void)>(null);

  useEffect(() => {
    return () => {
      unsubRef.current?.();
    };
  }, []);

  const start = () => {
    if (running) return;
    setError(null);
    const unsub = api.subscribeEvents(
      (event) => {
        counter.current += 1;
        const id = counter.current;
        setEntries((prev) => {
          const next = [{ id, ts: new Date().toISOString(), event }, ...prev];
          return next.length > MAX ? next.slice(0, MAX) : next;
        });
      },
      (e) => setError(e),
    );
    unsubRef.current = unsub;
    setRunning(true);
  };

  const stop = () => {
    unsubRef.current?.();
    unsubRef.current = null;
    setRunning(false);
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Live events"
        description="SSE stream from /v1/events/stream. Newest first; capped at 500 entries."
        actions={
          <>
            {running ? (
              <Button size="sm" variant="outline" onClick={stop}>
                Stop
              </Button>
            ) : (
              <Button size="sm" onClick={start}>
                Start
              </Button>
            )}
            <Button size="sm" variant="ghost" onClick={() => setEntries([])}>
              Clear
            </Button>
          </>
        }
      />
      <PageBody>
        {error ? <ErrorBox error={error} /> : null}
        {entries.length === 0 ? (
          <div className="rounded-lg border border-dashed py-12 text-center text-xs text-muted-foreground">
            {running ? "waiting for events…" : 'press "Start" to subscribe'}
          </div>
        ) : (
          <div className="overflow-hidden rounded-lg border">
            <table className="w-full text-sm">
              <thead className="bg-muted/40 text-xs text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 text-left font-medium">Time</th>
                  <th className="px-3 py-2 text-left font-medium">Kind</th>
                  <th className="px-3 py-2 text-left font-medium">Data</th>
                </tr>
              </thead>
              <tbody>
                {entries.map((e) => (
                  <tr key={e.id} className="border-t align-top">
                    <td className="px-3 py-1.5 font-mono text-[11px] text-muted-foreground">
                      {new Date(e.ts).toLocaleTimeString()}
                    </td>
                    <td className="px-3 py-1.5 font-mono text-[11px]">{e.event.kind}</td>
                    <td className="px-3 py-1.5 font-mono text-[11px] text-muted-foreground">
                      {JSON.stringify(e.event.data)}
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
