# Auth

Bearer-token everywhere. Three roles in v0.1: `owner`, `admin`,
`support`, `viewer`, plus `customer` (Phase 2 only).

## Claim flow

1. Gateway boots with empty DB. It generates a 32-byte URL-safe
   **claim token**, stores `sha256(token)` in `claim_pending`, and
   prints the raw token once to stdout / journal.
2. The first caller to `POST /v1/claim { token, owner }` receives an
   **owner token**.
3. The claim row is deleted and the owner row is inserted in **one
   SQL transaction** — so two simultaneous claims cannot both win.
4. Subsequent claim attempts return `409 already_claimed`.

If the operator misses the boot log: `hackline-gateway print-claim`
prints the pending token (works only while pending). The hard reset
is `hackline-gateway reset-claim`.

## Token storage

- Server stores only `sha256(token)`. Constant-time compare via
  `subtle` on every check.
- Lookup is by indexed equality on `token_hash` (the hash leaks
  nothing useful, so a B-tree lookup is safe — do not "fix" this
  into a linear scan).
- Client cache: `$XDG_CONFIG_HOME/hackline/credentials.json`,
  mode `0600`.

## Scoped user tokens

Owner mints additional tokens via `POST /v1/users/:id/tokens`:

| Field | Values |
|---|---|
| `role` | `owner` / `admin` / `support` / `viewer` / `customer` |
| `device_scope` | `*` or list of device IDs |
| `tunnel_scope` | `*` or list of tunnel IDs |
| `expires_at` | optional unix timestamp |

A future `tokens` child table (one user, many active tokens) is
already in scope — see [`DATABASE.md`](./DATABASE.md). For v0.1 each
user has one current token rotated by re-mint.

## Trust concentration

The gateway is a single Zenoh principal authorised to query
`hackline/*/**`. **A compromised gateway therefore owns every
device's loopback.** This is by design; agent ACLs prevent
device→device lateral movement, not gateway→device. Operators must
treat the gateway host as the highest-value target on the fleet.
