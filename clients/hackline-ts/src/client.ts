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

// Single client interface so the UI never knows which transport is in
// play. Mirrors codeless-ui's `RpcClient` pattern: same component tree
// works against the real gateway. No-mock policy (see `index.ts`):
// this package ships only real-transport clients; tests use a real
// loopback gateway, not in-memory fixtures.
export interface ApiClient {
  baseUrl: string;
  hasToken(): boolean;

  // Health / claim
  health(): Promise<{ status: "ok" }>;
  claimStatus(): Promise<ClaimStatus>;
  claim(input: { token: string; owner: string }): Promise<{ token: string; owner: string }>;

  // Devices
  listDevices(): Promise<Device[]>;
  getDevice(id: number): Promise<Device>;
  createDevice(input: { zid: string; label?: string | null }): Promise<Device>;
  deleteDevice(id: number): Promise<void>;
  getDeviceInfo(id: number): Promise<AgentInfo>;
  getDeviceHealth(id: number): Promise<DeviceHealth>;
  // Collection-level health: one call returns one entry per device
  // in the caller's org. Server fans out the per-device probe in
  // parallel; the wire envelope (`{ items }`) is unwrapped here so
  // callers see an array, matching `listDevices` ergonomics.
  getDevicesHealth(): Promise<DeviceHealthEntry[]>;

  // Tunnels
  listTunnels(): Promise<Tunnel[]>;
  createTunnel(input: {
    device_id: number;
    kind: TunnelKind;
    local_port: number;
    public_hostname?: string | null;
    public_port?: number | null;
  }): Promise<Tunnel>;
  deleteTunnel(id: number): Promise<void>;

  // Cmd outbox
  sendCmd(input: {
    device_id: number;
    topic: string;
    payload: unknown;
    expires_in_s?: number;
  }): Promise<{ cmd_id: string }>;
  listCmd(input: {
    device_id: number;
    status?: CmdStatus;
    cursor?: number | null;
    limit?: number;
  }): Promise<Page<CmdOutboxRow>>;
  cancelCmd(cmd_id: string): Promise<void>;

  // Audit
  listAudit(input?: { cursor?: number | null; limit?: number }): Promise<Page<AuditEntry>>;

  // Users
  listUsers(): Promise<User[]>;
  createUser(input: {
    name: string;
    role: UserRole;
    device_scope?: string | null;
    tunnel_scope?: string | null;
    expires_in_s?: number;
  }): Promise<User>;
  deleteUser(id: number): Promise<void>;
  mintToken(user_id: number): Promise<MintedToken>;

  // SSE event stream. Returns an unsubscribe handle. The implementation
  // owns the EventSource lifecycle.
  subscribeEvents(
    listener: (event: GatewayEvent) => void,
    onError?: (error: Error) => void,
  ): () => void;
}

export class ApiError extends Error {
  status: number;
  body: unknown;
  constructor(status: number, message: string, body: unknown) {
    super(message);
    this.status = status;
    this.body = body;
  }
}
