import { afterEach, describe, expect, it } from "vitest";
import { deleteDeviceQuiet, deleteTunnelQuiet, freshClient, uniqueZid } from "./helpers";

describe("tunnels", () => {
  const devices: number[] = [];
  const tunnels: number[] = [];

  afterEach(async () => {
    while (tunnels.length) await deleteTunnelQuiet(tunnels.pop()!);
    while (devices.length) await deleteDeviceQuiet(devices.pop()!);
  });

  it("create + list + delete round-trip (no live agent required)", async () => {
    const c = freshClient();
    const dev = await c.createDevice({ zid: uniqueZid(), label: "tunnel-test" });
    devices.push(dev.id);

    // Use kind="http" so the gateway records the spec without
    // opening a TCP listener — that side-effect needs the http
    // host-routing config block we are not exercising here.
    const t = await c.createTunnel({
      device_id: dev.id,
      kind: "http",
      local_port: 8080,
      public_hostname: `t-${dev.id}.example.invalid`,
    });
    tunnels.push(t.id);

    expect(t.device_id).toBe(dev.id);
    expect(t.kind).toBe("http");
    expect(t.local_port).toBe(8080);

    const list = await c.listTunnels();
    expect(Array.isArray(list)).toBe(true);
    const found = list.find((x) => x.id === t.id);
    expect(found?.public_hostname).toBe(`t-${dev.id}.example.invalid`);

    await c.deleteTunnel(t.id);
    tunnels.pop();

    const after = await c.listTunnels();
    expect(after.find((x) => x.id === t.id)).toBeUndefined();
  });

  // Goal-18 lock-in: Tunnel wire shape per `DOCS/openapi.yaml`. The
  // prior TS type called `created_at: string` and omitted `enabled`,
  // both of which the gateway has always returned canonically.
  it("Tunnel wire shape: created_at is number, enabled is boolean", async () => {
    const c = freshClient();
    const dev = await c.createDevice({ zid: uniqueZid(), label: "tunnel-shape" });
    devices.push(dev.id);
    const t = await c.createTunnel({
      device_id: dev.id,
      kind: "http",
      local_port: 8081,
      public_hostname: `s-${dev.id}.example.invalid`,
    });
    tunnels.push(t.id);

    const list = await c.listTunnels();
    const found = list.find((x) => x.id === t.id);
    expect(found).toBeDefined();
    const raw = found as unknown as Record<string, unknown>;

    expect(typeof raw.created_at).toBe("number");
    expect(typeof raw.enabled).toBe("boolean");
    expect(typeof raw.id).toBe("number");
    expect(typeof raw.device_id).toBe("number");
    expect(typeof raw.kind).toBe("string");
    expect(raw.kind === "http" || raw.kind === "tcp").toBe(true);
    expect(typeof raw.local_port).toBe("number");
    expect(
      raw.public_hostname === null || typeof raw.public_hostname === "string",
    ).toBe(true);
    expect(
      raw.public_port === null || typeof raw.public_port === "number",
    ).toBe(true);
  });
});
