-- Phase 4 multi-tenant orgs (SCOPE.md §13 Phase 4). Every device and
-- user now belongs to exactly one org; cross-org isolation is enforced
-- in REST handlers and (via the per-org keyexpr prefix) on the Zenoh
-- fabric.
--
-- A single default org is created here so the existing rows have a
-- home. SCOPE.md §6.1's claim flow inserts a fresh org from the
-- supplied `org_slug` (or `default` if omitted) — see
-- `crates/hackline-gateway/src/db/orgs.rs`.

CREATE TABLE orgs (
  id         INTEGER PRIMARY KEY,
  slug       TEXT    NOT NULL UNIQUE
               CHECK (length(slug) BETWEEN 1 AND 63
                      AND slug GLOB '[a-z0-9][a-z0-9-]*'),
  name       TEXT    NOT NULL,
  created_at INTEGER NOT NULL
);

INSERT INTO orgs (id, slug, name, created_at) VALUES (1, 'default', 'default', unixepoch());

ALTER TABLE users   ADD COLUMN org_id INTEGER NOT NULL DEFAULT 1 REFERENCES orgs(id);
ALTER TABLE devices ADD COLUMN org_id INTEGER NOT NULL DEFAULT 1 REFERENCES orgs(id);

CREATE INDEX users_org_id   ON users(org_id);
CREATE INDEX devices_org_id ON devices(org_id);
