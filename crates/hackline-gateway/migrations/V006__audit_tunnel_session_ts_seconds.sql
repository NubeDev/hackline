-- Forward-only backfill for the goal-22 fix in
-- `src/db/audit.rs`. Before goal 22, the bridge code stored
-- `tunnel.session` audit rows with `ts` / `ts_close` in
-- *milliseconds* while every other audit row used seconds. The
-- goal-20 REST projection at `GET /v1/audit` documents
-- `AuditEntry.at` as unix epoch seconds, so any pre-fix
-- `tunnel.session` row renders ~55,000 years in the future under
-- the UI's `relTime`.
--
-- Heuristic: divide by 1000 only when the value is large enough
-- that it cannot already be seconds. A seconds-shaped epoch
-- crossing 10_000_000_000 represents the year 2286, so any
-- `audit.ts` greater than that threshold today is necessarily a
-- millisecond stamp from the pre-fix code path. The same
-- threshold is safe for `audit.ts_close`.
--
-- Scoped to `action = 'tunnel.session'` so unrelated rows are not
-- touched even if a future bug ever stamps a non-tunnel row in
-- ms; keeps the blast radius minimal.

UPDATE audit
   SET ts = ts / 1000
 WHERE action = 'tunnel.session'
   AND ts > 10000000000;

UPDATE audit
   SET ts_close = ts_close / 1000
 WHERE action = 'tunnel.session'
   AND ts_close IS NOT NULL
   AND ts_close > 10000000000;
