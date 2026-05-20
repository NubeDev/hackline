// Public surface of `@hackline/client`. The React-only `provider`
// helper lives in the consumer (`hackline-ui`) so the package itself
// stays framework-agnostic and carries no React peer dependency.
//
// Wire types generated from `hackline-proto` are exported on the
// `./wire` subpath rather than re-exported here, to keep the two
// surfaces (REST + SSE control plane vs Zenoh-side wire) clearly
// labelled until they are reconciled.
//
// No-mock policy: this package ships only real-transport clients.
// Tests and dev runs go against a real gateway (loopback Zenoh
// router for E2E). See `DOCS/CODEBASE-ANALYSIS.md` and
// `DOCS/DEVELOPMENT.md`.

export { ApiError, type ApiClient } from "./client";
export { HttpApiClient, type HttpApiClientOptions } from "./http-client";
export { readBaseUrl, readToken, writeBaseUrl, writeToken } from "./token";
export type * from "./types";
