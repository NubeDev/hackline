import { inject } from "vitest";
import { HttpApiClient } from "../src/http-client";

export function harness() {
  return inject("hacklineHarness");
}

export function freshClient(token?: string): HttpApiClient {
  const h = harness();
  return new HttpApiClient({ baseUrl: h.baseUrl, token: token ?? h.token });
}

let zidCounter = 0;
export function uniqueZid(): string {
  // Zenoh ZIDs are short hex; the gateway only enforces non-empty
  // here, so a per-process monotonic counter plus a random suffix
  // is enough to avoid collisions across tests in the same run.
  zidCounter += 1;
  const rand = Math.floor(Math.random() * 0xffff)
    .toString(16)
    .padStart(4, "0");
  return `t${zidCounter.toString(16).padStart(4, "0")}${rand}`;
}

export async function deleteDeviceQuiet(id: number): Promise<void> {
  try {
    await freshClient().deleteDevice(id);
  } catch {
    // best-effort cleanup; the test that owns the resource has
    // already asserted what it cared about.
  }
}

export async function deleteTunnelQuiet(id: number): Promise<void> {
  try {
    await freshClient().deleteTunnel(id);
  } catch {
    // see deleteDeviceQuiet
  }
}
