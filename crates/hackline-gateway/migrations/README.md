# Migrations

Embedded `refinery` migrations. The runner in
[`src/db/migrations.rs`](../src/db/migrations.rs) embeds these at build
time; there is no migration CLI.

## Naming

`V<###>__<snake_case>.sql`. Numbers are dense and never reused.

## Rules

- Migrations are append-only. Never edit a migration that has shipped
  in a release.
- Schema-only here — seed data (the bootstrap claim row) is created in
  Rust on first boot.
- Every migration is reviewed against `DOCS/DATABASE.md`. If the
  shape diverges, the doc updates in the same PR.
