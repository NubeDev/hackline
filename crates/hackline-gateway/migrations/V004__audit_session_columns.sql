-- Phase 3 audit completeness (SCOPE.md §7.2). `tunnel.session` rows
-- need to carry the per-bridged-connection counters; the original
-- V001 schema only had `ts` + `action` + `detail`. Add the missing
-- columns so a single row spans open + close instead of two rows or
-- a JSON `detail` blob that the admin UI would have to re-parse.
--
-- Columns are NULLABLE so every existing point-in-time action
-- (`cmd.send`, `auth.login`, etc.) keeps inserting just `ts` +
-- `action` + `detail` and leaves the session-specific columns blank.

ALTER TABLE audit ADD COLUMN ts_close   INTEGER;
ALTER TABLE audit ADD COLUMN request_id TEXT;
ALTER TABLE audit ADD COLUMN peer       TEXT;
ALTER TABLE audit ADD COLUMN bytes_up   INTEGER;
ALTER TABLE audit ADD COLUMN bytes_down INTEGER;

CREATE INDEX audit_request ON audit(request_id);
