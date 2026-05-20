import { ApiError, type ApiClient } from "./client";
import type {
  AgentInfo,
  AuditEntry,
  ClaimStatus,
  CmdOutboxRow,
  CmdStatus,
  Device,
  DeviceHealth,
  DeviceHealthEntry,
  GatewayEvent,
  MintedToken,
  Page,
  Tunnel,
  TunnelKind,
  User,
  UserRole,
} from "./types";

export interface HttpApiClientOptions {
  baseUrl: string;
  token: string | null;
}

// Speaks the gateway's REST + SSE surface (SCOPE.md §5.3 / §5.4).
// EventSource is the right primitive for the control-plane stream:
// `flush_interval -1` is required on Caddy, see SCOPE.md §5.4 / §9.8.
export class HttpApiClient implements ApiClient {
  baseUrl: string;
  private token: string | null;

  constructor(opts: HttpApiClientOptions) {
    this.baseUrl = opts.baseUrl.replace(/\/$/, "");
    this.token = opts.token;
  }

  hasToken(): boolean {
    return this.token != null && this.token.length > 0;
  }

  // ---- low-level ----

  private async req<T>(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    const headers: Record<string, string> = {
      accept: "application/json",
    };
    if (body !== undefined) headers["content-type"] = "application/json";
    if (this.token) headers["authorization"] = `Bearer ${this.token}`;
    const res = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (!res.ok) {
      let parsed: unknown = null;
      const text = await res.text().catch(() => "");
      try {
        parsed = text ? JSON.parse(text) : null;
      } catch {
        parsed = text;
      }
      throw new ApiError(res.status, `HTTP ${res.status} ${path}`, parsed);
    }
    if (res.status === 204) return undefined as T;
    const ct = res.headers.get("content-type") ?? "";
    if (!ct.includes("json")) return undefined as T;
    return (await res.json()) as T;
  }

  // ---- health / claim ----
  health = () => this.req<{ status: "ok" }>("GET", "/v1/health");
  claimStatus = () => this.req<ClaimStatus>("GET", "/v1/claim/status");
  claim = (input: { token: string; owner: string }) =>
    this.req<{ token: string; owner: string }>("POST", "/v1/claim", input);

  // ---- devices ----
  listDevices = () => this.req<Device[]>("GET", "/v1/devices");
  getDevice = (id: number) => this.req<Device>("GET", `/v1/devices/${id}`);
  createDevice = (input: { zid: string; label?: string | null }) =>
    this.req<Device>("POST", "/v1/devices", input);
  deleteDevice = (id: number) =>
    this.req<void>("DELETE", `/v1/devices/${id}`);
  getDeviceInfo = (id: number) =>
    this.req<AgentInfo>("GET", `/v1/devices/${id}/info`);
  getDeviceHealth = (id: number) =>
    this.req<DeviceHealth>("GET", `/v1/devices/${id}/health`);
  // Collection-level health. The wire is `{ items: [...] }` (an
  // envelope reserved for future pagination); we unwrap to keep
  // the client surface array-shaped like `listDevices`.
  getDevicesHealth = async (): Promise<DeviceHealthEntry[]> => {
    const res = await this.req<{ items: DeviceHealthEntry[] }>(
      "GET",
      "/v1/devices/health",
    );
    return res.items;
  };

  // ---- tunnels ----
  listTunnels = () => this.req<Tunnel[]>("GET", "/v1/tunnels");
  createTunnel = (input: {
    device_id: number;
    kind: TunnelKind;
    local_port: number;
    public_hostname?: string | null;
    public_port?: number | null;
  }) => this.req<Tunnel>("POST", "/v1/tunnels", input);
  deleteTunnel = (id: number) => this.req<void>("DELETE", `/v1/tunnels/${id}`);

  // ---- cmd outbox ----
  sendCmd = (input: {
    device_id: number;
    topic: string;
    payload: unknown;
    expires_in_s?: number;
  }) =>
    this.req<{ cmd_id: string }>(
      "POST",
      `/v1/devices/${input.device_id}/cmd/${encodeURIComponent(input.topic)}`,
      { payload: input.payload, expires_in: input.expires_in_s },
    );
  listCmd = (input: {
    device_id: number;
    status?: CmdStatus;
    cursor?: number | null;
    limit?: number;
  }) => {
    const qs = new URLSearchParams();
    if (input.status) qs.set("status", input.status);
    if (input.cursor != null) qs.set("cursor", String(input.cursor));
    if (input.limit) qs.set("limit", String(input.limit));
    const q = qs.toString();
    return this.req<Page<CmdOutboxRow>>(
      "GET",
      `/v1/devices/${input.device_id}/cmd${q ? `?${q}` : ""}`,
    );
  };
  cancelCmd = (cmd_id: string) =>
    this.req<void>("DELETE", `/v1/cmd/${encodeURIComponent(cmd_id)}`);

  // ---- audit ----
  listAudit = (input?: { cursor?: number | null; limit?: number }) => {
    const qs = new URLSearchParams();
    if (input?.cursor != null) qs.set("cursor", String(input.cursor));
    if (input?.limit) qs.set("limit", String(input.limit));
    const q = qs.toString();
    return this.req<Page<AuditEntry>>("GET", `/v1/audit${q ? `?${q}` : ""}`);
  };

  // ---- users ----
  listUsers = () => this.req<User[]>("GET", "/v1/users");
  createUser = (input: {
    name: string;
    role: UserRole;
    device_scope?: string | null;
    tunnel_scope?: string | null;
    expires_in_s?: number;
  }) =>
    this.req<User>("POST", "/v1/users", {
      ...input,
      expires_in: input.expires_in_s,
    });
  deleteUser = (id: number) => this.req<void>("DELETE", `/v1/users/${id}`);
  mintToken = (user_id: number) =>
    this.req<MintedToken>("POST", `/v1/users/${user_id}/tokens`);

  // ---- SSE ----
  // EventSource has no header support, so the bearer token is passed
  // as a query parameter. The gateway accepts this on stream endpoints
  // (single-tenant, single trust boundary — same security level as the
  // header form, just URL-shaped). If the gateway tightens this later,
  // swap to a fetch-based ReadableStream reader without changing the
  // surface of `subscribeEvents`.
  subscribeEvents(
    listener: (event: GatewayEvent) => void,
    onError?: (error: Error) => void,
  ): () => void {
    const url = new URL(`${this.baseUrl}/v1/events/stream`);
    if (this.token) url.searchParams.set("token", this.token);
    const es = new EventSource(url.toString());
    es.onmessage = (ev) => {
      try {
        const parsed = JSON.parse(ev.data) as GatewayEvent;
        listener(parsed);
      } catch (err) {
        onError?.(err as Error);
      }
    };
    es.onerror = () => {
      onError?.(new Error("SSE stream error"));
    };
    return () => es.close();
  }
}
