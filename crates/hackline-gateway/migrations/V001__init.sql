-- Initial schema. Mirrors DOCS/DATABASE.md exactly; if the two
-- diverge, fix the doc or the migration in the same PR.

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
CREATE INDEX audit_ts     ON audit(ts);
CREATE INDEX audit_device ON audit(device_id);
