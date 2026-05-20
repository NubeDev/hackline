# Database

SQLite via `rusqlite` (bundled). One writer (the gateway), `r2d2`
pool for readers, `refinery` for migrations. WAL mode enabled at
open.

Repository code: one file per table under
[`crates/hackline-gateway/src/db/`](../crates/hackline-gateway/src/db/).
Migrations: [`crates/hackline-gateway/migrations/`](../crates/hackline-gateway/migrations/).

## Tables (v0.1)

```sql
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE claim_pending (
  id          INTEGER PRIMARY KEY CHECK (id = 1),
  token_hash  TEXT NOT NULL,
  created_at  INTEGER NOT NULL
);

CREATE TABLE users (
  id              INTEGER PRIMARY KEY,
  name            TEXT    NOT NULL,
  role            TEXT    NOT NULL,
  token_hash      TEXT    NOT NULL UNIQUE,
  device_scope    TEXT    NOT NULL DEFAULT '*',
  tunnel_scope    TEXT    NOT NULL DEFAULT '*',
  expires_at      INTEGER,
  created_at      INTEGER NOT NULL,
  last_used_at    INTEGER
);

CREATE TABLE devices (
  id           INTEGER PRIMARY KEY,
  zid          TEXT    NOT NULL UNIQUE,
  label        TEXT    NOT NULL,
  customer_id  INTEGER,
  created_at   INTEGER NOT NULL,
  last_seen_at INTEGER
);

CREATE TABLE tunnels (
  id              INTEGER PRIMARY KEY,
  device_id       INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
  kind            TEXT    NOT NULL,
  local_port      INTEGER NOT NULL,
  public_hostname TEXT,
  public_port     INTEGER,
  enabled         INTEGER NOT NULL DEFAULT 1,
  created_at      INTEGER NOT NULL,
  UNIQUE (public_hostname),
  UNIQUE (public_port),
  CHECK (
    (kind = 'http' AND public_hostname IS NOT NULL AND public_port IS NULL) OR
    (kind = 'tcp'  AND public_port     IS NOT NULL AND public_hostname IS NULL)
  )
);

CREATE TABLE audit (
  id          INTEGER PRIMARY KEY,
  ts          INTEGER NOT NULL,
  user_id     INTEGER REFERENCES users(id),
  device_id   INTEGER REFERENCES devices(id),
  tunnel_id   INTEGER REFERENCES tunnels(id),
  action      TEXT    NOT NULL,
  detail      TEXT
);
CREATE INDEX audit_ts        ON audit(ts);
CREATE INDEX audit_device    ON audit(device_id);
```

The `tunnels` `CHECK` constraint enforces the kind ↔ which-column-is-non-null
invariant; without it the unique constraints behave inconsistently
because SQLite treats `NULL` as distinct.

## Audit retention

`audit` grows unbounded if untreated. A fleet doing thousands of
HTTP connections per device per day will reach hundreds of millions
of rows in a year. Two strategies, in order of preference:

1. Per-tunnel-session rows (open / close + byte counts) instead of
   per-connection. Reduces row count by ~3 orders of magnitude.
2. Time-based retention: vacuum rows older than N days, on a daily
   cron in the gateway. N defaults to 90, configurable.

Pick a strategy in the same PR that adds the first heavy audit
producer.

## Pool sizing

`r2d2` is sync; gateway calls it from async via
`tokio::task::spawn_blocking`. **Pool max size must be ≤ tokio's
blocking-thread budget** (default 512). Setting `max_size(10000)`
deadlocks the runtime under load.
