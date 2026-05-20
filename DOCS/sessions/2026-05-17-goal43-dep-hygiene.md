# goal 43 — dependency hygiene pass

## context

goal 42 promoted `cargo deny check` to a CI gate with
`multiple-versions = "warn"`. that warn level produces 17 distinct
duplicate-version warnings on the current graph. this goal investigates
whether any of those are deduplicable and freshens the lockfile.

## findings

### duplicate inventory (from `cargo tree -d`)

structural duplicates currently locked in by upstream caret ranges:

| crate         | versions                  | reason (transitive owner)                              |
| ------------- | ------------------------- | ------------------------------------------------------ |
| cpufeatures   | 0.2 / 0.3                 | rustls 0.23 chain vs sha2 0.10 chain                   |
| foldhash      | 0.1 / 0.2                 | hashbrown 0.15 vs 0.16/0.17                            |
| getrandom     | 0.2 / 0.3 / 0.4           | rand 0.8/0.9/0.10 split (zenoh pins 0.8)               |
| hashbrown     | 0.14 / 0.15 / 0.16 / 0.17 | zenoh, indexmap 2.x, sqlx, several others              |
| rand          | 0.8 / 0.9 / 0.10          | same family split as getrandom                         |
| rand_chacha   | 0.3 / 0.9                 | follows rand                                           |
| rand_core     | 0.6 / 0.9 / 0.10          | follows rand                                           |
| socket2       | 0.5 / 0.6                 | tokio 1.x vs reqwest's hyper newer chain               |
| spin          | 0.9 / 0.10                | ring 0.16 (zenoh) vs ring 0.17                         |
| syn           | 1 / 2                     | proc-macro crates; pervasive, no dedupe possible       |
| winnow        | 0.7 / 1.0                 | toml vs serde_with                                     |
| core-foundation, r-efi, toml_datetime, toml_edit, windows-sys, wit-bindgen | 2× each | platform/transitive splits   |

every one of these is gated by a caret range owned by a crate we do
not control. forcing a single version with `cargo update --precise`
fails resolution; the only real fix is for upstream (predominantly
zenoh-transport 1.9 and its rand 0.8 / hashbrown 0.14 / ring 0.16
dependencies) to bump, at which point the chain collapses naturally.

### actionable changes

- `cargo update` (within existing caret ranges) bumped:
  - `openssl 0.10.79 -> 0.10.80`
  - `openssl-sys 0.9.115 -> 0.9.116`
  - `winnow 1.0.2 -> 1.0.3`

  no duplicate counts changed; this is just routine lockfile freshening
  that brings in latest patch-level security fixes from the openssl
  family.

- 12 deps remain "behind latest" because their next bump is a
  semver-major; deferred to upstream coordination.

### policy decision

leave `multiple-versions = "warn"` (not deny). promoting it to deny
today would fail CI on transitive splits we cannot fix, which is the
wrong incentive. revisit when zenoh-transport publishes a release that
unifies its rand/hashbrown/ring trees.

## changes

- `Cargo.lock` — refreshed via `cargo update` (3 patches).
- this session doc.

## verification

- `cargo check --workspace` — ok.
- `cargo test --workspace` — all suites green.
- `cargo audit` (with .cargo/audit.toml ignores) — exit 0.
- `cargo deny check` — advisories ok, bans ok (17 dup warnings, all
  expected per inventory above), licenses ok, sources ok.

## what's next

- `cargo fmt --check` — long-deferred rustfmt-vs-handstyle decision.
  needs user input on whether to format the tree or accept the gate as
  not-in-scope permanently.
- track zenoh-transport for the lz4_flex 0.10 -> 0.11 bump (would let
  us drop `RUSTSEC-2026-0041` from audit/deny ignores).
- track sqlx-macros migrating off `paste` (would let us drop
  `RUSTSEC-2024-0436`).
- re-run this inventory after any major zenoh bump; many of the
  duplicates collapse the moment zenoh unblocks.
