import { describe, expect, it } from "vitest";
import { ApiError } from "../src/client";
import { HttpApiClient } from "../src/http-client";
import { freshClient, harness } from "./helpers";

describe("errors", () => {
  it("bad bearer → ApiError(401)", async () => {
    const c = new HttpApiClient({
      baseUrl: harness().baseUrl,
      token: "definitely-not-a-real-token",
    });
    let caught: unknown = null;
    try {
      await c.listDevices();
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeInstanceOf(ApiError);
    expect((caught as ApiError).status).toBe(401);
  });

  it("missing record → ApiError(404)", async () => {
    const c = freshClient();
    let caught: unknown = null;
    try {
      // 2^31 - 1 is well past anything the test run could allocate.
      await c.getDevice(2147483647);
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeInstanceOf(ApiError);
    expect((caught as ApiError).status).toBe(404);
  });
});
