-- Goal 24: turn `audit.user_id` / `audit.device_id` /
-- `audit.tunnel_id` into `ON DELETE SET NULL` foreign keys.
--
-- The original V001 schema gave each FK no ON DELETE clause, which
-- under `PRAGMA foreign_keys = ON` (set per-connection in
-- `db/pool.rs`) means *any* DELETE on the parent row fails the
-- moment a single audit row references it. In practice this means
-- a tunnel or device with operational history can never be
-- removed — `DELETE FROM tunnels WHERE id = ?` returns an FK
-- violation as soon as a `tunnel.session` row exists. The handlers
-- in `api/{tunnels,devices}/delete.rs` already work around this by
-- stuffing the soon-to-be-orphaned id into the audit `detail` JSON
-- and inserting the post-delete audit row with FK columns set to
-- NULL, but the workaround does not help pre-existing audit rows
-- whose FKs still pin the parent.
--
-- ON DELETE SET NULL keeps the audit row intact (history is
-- forever) while letting the parent go. The goal-20 audit
-- projection at `GET /v1/audit` already tolerates NULL FKs by
-- emitting an empty `subject`, and the delete handlers already
-- carry the original id in `detail`, so the wire shape does not
-- change.
--
-- SQLite has no `ALTER TABLE ... ALTER CONSTRAINT`. The supported
-- procedure (https://www.sqlite.org/lang_altertable.html §7) is the
-- 12-step recreate dance: disable FKs (only allowed *outside* a
-- transaction), open a transaction, create the new table, copy
-- rows, drop the old table, rename, recreate indexes, FK-check,
-- commit, re-enable FKs.

PRAGMA foreign_keys = OFF;

BEGIN TRANSACTION;

CREATE TABLE audit_new (
  id          INTEGER PRIMARY KEY,
  ts          INTEGER NOT NULL,
  user_id     INTEGER REFERENCES users(id)   ON DELETE SET NULL,
  device_id   INTEGER REFERENCES devices(id) ON DELETE SET NULL,
  tunnel_id   INTEGER REFERENCES tunnels(id) ON DELETE SET NULL,
  action      TEXT    NOT NULL,
  detail      TEXT,
  ts_close    INTEGER,
  request_id  TEXT,
  peer        TEXT,
  bytes_up    INTEGER,
  bytes_down  INTEGER
);

INSERT INTO audit_new
  (id, ts, user_id, device_id, tunnel_id, action, detail,
   ts_close, request_id, peer, bytes_up, bytes_down)
SELECT
  id, ts, user_id, device_id, tunnel_id, action, detail,
  ts_close, request_id, peer, bytes_up, bytes_down
FROM audit;

DROP TABLE audit;
ALTER TABLE audit_new RENAME TO audit;

CREATE INDEX audit_ts      ON audit(ts);
CREATE INDEX audit_device  ON audit(device_id);
CREATE INDEX audit_request ON audit(request_id);

-- Final integrity check before commit, per SQLite recreate
-- procedure step 11. If any row in the new table violates a
-- declared FK, the transaction rolls back instead of leaving the
-- DB in an inconsistent state once FKs are re-enabled.
PRAGMA foreign_key_check;

COMMIT;

PRAGMA foreign_keys = ON;
