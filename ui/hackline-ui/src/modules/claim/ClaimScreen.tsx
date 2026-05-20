import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { ErrorBox } from "@/components/PageChrome";
import { useApi, writeToken } from "@/lib/api";

// First-boot claim screen (SCOPE.md §6.1). The gateway prints the
// claim token on startup; the operator pastes it here along with their
// owner name. On success the minted owner-token is persisted and the
// app reloads into the authenticated UI.
export function ClaimScreen({ onDone }: { onDone: () => void }) {
  const api = useApi();
  const [claim, setClaim] = useState("");
  const [owner, setOwner] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<unknown>(null);

  const onSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!claim.trim() || !owner.trim()) return;
    setSubmitting(true);
    setError(null);
    try {
      const res = await api.claim({ token: claim.trim(), owner: owner.trim() });
      writeToken(res.token);
      onDone();
    } catch (err) {
      setError(err);
      setSubmitting(false);
    }
  };

  return (
    <div className="flex min-h-screen items-center justify-center p-6">
      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle>Claim this gateway</CardTitle>
          <p className="text-xs text-muted-foreground">
            On first boot the gateway prints a one-shot claim token. Paste it
            here to mint an owner token.
          </p>
        </CardHeader>
        <CardContent>
          <form onSubmit={onSubmit} className="flex flex-col gap-3">
            <div className="flex flex-col gap-1">
              <label className="text-[11px] text-muted-foreground">Claim token</label>
              <Input
                value={claim}
                onChange={(e) => setClaim(e.target.value)}
                placeholder="hk_…"
                className="font-mono"
                autoFocus
              />
            </div>
            <div className="flex flex-col gap-1">
              <label className="text-[11px] text-muted-foreground">Owner name</label>
              <Input
                value={owner}
                onChange={(e) => setOwner(e.target.value)}
                placeholder="alex"
              />
            </div>
            {error ? <ErrorBox error={error} /> : null}
            <Button type="submit" disabled={submitting}>
              {submitting ? "Claiming…" : "Claim"}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
