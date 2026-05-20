# goal 47 — weekly cron of the security gates

## context

`ci.yml` runs `cargo audit` and `cargo deny` on every push and PR.
that catches advisories introduced by *lockfile changes*. it does
not catch advisories the RustSec database adds *after* the last
push: between two code changes, weeks can pass in which a fresh
RUSTSEC against unchanged pins is invisible to CI.

dropping a `schedule:` trigger onto `ci.yml` would re-run the full
gate (clippy + doc + js builds + integration tests) weekly, which
is wasteful and would race the normal push-triggered run when a
cron fires during active development.

## changes

new `.github/workflows/security.yml`:

- triggers on `schedule: cron "0 9 * * 1"` (Mondays 09:00 UTC) and
  `workflow_dispatch` (manual button in the Actions tab).
- one job, `audit-deny`, that runs only the two security checks
  against a fresh checkout + a separately-keyed Swatinem cache.
- `concurrency` group cancels a stale cron if a manual dispatch
  fires while it is still queued.

no changes to `ci.yml`: the push-time security gates remain there
unchanged; this workflow is purely a time-driven backstop.

## verification

- yaml validity verified by hand against the format used in
  `ci.yml`.
- no local runtime verification possible (cron triggers are a
  GitHub Actions concern). first scheduled run will land on the
  Monday after merge; manual dispatch is available immediately.

## what's next

- `cargo fmt --check` — still deferred (needs user input).
- track external deps for the three open advisory ignores
  (lz4_flex, paste, rustls-pemfile).
- consider extending the cron job to additionally run `cargo
  update --dry-run` and post a diff issue when meaningful updates
  are available (out of scope here — would require a bot token
  and an issue-template).
