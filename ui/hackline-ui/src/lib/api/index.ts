// Thin re-export shim. The actual client lives in `@hackline/client`
// (workspace-linked). Keeping the `@/lib/api` import path means the
// UI's call sites stay byte-identical and a future cleanup can inline
// the package import without entangling the extraction history.
//
// The React-only `ApiProvider` / `useApi` stay here because the
// package itself is framework-agnostic.

export {
  ApiError,
  HttpApiClient,
  readBaseUrl,
  readToken,
  writeBaseUrl,
  writeToken,
  type ApiClient,
  type HttpApiClientOptions,
} from "@hackline/client";
export type * from "@hackline/client";
export { ApiProvider, useApi } from "./provider";
