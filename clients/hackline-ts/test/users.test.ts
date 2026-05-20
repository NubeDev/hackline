import { describe, expect, it } from "vitest";
import { ApiError } from "../src/client";
import { HttpApiClient } from "../src/http-client";
import { freshClient, harness } from "./helpers";

describe("users", () => {
  it("owner mints a scoped user; minted token works; revocation flips to 401", async () => {
    const owner = freshClient();
    // The gateway responds to POST /v1/users with { user, token } —
    // the package types this as `User` for legacy reasons. Cast to
    // the actual server shape rather than refactoring the client
    // (out of scope for goal 15).
    const created = (await owner.createUser({
      name: "scoped-test-user",
      role: "operator",
    })) as unknown as {
      user: { id: number; name: string; role: string };
      token: string;
    };
    expect(created.user.role).toBe("operator");
    expect(typeof created.token).toBe("string");
    expect(created.token.length).toBeGreaterThan(8);

    const scoped = new HttpApiClient({
      baseUrl: harness().baseUrl,
      token: created.token,
    });
    const list = await scoped.listDevices();
    expect(Array.isArray(list)).toBe(true);

    await owner.deleteUser(created.user.id);

    let caught: unknown = null;
    try {
      await scoped.listDevices();
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeInstanceOf(ApiError);
    expect((caught as ApiError).status).toBe(401);
  });

  // Goal-19 lock-in: `POST /v1/users/:id/tokens` returns the openapi
  // `TokenMinted` shape, which is `{ token: string }` — no
  // `expires_at`. The prior TS type fabricated `expires_at:
  // string|null`; this test asserts the wire never grew that field.
  it("mintToken returns { token } only — no fabricated expires_at", async () => {
    const owner = freshClient();
    const created = (await owner.createUser({
      name: "mint-shape-test",
      role: "operator",
    })) as unknown as { user: { id: number } };

    const minted = await owner.mintToken(created.user.id);
    const raw = minted as unknown as Record<string, unknown>;
    expect(typeof raw.token).toBe("string");
    expect((raw.token as string).length).toBeGreaterThan(8);
    expect(raw).not.toHaveProperty("expires_at");
    expect(Object.keys(raw).sort()).toEqual(["token"]);

    await owner.deleteUser(created.user.id);
  });
});
