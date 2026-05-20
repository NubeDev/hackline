import { afterEach, describe, expect, it } from "vitest";
import { deleteDeviceQuiet, freshClient, uniqueZid } from "./helpers";

describe("devices", () => {
  const created: number[] = [];

  afterEach(async () => {
    while (created.length) {
      const id = created.pop()!;
      await deleteDeviceQuiet(id);
    }
  });

  it("create + list + get + delete round-trip", async () => {
    const c = freshClient();
    const zid = uniqueZid();
    const dev = await c.createDevice({ zid, label: "round-trip" });
    created.push(dev.id);

    expect(dev.zid).toBe(zid);
    expect(dev.label).toBe("round-trip");
    expect(typeof dev.id).toBe("number");

    const list = await c.listDevices();
    expect(Array.isArray(list)).toBe(true);
    const found = list.find((d) => d.id === dev.id);
    expect(found?.zid).toBe(zid);

    const got = await c.getDevice(dev.id);
    expect(got.id).toBe(dev.id);
    expect(got.zid).toBe(zid);

    await c.deleteDevice(dev.id);
    created.pop();

    const after = await c.listDevices();
    expect(after.find((d) => d.id === dev.id)).toBeUndefined();
  });

  it("getDeviceHealth returns the openapi shape for an offline device", async () => {
    const c = freshClient();
    const zid = uniqueZid();
    const dev = await c.createDevice({ zid, label: "health-test" });
    created.push(dev.id);

    const h = await c.getDeviceHealth(dev.id);
    expect(h).toEqual({
      online: false,
      last_seen_at: null,
      rtt_ms: null,
    });
  });

  // Goal 28/29: collection-level health endpoint returns one entry
  // per device in the org. Two devices proves "this is a list,
  // not a short-circuit"; filtering by `device_id` keeps the test
  // stable when other tests in the same vitest run leave devices
  // around.
  it("getDevicesHealth returns one entry per device with the offline shape", async () => {
    const c = freshClient();
    const a = await c.createDevice({ zid: uniqueZid(), label: "list-health-a" });
    created.push(a.id);
    const b = await c.createDevice({ zid: uniqueZid(), label: "list-health-b" });
    created.push(b.id);

    const all = await c.getDevicesHealth();
    const byId = new Map(all.map((e) => [e.device_id, e]));
    expect(byId.get(a.id)).toEqual({
      device_id: a.id,
      online: false,
      last_seen_at: null,
      rtt_ms: null,
    });
    expect(byId.get(b.id)).toEqual({
      device_id: b.id,
      online: false,
      last_seen_at: null,
      rtt_ms: null,
    });
  });

  // Lock-in for goal 17: the canonical wire field is `last_seen_at`
  // (number | null, unix epoch seconds) per `DOCS/openapi.yaml`. The
  // prior TS type called it `last_seen_ts: string | null`, which lied
  // on both name and type. If a future refactor reintroduces either
  // mistake, this test fails before any UI consumer notices.
  it("Device wire shape: last_seen_at is number|null, never a string", async () => {
    const c = freshClient();
    const zid = uniqueZid();
    const dev = await c.createDevice({ zid, label: "shape-test" });
    created.push(dev.id);

    const list = await c.listDevices();
    const found = list.find((d) => d.id === dev.id);
    expect(found).toBeDefined();
    const raw = found as unknown as Record<string, unknown>;

    expect(raw).not.toHaveProperty("last_seen_ts");
    expect(raw).toHaveProperty("last_seen_at");
    const ls = raw.last_seen_at;
    expect(ls === null || typeof ls === "number").toBe(true);
    expect(typeof ls).not.toBe("string");

    expect(typeof raw.id).toBe("number");
    expect(typeof raw.zid).toBe("string");
    expect(typeof raw.label).toBe("string");
    expect(raw.customer_id === null || typeof raw.customer_id === "number").toBe(
      true,
    );
    expect(typeof raw.created_at).toBe("number");
  });
});
