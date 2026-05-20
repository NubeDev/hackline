// Bearer token persistence. Single-tenant trust boundary (matches the
// codeless model in workspace `CLAUDE.md` R5): one token authorises
// the whole UI for the current user. Stored in localStorage so a page
// reload doesn't kick the operator back to the claim screen.

const KEY = "hackline-ui-token";
const BASE_KEY = "hackline-ui-base-url";

export function readToken(): string | null {
  try {
    return window.localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

export function writeToken(token: string | null): void {
  try {
    if (token == null || token.length === 0) {
      window.localStorage.removeItem(KEY);
    } else {
      window.localStorage.setItem(KEY, token);
    }
  } catch {
    // localStorage may be unavailable (private mode quotas, etc.).
    // The UI keeps working in-memory until reload; nothing to do.
  }
}

// Same-origin in dev (Vite proxy) and prod (gateway serves the bundle).
// `?server=` query param overrides for pointing the dev UI at a
// remote gateway without rebuilding.
export function readBaseUrl(): string {
  try {
    const stored = window.localStorage.getItem(BASE_KEY);
    if (stored) return stored.replace(/\/$/, "");
    const fromQuery = new URLSearchParams(window.location.search).get("server");
    if (fromQuery) return fromQuery.replace(/\/$/, "");
  } catch {
    // ignore — fall through to default
  }
  return window.location.origin;
}

export function writeBaseUrl(url: string | null): void {
  try {
    if (url == null || url.length === 0) {
      window.localStorage.removeItem(BASE_KEY);
    } else {
      window.localStorage.setItem(BASE_KEY, url.replace(/\/$/, ""));
    }
  } catch {
    // ignore
  }
}
