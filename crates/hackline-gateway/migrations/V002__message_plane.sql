-- Message-plane storage: device-side events and structured logs.
-- Both are bounded ring buffers per device; the oldest row is pruned
-- in the same transaction as the insert (SCOPE.md §7.2, §7.3).

CREATE TABLE events (
  id           INTEGER PRIMARY KEY,
  device_id    INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
  topic        TEXT    NOT NULL,
  ts           INTEGER NOT NULL,
  content_type TEXT    NOT NULL DEFAULT 'application/json',
  payload      BLOB    NOT NULL,
  CHECK (length(payload) <= 65536)
);
CREATE INDEX events_device_ts ON events(device_id, ts);
CREATE INDEX events_device_id ON events(device_id, id);

CREATE TABLE logs (
  id           INTEGER PRIMARY KEY,
  device_id    INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
  topic        TEXT    NOT NULL,
  ts           INTEGER NOT NULL,
  level        TEXT    NOT NULL CHECK (level IN ('trace','debug','info','warn','error')),
  content_type TEXT    NOT NULL DEFAULT 'application/json',
  payload      BLOB    NOT NULL,
  CHECK (length(payload) <= 65536)
);
CREATE INDEX logs_device_ts ON logs(device_id, ts);
CREATE INDEX logs_device_id ON logs(device_id, id);
