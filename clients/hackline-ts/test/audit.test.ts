import { afterEach, describe, expect, it } from "vitest";
import { deleteDeviceQuiet, deleteTunnelQuiet, freshClient, uniqueZid } from "./helpers";

describe("audit", () => {
  const devices: number[] = [];
  const tunnels: number[] = [];

  afterEach(async () => {
    while (tunnels.length) await deleteTunnelQuiet(tunnels.pop()!);
    while (devices.length) await deleteDeviceQuiet(devices.pop()!);
  });

  it("device.create / tunnel.create / cmd.send all surface in listAudit()", async () => {
    const c = freshClient();
    const zid = uniqueZid();

    const dev = await c.createDevice({ zid, label: "audit-test" });
    devices.push(dev.id);

    const t = await c.createTunnel({
      device_id: dev.id,
      kind: "http",
      local_port: 9090,
      public_hostname: `audit-${dev.id}.example.invalid`,
    });
    tunnels.push(t.id);

    const sent = await c.sendCmd({
      device_id: dev.id,
      topic: "audit.probe",
      payload: { hello: zid },
    });

    const page = await c.listAudit({ limit: 50 });
    expect(page.items.length).toBeGreaterThan(0);

    // Goal-20 wire shape: every entry conforms to the openapi
    // `AuditEntry` projection. Lock it in here so any regression
    // (server stops projecting, TS type drifts) trips immediately.
    for (const e of page.items) {
      expect(typeof e.id).toBe("number");
      expect(typeof e.at).toBe("number");
      expect(e.actor_user_id === null || typeof e.actor_user_id === "number").toBe(true);
      expect(typeof e.action).toBe("string");
      expect(typeof e.subject).toBe("string");
      expect(e.detail === null || (typeof e.detail === "object" && !Array.isArray(e.detail))).toBe(true);
    }

    const detailHas = (e: { detail: Record<string, unknown> | null }, needle: string) =>
      e.detail != null && JSON.stringify(e.detail).includes(needle);

    const deviceCreate = page.items.find(
      (e) => e.action === "device.create" && detailHas(e, zid),
    );
    expect(deviceCreate, `no device.create row for zid=${zid}`).toBeDefined();
    // The new audit rows from goal 16 deliberately carry the new
    // entity id in `detail`, not the FK column (so the FK doesn't
    // pin a still-live id and block its own delete). The projection
    // therefore falls back to `user:<actor>` here. Subject must be
    // non-empty either way.
    expect(deviceCreate?.subject.length).toBeGreaterThan(0);

    const tunnelCreate = page.items.find(
      (e) =>
        e.action === "tunnel.create" &&
        detailHas(e, `audit-${dev.id}.example.invalid`),
    );
    expect(tunnelCreate, `no tunnel.create row for tunnel ${t.id}`).toBeDefined();

    const cmdSend = page.items.find(
      (e) => e.action === "cmd.send" && detailHas(e, sent.cmd_id),
    );
    expect(cmdSend, `no cmd.send row for cmd ${sent.cmd_id}`).toBeDefined();
  });
});
