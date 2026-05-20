import { describe, expect, it } from "vitest";
import { freshClient } from "./helpers";

describe("health", () => {
  it("GET /v1/health returns { status: 'ok' }", async () => {
    const c = freshClient();
    const res = await c.health();
    expect(res).toEqual({ status: "ok" });
  });

  it("GET /v1/claim/status reports already-claimed", async () => {
    const c = freshClient();
    const status = await c.claimStatus();
    expect(status.claimed).toBe(true);
    expect(status.can_claim).toBe(false);
  });
});
