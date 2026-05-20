# goal 45 — cargo-deny scans the all-features graph

## context

goal 42 enabled cargo-deny with `[graph] all-features = false`,
which scans only the default-feature dependency graph. the gateway's
`tls` feature is off by default, so the rustls / instant-acme /
hyper-rustls subtree was invisible to deny.

flipping `cargo deny --all-features check` surfaced two real issues
that ship with the gateway in production (tls is required for ACME
and serving HTTPS):

1. **RUSTSEC-2025-0134** — `rustls-pemfile` 2.2.0 unmaintained
   (archived Aug 2025). Pulled in by `instant-acme` →
   `hyper-rustls`. No drop-in replacement on the resolved version;
   upstream guidance is to migrate to `rustls-pki-types::PemObject`.
2. **license `CDLA-Permissive-2.0`** on `webpki-root-certs` 1.0.7.
   Mozilla's data licence for the bundled root-CA list; permissive,
   no copyleft. Not in our default `allow` list because we curate
   exact licences rather than blanket-permitting families.

cargo-audit (which doesn't honour feature flags — it scans the full
lockfile) already saw rustls-pemfile but treated it as a warning;
deny was silent because of the feature filter.

## changes

- `deny.toml`:
  - `[graph] all-features = true` — close the gap permanently;
    every advisory the production gateway ships under now lands in
    CI policy.
  - `[advisories].ignore` adds `RUSTSEC-2025-0134` with the same
    "track upstream" rationale used for the other two ignores.
  - `[[licenses.exceptions]]` adds `webpki-root-certs` allowing
    `CDLA-Permissive-2.0`. Per-crate exception rather than allowing
    CDLA broadly — the licence text is fine for Mozilla's root-CA
    data table but other CDLA-licensed crates would need their own
    review.
- `.cargo/audit.toml`: mirrors the two new advisories
  (`RUSTSEC-2024-0436`, `RUSTSEC-2025-0134`) that were in deny.toml
  but missing from audit.toml. deny.toml's comment says the two
  files are kept in sync by hand; this restores parity.

CI workflow needs no edit: the cargo-deny-action reads the global
`[graph]` block, so `all-features = true` takes effect without
adding a `--all-features` flag to the step.

## verification

- `cargo deny check` — advisories ok, bans ok, licenses ok,
  sources ok (against the all-features graph).
- `cargo audit` — exit 0, "2 allowed warnings found" (paste +
  rustls-pemfile), both ignored with rationale.
- `cargo check --workspace --all-targets`, `cargo test --workspace`,
  `cargo clippy -D warnings`, `cargo machete` — all still green.

## what's next

- `cargo fmt --check` — still long-deferred (needs user input).
- track `instant-acme` / `hyper-rustls` for the
  rustls-pki-types::PemObject migration → drop RUSTSEC-2025-0134
  ignore.
- track zenoh-transport for the lz4_flex bump.
- track sqlx-macros migrating off `paste`.
