import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { ErrorBox, PageBody, PageHeader } from "@/components/PageChrome";
import { readBaseUrl, readToken, writeBaseUrl, writeToken } from "@/lib/api";

export function SettingsPage() {
  const [baseUrl, setBaseUrl] = useState(readBaseUrl);
  const [token, setToken] = useState(() => readToken() ?? "");
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const [error, setError] = useState<unknown>(null);

  const onSave = (e: React.FormEvent) => {
    e.preventDefault();
    try {
      writeBaseUrl(baseUrl.trim() || null);
      writeToken(token.trim() || null);
      setSavedAt(Date.now());
      // The API client is bound at app boot and is not reactive to
      // these values. A reload picks up the new transport cleanly.
      setTimeout(() => window.location.reload(), 250);
    } catch (err) {
      setError(err);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Settings"
        description="Gateway base URL and bearer token. Stored in localStorage."
        help={
          <div className="space-y-2">
            <p>Where this UI talks to. <strong>Base URL</strong> points at a gateway’s REST API (e.g. <code>http://127.0.0.1:8080</code> for local dev, or your public hostname). <strong>Bearer token</strong> is the token of the operator you’re acting as.</p>
            <p><strong>Use it to:</strong> switch between gateways, or swap to a different user’s token to test their permissions.</p>
            <p>Saving reloads the page so the new transport takes effect. Values live in browser localStorage — clearing site data logs you out.</p>
          </div>
        }
      />
      <PageBody>
        <Card className="max-w-2xl">
          <CardHeader>
            <CardTitle>Connection</CardTitle>
          </CardHeader>
          <CardContent>
            <form onSubmit={onSave} className="flex flex-col gap-3">
              <div className="flex flex-col gap-1">
                <label className="text-[11px] text-muted-foreground">Base URL</label>
                <Input
                  value={baseUrl}
                  onChange={(e) => setBaseUrl(e.target.value)}
                  placeholder="https://hackline.example.com"
                />
              </div>
              <div className="flex flex-col gap-1">
                <label className="text-[11px] text-muted-foreground">Bearer token</label>
                <Input
                  type="password"
                  value={token}
                  onChange={(e) => setToken(e.target.value)}
                  placeholder="hk_…"
                  className="font-mono"
                />
              </div>
              {error ? <ErrorBox error={error} /> : null}
              <div className="flex items-center gap-3">
                <Button type="submit">Save and reload</Button>
                {savedAt ? (
                  <span className="text-xs text-muted-foreground">saved · reloading…</span>
                ) : null}
              </div>
            </form>
          </CardContent>
        </Card>
      </PageBody>
    </div>
  );
}
