-- Message-plane durable command outbox (SCOPE.md §7.2). Cloud
-- writes, gateway delivers via Zenoh, device acks. Bounded by a
-- per-device row cap and a TTL; both are enforced at write time
-- inside the same transaction as the insert so a reader never sees
-- the table over-cap.

CREATE TABLE cmd_outbox (
  id           INTEGER PRIMARY KEY,
  cmd_id       TEXT    NOT NULL UNIQUE,
  device_id    INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
  topic        TEXT    NOT NULL,
  content_type TEXT    NOT NULL DEFAULT 'application/json',
  payload      BLOB    NOT NULL,
  enqueued_at  INTEGER NOT NULL,
  expires_at   INTEGER NOT NULL,
  delivered_at INTEGER,
  ack_at       INTEGER,
  ack_result   TEXT    CHECK (ack_result IN ('accepted','rejected','failed','done')),
  ack_detail   TEXT,
  attempts     INTEGER NOT NULL DEFAULT 0,
  last_error   TEXT,
  CHECK (length(payload) <= 65536)
);
CREATE INDEX cmd_outbox_pending
  ON cmd_outbox(device_id, enqueued_at)
  WHERE delivered_at IS NULL;
CREATE INDEX cmd_outbox_device ON cmd_outbox(device_id, id);
