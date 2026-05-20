// Wire types matching `hackline-proto` and the REST surface in
// `hackline/SCOPE.md` §5. Kept hand-written for now; once the gateway
// emits TS via specta we can replace these with the generated module
// (the consumer-facing `hackline-ts` package already exists for the
// connection-lifecycle subset, see `hackline/clients/hackline-ts/`).

// Wire shape per `DOCS/openapi.yaml` Device + the gateway's actual JSON
// (`crates/hackline-gateway/src/db/devices.rs::Device`). Timestamps are
// unix epoch seconds. `online` is *not* a field on this row — callers
// derive it from `GET /v1/devices[/{id}]/health`.
export interface Device {
  id: number;
  zid: string;
  label: string;
  customer_id: number | null;
  last_seen_at: number | null;
  created_at: number;
}

export interface DeviceHealth {
  online: boolean;
  last_seen_at: number | null;
  rtt_ms: number | null;
}

// Wire shape per `DOCS/openapi.yaml` DeviceHealthEntry. Same fields
// as `DeviceHealth` plus the device id, which is required when the
// rows arrive as a collection (the per-id endpoint keys by URL).
export interface DeviceHealthEntry {
  device_id: number;
  online: boolean;
  last_seen_at: number | null;
  rtt_ms: number | null;
}

export interface AgentInfo {
  zid: string;
  version: string;
  allowed_ports: number[];
  uptime_s: number;
}

export type TunnelKind = "http" | "tcp";

// Wire shape per `DOCS/openapi.yaml` Tunnel + the gateway row in
// `crates/hackline-gateway/src/db/tunnels.rs::Tunnel`. Timestamps are
// unix epoch seconds (int64), not ISO strings. `enabled` is required
// on the wire — the prior TS type omitted it; `"ssh"` was speculative
// (openapi enum is `[tcp, http]`).
export interface Tunnel {
  id: number;
  device_id: number;
  kind: TunnelKind;
  local_port: number;
  public_hostname: string | null;
  public_port: number | null;
  enabled: boolean;
  created_at: number;
}

export type CmdStatus = "pending" | "delivered" | "acked" | "expired";

export interface CmdOutboxRow {
  cmd_id: string;
  device_id: number;
  topic: string;
  status: CmdStatus;
  enqueued_at: string;
  expires_at: string;
  delivered_at: string | null;
  acked_at: string | null;
  result: "accepted" | "rejected" | "failed" | "done" | null;
  detail: string | null;
}

// Wire shape per `DOCS/openapi.yaml` AuditEntry, projected
// server-side from `db::audit::AuditEntry` (the DB row carries
// `tunnel.session`-shaped extras that are not on the wire). The
// prior TS shape (`ts: string`, `actor: string`, `target: string |
// null`, `detail: object | null`) had no producer — `e.actor` and
// `e.target` rendered as `undefined` in the UI.
export interface AuditEntry {
  id: number;
  at: number;
  actor_user_id: number | null;
  action: string;
  subject: string;
  detail: Record<string, unknown> | null;
}

export type UserRole = "owner" | "admin" | "operator" | "viewer";

export interface User {
  id: number;
  name: string;
  role: UserRole;
  device_scope: string | null;
  tunnel_scope: string | null;
  expires_at: string | null;
  created_at: string;
}

// Wire shape per `DOCS/openapi.yaml` TokenMinted + the gateway
// handler `crates/hackline-gateway/src/api/users/mint_token.rs`,
// which serialises `MintTokenResponse { token: String }` only.
// The prior TS shape carried a fabricated `expires_at: string|null`
// that the server has never returned.
export interface MintedToken {
  token: string;
}

// Generic page envelope per `DOCS/openapi.yaml` §AuditPage. All
// paginated endpoints (audit, cmd outbox, events, logs) share this
// shape: `items` (not `entries`) and a numeric cursor. The prior
// `{ entries, next_cursor: string|null }` shape disagreed with the
// schema on both axes.
export interface Page<T> {
  items: T[];
  next_cursor: number | null;
}

export interface ClaimStatus {
  claimed: boolean;
  can_claim: boolean;
}

// SSE control-plane event kinds (SCOPE.md §5.4).
export type GatewayEvent =
  | { kind: "device.online"; data: { device_id: number; zid: string; at: string } }
  | { kind: "device.offline"; data: { device_id: number; zid: string; at: string; reason: string } }
  | {
      kind: "tunnel.opened";
      data: { tunnel_id: number; device_id: number; request_id: string; peer: string | null };
    }
  | {
      kind: "tunnel.closed";
      data: {
        tunnel_id: number;
        request_id: string;
        bytes_up: number;
        bytes_down: number;
        duration_ms: number;
      };
    }
  | { kind: "cmd.queued"; data: { cmd_id: string; device_id: number; topic: string } }
  | { kind: "cmd.delivered"; data: { cmd_id: string; device_id: number; at: string } }
  | {
      kind: "cmd.acked";
      data: { cmd_id: string; device_id: number; result: string; at: string };
    }
  | { kind: "cmd.expired"; data: { cmd_id: string; device_id: number } }
  | { kind: "audit.entry"; data: AuditEntry };
