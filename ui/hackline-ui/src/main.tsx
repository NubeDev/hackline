import { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";

import "./styles/globals.css";

import { App } from "./App";
import {
  ApiProvider,
  HttpApiClient,
  readBaseUrl,
  readToken,
  writeBaseUrl,
  writeToken,
  type ApiClient,
} from "./lib/api";
import { ClaimScreen } from "./modules/claim/ClaimScreen";

// Boot order. The UI talks only to a real gateway — no in-memory
// mock fast-path. If the gateway is down, the operator sees a
// "cannot reach gateway" screen and starts it.
//
//   1. Probe `GET /v1/health` on the configured base URL.
//      - unreachable -> honest "cannot reach gateway" screen
//      - reachable + unclaimed -> ClaimScreen (writes the owner token)
//      - reachable + claimed but no token -> Settings is the only
//        useful page; we render a token-prompt screen.
//      - reachable + claimed + token -> full App.

type Mode =
  | { kind: "probing" }
  | { kind: "down"; baseUrl: string }
  | { kind: "claim" }
  | { kind: "no-token" }
  | { kind: "ready" };

function buildHttp(): HttpApiClient {
  return new HttpApiClient({ baseUrl: readBaseUrl(), token: readToken() });
}

function Root() {
  const [baseUrl] = useState(readBaseUrl);
  const [client, setClient] = useState<ApiClient | null>(null);
  const [mode, setMode] = useState<Mode>({ kind: "probing" });

  useEffect(() => {
    if (mode.kind !== "probing") return;
    let cancelled = false;
    (async () => {
      const probe = new HttpApiClient({ baseUrl, token: readToken() });
      try {
        await probe.health();
      } catch {
        if (!cancelled) setMode({ kind: "down", baseUrl });
        return;
      }
      let claim;
      try {
        claim = await probe.claimStatus();
      } catch {
        // Older gateway revs might not expose claim status; treat as
        // "claimed" so we don't block the operator.
        claim = { claimed: true, can_claim: false };
      }
      if (cancelled) return;
      if (!claim.claimed && claim.can_claim) {
        setClient(probe);
        setMode({ kind: "claim" });
        return;
      }
      const c = buildHttp();
      // Validate the stored token before letting the App tree mount.
      // A stale/revoked token would otherwise render the full UI and
      // make every page show "HTTP 401" with no obvious recovery —
      // the token gate is what surfaces NoToken (which lets the
      // operator paste a fresh one).
      if (c.hasToken()) {
        try {
          await c.listDevices();
        } catch (e) {
          const status = (e as { status?: number } | null)?.status;
          if (status === 401 || status === 403) {
            writeToken(null);
            if (!cancelled) {
              setClient(buildHttp());
              setMode({ kind: "no-token" });
            }
            return;
          }
        }
      }
      setClient(c);
      setMode(c.hasToken() ? { kind: "ready" } : { kind: "no-token" });
    })();
    return () => {
      cancelled = true;
    };
  }, [mode, baseUrl]);

  if (mode.kind === "probing" || !client) return null;
  if (mode.kind === "down") {
    return <ServerDown baseUrl={mode.baseUrl} onRetry={() => setMode({ kind: "probing" })} />;
  }
  if (mode.kind === "claim") {
    return (
      <ApiProvider client={client}>
        <ClaimScreen onDone={() => window.location.reload()} />
      </ApiProvider>
    );
  }
  if (mode.kind === "no-token") {
    return <NoToken />;
  }
  return (
    <ApiProvider client={client}>
      <App />
    </ApiProvider>
  );
}

function ServerDown({ baseUrl, onRetry }: { baseUrl: string; onRetry: () => void }) {
  return (
    <div className="flex min-h-screen items-center justify-center p-6">
      <div className="max-w-lg space-y-3">
        <h1 className="text-base font-semibold">cannot reach hackline gateway</h1>
        <p className="text-xs text-muted-foreground">The UI tried:</p>
        <pre className="rounded bg-muted px-2 py-1 text-xs">{baseUrl}</pre>
        <p className="text-xs text-muted-foreground">
          Start the gateway and retry. The UI talks only to a real
          gateway — there is no offline / mock mode.
        </p>
        <button
          onClick={onRetry}
          className="rounded-md border px-3 py-1 text-xs hover:bg-accent"
        >
          retry
        </button>
      </div>
    </div>
  );
}

function NoToken() {
  // Inline form rather than a link to `#/settings`: the App tree (and
  // therefore SettingsPage) is gated behind a configured token, so a
  // hash-route nav would just re-render this same screen and trap the
  // user. Give them the inputs they need right here.
  const [base, setBase] = useState(() => readBaseUrl());
  const [tok, setTok] = useState("");
  const save = () => {
    writeBaseUrl(base.trim() || null);
    writeToken(tok.trim() || null);
    window.location.reload();
  };
  return (
    <div className="flex min-h-screen items-center justify-center p-6">
      <div className="w-full max-w-md space-y-4">
        <div className="space-y-1">
          <h1 className="text-base font-semibold">No bearer token configured</h1>
          <p className="text-xs text-muted-foreground">
            Paste a bearer token to unlock the UI. Mint one with{" "}
            <code>hackline users tokens</code>, or use the owner token printed
            by <code>POST /v1/claim</code>.
          </p>
        </div>
        <label className="block space-y-1">
          <span className="text-xs text-muted-foreground">Base URL</span>
          <input
            value={base}
            onChange={(e) => setBase(e.target.value)}
            placeholder="leave blank to use this origin (vite proxy)"
            className="w-full rounded-md border bg-background px-2 py-1 text-xs"
          />
        </label>
        <label className="block space-y-1">
          <span className="text-xs text-muted-foreground">Bearer token</span>
          <input
            value={tok}
            onChange={(e) => setTok(e.target.value)}
            placeholder="hk_…"
            autoFocus
            className="w-full rounded-md border bg-background px-2 py-1 font-mono text-xs"
          />
        </label>
        <button
          onClick={save}
          disabled={!tok.trim()}
          className="rounded-md border px-3 py-1 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
        >
          Save and reload
        </button>
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <Root />,
);
