import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, openSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "node:net";
import type { TestProject } from "vitest/node";

// Real-binary loopback harness for `@hackline/client`. Spawns the
// `hackline-gateway` `serve` binary against an ephemeral SQLite DB
// and a loopback Zenoh endpoint, claims the gateway over `/v1/claim`,
// and publishes `{ baseUrl, token, tempDir }` to test workers via
// vitest's `provide` channel. Per the no-mock policy (goal 14):
// no mocks, no stubs, no fixtures — the binary the tests drive is
// the same binary the operator runs in production.

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(HERE, "../../..");
const BIN_PATH = join(REPO_ROOT, "target/debug/serve");

async function getFreePort(): Promise<number> {
  return new Promise((resolveP, rejectP) => {
    const srv = createServer();
    srv.unref();
    srv.on("error", rejectP);
    srv.listen(0, "127.0.0.1", () => {
      const addr = srv.address();
      if (!addr || typeof addr === "string") {
        srv.close();
        rejectP(new Error("getFreePort: no address"));
        return;
      }
      const port = addr.port;
      srv.close(() => resolveP(port));
    });
  });
}

function ensureBinary(): void {
  if (existsSync(BIN_PATH)) return;
  // First-run build. `cargo build` is idempotent and reuses the
  // workspace's incremental cache, so subsequent test runs just hit
  // the existsSync above.
  const r = spawnSync(
    "cargo",
    ["build", "-p", "hackline-gateway", "--bin", "serve"],
    { cwd: REPO_ROOT, stdio: "inherit" },
  );
  if (r.status !== 0) {
    throw new Error(`cargo build failed (exit ${r.status})`);
  }
  if (!existsSync(BIN_PATH)) {
    throw new Error(`expected ${BIN_PATH} after build`);
  }
}

async function waitForHealth(baseUrl: string, deadlineMs: number): Promise<void> {
  const start = Date.now();
  let lastErr: unknown = null;
  while (Date.now() - start < deadlineMs) {
    try {
      const res = await fetch(`${baseUrl}/v1/health`);
      if (res.ok) {
        await res.text().catch(() => "");
        return;
      }
      lastErr = new Error(`health status ${res.status}`);
    } catch (err) {
      lastErr = err;
    }
    await new Promise((r) => setTimeout(r, 150));
  }
  throw new Error(`gateway did not become healthy within ${deadlineMs}ms: ${String(lastErr)}`);
}

async function readClaimToken(logFile: string, deadlineMs: number): Promise<string> {
  // The gateway writes "  CLAIM TOKEN: <token>" on stdout during
  // first-boot, which `serve.rs` does *before* it starts listening.
  // We poll the log file rather than the stream because the stdout
  // pipe is owned by the spawned child for its lifetime.
  const start = Date.now();
  const fs = await import("node:fs/promises");
  while (Date.now() - start < deadlineMs) {
    try {
      const text = await fs.readFile(logFile, "utf8");
      const m = text.match(/CLAIM TOKEN:\s+(\S+)/);
      if (m) return m[1];
    } catch {
      // log file may not exist yet
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`no CLAIM TOKEN line in ${logFile} within ${deadlineMs}ms`);
}

let child: ChildProcess | null = null;
let tempDir: string | null = null;

export default async function setup(project: TestProject): Promise<() => Promise<void>> {
  ensureBinary();

  const restPort = await getFreePort();
  const zenohPort = await getFreePort();

  tempDir = mkdtempSync(join(tmpdir(), "hackline-it-"));
  mkdirSync(tempDir, { recursive: true });

  const dbPath = join(tempDir, "gateway.db");
  const configPath = join(tempDir, "gateway.toml");
  const logPath = join(tempDir, "gateway.log");

  const config = [
    `listen = "127.0.0.1:${restPort}"`,
    `database = "${dbPath}"`,
    ``,
    `[zenoh]`,
    `mode = "peer"`,
    `listen = ["tcp/127.0.0.1:${zenohPort}"]`,
    ``,
    `[log]`,
    `level = "warn"`,
    `format = "pretty"`,
    ``,
  ].join("\n");
  writeFileSync(configPath, config);

  // Pipe stdout/stderr to the log file so we can both capture the
  // claim token and surface the tail on failure without fighting
  // node's stream handling.
  const out = openSync(logPath, "a");
  child = spawn(BIN_PATH, [configPath], {
    cwd: tempDir,
    stdio: ["ignore", out, out],
    env: {
      ...process.env,
      RUST_LOG: process.env.RUST_LOG ?? "warn",
    },
    detached: false,
  });

  const exitPromise = new Promise<number | null>((res) => {
    child!.once("exit", (code) => res(code));
  });

  const baseUrl = `http://127.0.0.1:${restPort}`;
  const claimToken = await Promise.race([
    readClaimToken(logPath, 15_000),
    exitPromise.then((code) => {
      throw new Error(`gateway exited (code=${code}) before printing claim token`);
    }),
  ]);

  await waitForHealth(baseUrl, 10_000);

  const claimRes = await fetch(`${baseUrl}/v1/claim`, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify({ token: claimToken, name: "test-owner" }),
  });
  if (!claimRes.ok) {
    const body = await claimRes.text().catch(() => "");
    throw new Error(`claim failed: HTTP ${claimRes.status} ${body}`);
  }
  const claimBody = (await claimRes.json()) as { token: string };
  if (!claimBody.token) {
    throw new Error(`claim response missing token: ${JSON.stringify(claimBody)}`);
  }

  project.provide("hacklineHarness", {
    baseUrl,
    token: claimBody.token,
    tempDir,
    logPath,
  });

  return async () => {
    if (child) {
      child.kill("SIGTERM");
      const exited = await Promise.race([
        new Promise<boolean>((res) => child!.once("exit", () => res(true))),
        new Promise<boolean>((res) => setTimeout(() => res(false), 2_000)),
      ]);
      if (!exited) child.kill("SIGKILL");
    }
    if (tempDir) {
      rmSync(tempDir, { recursive: true, force: true });
    }
  };
}

declare module "vitest" {
  export interface ProvidedContext {
    hacklineHarness: {
      baseUrl: string;
      token: string;
      tempDir: string;
      logPath: string;
    };
  }
}
