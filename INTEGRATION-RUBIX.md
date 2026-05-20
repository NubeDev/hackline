# hackline ↔ rubix integration

> Hackline is built **standalone** so its public surface stays honest.
> Rubix is the first consumer. This document captures the contract, the
> migration plan, and the rules that prevent the integration from
> sliding into a tangled in-tree fork.
>
> **This is load-bearing.** Violating any rule in §2 means we are
> heading back to where `com.rubix.fleet` ended up — see
> `rubix-agent/docs/design/fleet/FLEET-REMOVAL.md` for what that
> looks like.

---

## 1. Why this doc exists

Rubix already had a fleet transport. It started life as a core crate
inside rubix-agent (`transport-fleet-zenoh`, `Scope::Remote`,
`FleetRequestTransport`, `pathToSubject`, `sys.fleet.remote-agent`,
auth-context-on-fleet-handler, …). The fleet-transport code accreted
for months and in May 2026 the whole subsystem was **moved out** of
rubix-agent into the `com.rubix.fleet` extension. The extension's own
`FLEET-REMOVAL.md` records the verdict directly:

> The extension is not yet functional — the code is preserved for
> future re-integration. The design remains the intended architecture
> but is **not active** in the current codebase.

That is the failure mode this doc exists to prevent on the *next*
attempt.

The next attempt is hackline. Standalone, with a small public
surface, consumed by rubix the same way any third party would consume
it. If rubix can't get what it needs through `hackline-client` +
`hackline-proto`, the SDK is wrong — fix the SDK.

---

## 2. The four rules (non-negotiable)

These are the rules that make standalone strictly better than
in-tree. Drop any of them and the trade flips.

### R1. Rubix depends only on `hackline-client` and `hackline-proto`

Allowed in `rubix-agent/**/Cargo.toml`:

```toml
hackline-client = { path = "../../hackline/crates/hackline-client" }
hackline-proto  = { path = "../../hackline/crates/hackline-proto" }
```

**Forbidden:**

- `hackline-core` — internal bridging helpers.
- `hackline-gateway` — server-side, has no business on the device.
- `hackline-agent` — separate binary, not a library.

Enforced by a CI check in rubix that greps `Cargo.toml` files for the
forbidden names. Adding any of them requires a rubix-side ADR
explaining why `hackline-client` is insufficient and a hackline-side
ticket to widen the SDK.

### R2. The wire is the contract, not the SDK

Anything `hackline-client` does must also be doable by speaking the
keyexpr conventions in `hackline-proto` directly (raw Zenoh `get` /
`put` / `subscribe` / queryable). The SDK is sugar.

Consequence: rubix can never build a load-bearing dependency on a
Rust-only side door. The TS/Dart/C SDKs we'll add later see the same
surface.

Enforced by: every new `hackline-client` capability ships with a
"raw Zenoh" example in `hackline/examples/raw-zenoh/` proving the
wire suffices. If the example is impossible, the wire is wrong;
revise `hackline-proto` before merging the SDK feature.

### R3. Hackline dogfoods its own SDK before rubix touches it

`hackline/examples/` ships at minimum:

- `examples/echo-app/` — publishes `event/heartbeat`, serves
  `api/ping`, subscribes to `cmd/noop`. The minimum viable
  `hackline-client` consumer.
- `examples/raw-zenoh/` — same surface, raw Zenoh, proving R2.
- `examples/multi-session/` — two sessions in one process (mirrors
  what a real device runs: `hackline-agent` + a real app).

If any of these is gnarly to write, the SDK is wrong. Fix the SDK
before rubix ever opens its `Cargo.toml`.

### R4. Path-deps in dev, semver in CI

| Phase | rubix's `Cargo.toml` |
|---|---|
| Active iteration (now → end of hackline Phase 2) | `path = "../../hackline/crates/hackline-client"` |
| Once hackline ships `0.1.0` to crates.io | `version = "0.1"` |
| After rubix consumes `0.1.0` for one release cycle | path-dep is removed permanently |

Path-deps give cargo-workspace ergonomics during the period when
both repos churn together. The flip to semver pins the contract.

**Pre-1.0 semver discipline** (Cargo's pre-1.0 rules give weaker
guarantees than 1.x; we make them strict by convention):

| Bump | Rule |
|---|---|
| `0.1.x → 0.1.y` (patch) | No public-surface change at all. Bug fixes only. Cargo treats these as compatible — we must not break that. |
| `0.1.x → 0.2.0` (minor) | Additive surface changes only — new functions, new types, new variants on `#[non_exhaustive]` enums. Cargo treats `0.1` and `0.2` as incompatible, which forces a rubix-side migration commit. |
| pre-`1.0` removals/renames | Done at minor bumps with a deprecation in the prior release — not silently. |
| `1.0.0` | Ships when the surface has been stable for one full rubix release cycle with no minor bumps. |

We do **not** rely on Cargo's `^0.1` resolution to catch breaks for
us pre-1.0 — the discipline is in our minor-bump policy.

`hackline/.cargo/config.toml` does **not** patch in rubix paths;
rubix imports hackline, never the other way around. (Hackline
*does* pull `token-crypto` etc. from rubix-workspace by path —
that's a one-way dependency on a stable utility crate, not the same
shape.)

---

## 3. What gets deleted from rubix

When a hackline equivalent ships, the corresponding rubix code goes.
Not "kept around for compatibility." Deleted. Below is the audit, in
order of likely deletion.

### 3.1 Deleted at hackline Phase 1.5 (events SDK)

| rubix path | Replaced by |
|---|---|
| `agent/crates/transport-fleet-zenoh/` (entire crate) | `hackline-client::Session::publish_event` |
| `agent/crates/spi/src/fleet.rs` — `FleetTransport`, `FleetMessage`, `NullTransport` | `hackline_client::Session` directly; no abstraction layer |
| `agent/crates/spi/src/subject.rs` — `Subject` builder | `hackline_proto::Topic` + `hackline_proto::keyexpr::*` |
| `com.rubix.fleet/` (entire extension) | All capability moves to hackline; the extension becomes the rubix→hackline glue |

The `FleetTransport` trait dies because hackline gives you a concrete
type, not an abstraction. We don't need a trait to mock; we mock with
a real Zenoh router on loopback (already proven by
`fleet_zenoh_e2e.rs`). Trait abstractions for "what if we swap
transport later" are exactly the YAGNI that produced the parked
extension.

### 3.2 Deleted at hackline Phase 2 (api/* RPC + cmd outbox)

| rubix path | Replaced by |
|---|---|
| `agent/crates/transport-rest/src/fleet.rs` — `fleet::mount`, `pathToSubject` | `hackline_client::Session::serve_api(topic, handler)` per route |
| `clients/ts/src/transport/fleet_request.ts` — `FleetRequestTransport`, `FleetRequestFn` | Studio uses HTTP-through-hackline-tunnel (Caddy → `device-N.cloud.com` → device's existing axum) |
| `clients/ts/src/transport/request.ts` — `pathToSubject` table | Gone; tunnel carries normal HTTP |
| `clients/ts/src/client.ts` — `AgentClient` scope-switching, `fleetRequestFn` injection | `AgentClient` only knows HTTP; the base URL switches between local and `https://device-N.cloud.com` |
| `crates/domain-fleet/manifests/sys.fleet.remote-agent.yaml` — *agent_id* slot | Same node kind, but `agent_id` becomes the **hackline ZID**, and the "expand subtree" UX becomes "open the device's hackline URL" |
| Studio `/scope/:agent_id/...` URL routing (gap #4) | Gone; URL is `https://device-N.cloud.com/...` |
| Studio fleet-WS `fleetRequestFn` (gap #3) | Gone; no Studio→fleet WS connection at all |
| Frontend remote-agent inline tree expansion (gap #5) | Gone; clicking a remote-agent node opens its hackline URL |
| Auth-context-on-fleet-handler (gap #6) | Gone; rubix's existing axum auth middleware sees the request normally — Caddy + hackline-tunnel are transparent |

That's gaps #3, #4, #5, #6 from FLEET-TRANSPORT.md *deleted* rather
than implemented. The work was real; the design that needed it was
wrong.

### 3.3 Kept (not replaced by hackline)

| rubix path | Why it stays |
|---|---|
| `crates/domain-fleet/manifests/sys.fleet.remote-agent.yaml` — kind itself | Still the right model: a remote agent is a node in the local graph. Only the implementation under it changes. |
| `crates/domain-fleet/manifests/sys.fleet.group.yaml` | Same — folder for grouping remote agents. |
| `sys.agent.fleet` node kind (gap #1) | Stays. Slots now reflect hackline session state instead of fleet-transport state. Source data is `hackline_client::Session::health()`. |

---

## 4. Migration sequence

One subsystem at a time. After each step: rubix CI green, hackline
example unchanged, deleted code stays deleted (no
"keep-just-in-case" branches).

### Step 0 — rename `agent_id` ↔ ZID *(if §7 Q2 resolves to "merge")*

If the rubix `agent_id` field is just the Zenoh ZID by another name
(very likely), do the rename in its own PR **before Step 1**. Two
names for the same concept across a migration guarantees a bug.
Likely rename: `agent_id` → `zid` in rubix config, with one release
cycle of accepting both names for backwards compatibility. If the
answer is "they're genuinely different," document the mapping in
rubix's CODELESS.md (or equivalent) before Step 1 starts.

### Step 1 — `event.graph.slot.*.changed` publisher (smallest seam)

**Hackline prereq:** Phase 1.5 shipped (`publish_event` in the SDK,
`/v1/events` + SSE in the gateway).

In rubix:

1. Add `hackline-client` + `hackline-proto` to `agent`'s root
   `Cargo.toml` as path-deps.
2. In whatever today calls `transport.publish(subject, payload)` for
   slot-change events, replace with
   `hackline_session.publish_event(topic, payload)`.
3. Open one `hackline_client::Session` in `apps/agent/src/main.rs`
   alongside existing initialisation. Pass it down via `AppState` as
   a concrete `Arc<hackline_client::Session>` (no `Arc<dyn _>`
   abstraction — see the box below).
4. Delete `transport-fleet-zenoh`. Delete `spi::fleet::FleetTransport`
   and friends. Delete `NullTransport`. Delete the `Cargo.toml`
   entries that mention them.
5. CI green = step 1 done.

> **Why concrete `Arc<Session>` instead of a trait** — the next
> reviewer will be tempted to reintroduce `trait FleetTransport` "for
> testability." Don't. Tests construct a real `Session` against a
> loopback Zenoh router (the rig from `fleet_zenoh_e2e.rs` already
> proves this works). A trait abstraction was exactly the seam that
> let the original fleet code accrete; the loopback rig replaces it
> at lower cost. If a unit test genuinely cannot afford the loopback
> session, factor the unit under test so it doesn't depend on
> `Session` at all — take the inputs and return the outputs.

**Done test:** Studio (running locally against this rubix) sees a
slot-change event arrive via `GET /v1/devices/:id/msg/events/stream`
on a hackline gateway. Two minutes of manual click-and-watch.

### Step 2 — `api/v1/*` handlers

**Hackline prereq:** Phase 2 shipped (`serve_api` in SDK,
`POST /v1/devices/:id/api/:topic` in gateway).

In rubix:

1. For each `*_core` fn currently registered via
   `transport_rest::fleet::mount`, also register it via
   `session.serve_api(topic, handler)`. The handler closure calls the
   same `*_core` fn the axum route already calls. **Both paths
   coexist for one PR cycle.**
2. Switch Studio's "remote agent" code path off `FleetRequestTransport`
   onto plain HTTP-against-`https://device-N.cloud.com`. (Requires
   Phase 2 of hackline shipping HTTP host-routing tunnels.)
3. **Origin-change shakedown** — before declaring step 2 done, verify
   that switching Studio's effective origin from `localhost:3000` to
   `https://device-N.cloud.com/` doesn't break:
   - cookie domain (Studio's session cookie was scoped to `localhost`
     or the cloud control-plane host — it must follow to the
     per-device subdomain or be replaced by something that does)
   - CORS preflights (the device's axum CORS allowlist needs the new
     origin pattern, or the request must be same-origin)
   - CSP (`connect-src` / `img-src` lists hardcoded hosts in dev?)
   - mixed-content with localhost during dev (Studio dev server on
     `http://localhost:3000` calling `https://edge-N...` is fine;
     the reverse is blocked)
   - Studio's auth state survives the origin change — specifically,
     whether the OIDC redirect URI matches the per-device subdomain
   This is the kind of detail that ate weeks the first time. Land it
   as its own PR with manual click-through evidence in the description.
4. Delete `transport_rest::fleet::mount` and the parallel surface.
5. Delete the TS `FleetRequestTransport`, `FleetRequestFn`,
   `pathToSubject`, scope-switching in `AgentClient`. `AgentClient`
   becomes "an HTTP client with a base URL," nothing more.
6. Delete Studio's `/scope/:agent_id/...` routes. URL for "I'm looking
   at edge-42" becomes `https://edge-42.cloud.com/`.

**Done test:** Studio loads `https://edge-42.cloud.com/`, the entire
local SPA renders against the device's own axum, the network tab
shows `https://edge-42.cloud.com/api/v1/nodes` (not a fleet WS).

**Acknowledged regression:** the unified-tree-of-many-devices view
(today: expand a `sys.fleet.remote-agent` node and see its subtree
inline alongside other agents) goes away. Browsing N devices is now
N tabs, one per per-device URL. This is a deliberate trade — the
unified tree was the abstraction that produced gaps #3–#6 in the
first place. If users push back, the right response is a
"device-switcher" UI element that opens a different URL, not
rebuilding the in-tree fleet client.

### Step 3 — `cmd/*` subscriber

**Hackline prereq:** Phase 2 shipped (cmd outbox + at-least-once delivery).

In rubix:

1. Wherever today the agent receives "cloud→edge commands" (block
   install, reload, …) — replace with
   `session.subscribe_cmd(topic).await`.
2. **Persistent dedupe**, not in-memory — at-least-once + crash
   between handler-success and ack means the same `cmd_id` will
   redeliver after restart. An in-memory `HashMap` would re-execute
   the command. Use rubix's existing SQLite (or a small dedicated
   sqlite file) with this shape:
   ```sql
   CREATE TABLE hackline_cmd_seen (
     cmd_id   TEXT PRIMARY KEY,
     topic    TEXT NOT NULL,
     seen_at  INTEGER NOT NULL,
     result   TEXT NOT NULL
   );
   CREATE INDEX hackline_cmd_seen_age ON hackline_cmd_seen(seen_at);
   ```
   On receipt: `INSERT OR IGNORE`; if the row already existed,
   re-emit the previous ack and skip handler invocation. **TTL: 24h
   by default**, vacuumed by a daily background task — the table
   grows without bound otherwise. The TTL must comfortably exceed
   the gateway's `cmd.default_ttl` (currently 7d), so set it to
   either 7d+1h or document the reasoning if shorter.
3. Wire the existing audit-log emission into `cmd.ack` so an audit
   row lands per command.

**Done test:** From a `hackline` CLI:
`hackline cmd send --device edge-42 --topic block.install --payload @block.json`
results in the device installing the block, an SSE
`cmd.acked` event reaching Studio, and an `audit` row in the gateway.

### Step 4 — kill `com.rubix.fleet`

After steps 1–3 land, the `com.rubix.fleet` extension has no code
path it owns. Delete the extension directory. Delete
`FLEET-REMOVAL.md`, `FLEET-NEXT.md`. Update `OVERVIEW.md` to point at
hackline.

The four documents in `rubix-agent/docs/design/fleet/` get replaced by
one short pointer: "rubix uses hackline for fleet messaging; see
`hackline/SCOPE.md` for the design and `hackline/INTEGRATION-RUBIX.md`
for our consumption rules."

---

## 5. Failure modes and what to do

These are the moments when the discipline matters.

### "I need a thing from `hackline-core`"

You don't. You need `hackline-client` to expose that thing. Open a
ticket in hackline; add the API; ship a `hackline-client` patch
release; consume it in rubix. If you find yourself reaching past the
SDK twice in a week, the SDK is too thin — push back on hackline's
public surface.

### "The SDK is awkward for our use case"

Good. That's the signal we need. Do not paper over it in rubix. Open a
hackline issue with the awkward call site copied verbatim; decide
whether the right fix is an SDK ergonomic improvement, a wire-protocol
change (rare), or a documented rubix-side wrapper (rarer still — and
only ever a thin one).

### "We need to ship a rubix release before hackline lands feature X"

Pin `hackline-client` + `hackline-proto` to a specific git SHA in
rubix's `Cargo.toml`:

```toml
hackline-client = { git = "https://github.com/NubeIO/hackline", rev = "<sha>" }
hackline-proto  = { git = "https://github.com/NubeIO/hackline", rev = "<sha>" }
```

Ship rubix; flip back to `version = "0.x"` once X lands. **Don't
fork.** Don't add a "rubix-flavoured" patch on top. **Don't use a
path-dep against an out-of-tree checkout** — path-deps point at a
working tree, not a SHA, and a dirty checkout will quietly differ
from what was tagged. Git-rev or `cargo vendor` are the only valid
mechanisms. The pin is ephemeral; a fork would not be.

### "Two device apps want to share state"

Through Zenoh, not through `hackline-client` internals. Each app
publishes/subscribes on its own keyexprs; the SDK is a Zenoh-session
wrapper, not a shared-memory broker. If you find yourself wanting
inter-app state-sharing primitives, you want a different abstraction
above hackline — build it in rubix as `rubix-app-coordinator` or
similar, don't push it into hackline.

### "Hackline is down / not deployed yet"

Rubix runs in standalone mode (no `hackline-client::Session::open`
called) the same way it does today with `fleet: null`. Local REST
surface is unchanged; the device is still fully usable on its
loopback. This is the correct fail-open behaviour and matches the
existing `NullTransport` semantic — except now it's just "don't
construct the optional `Arc<Session>`," not a whole trait.

**Critical for kiosk-mode customers:** the `--offline` flag (or
`fleet: null` config) must short-circuit *before* `Session::open` is
called — not just before the first `publish_event`. Otherwise
devices without network spend the boot-time Zenoh discovery window
blocking on peers that don't exist. Concretely: in
`apps/agent/src/main.rs`, the construction of `Arc<Session>` is
gated by `if !offline_mode { Some(Session::open(…).await?) } else
{ None }`, and every call site is `Option<&Session>`-aware.

---

## 6. Boundary tests (CI, both repos)

### In hackline CI

- `examples/echo-app` builds and runs against a local Zenoh peer.
  Catches "we shipped an SDK regression."
- `examples/raw-zenoh` does the same operations without
  `hackline-client`. Catches "we shipped an SDK feature with no
  wire-level path."
- **R3 parity test:** `examples/echo-app` and `examples/raw-zenoh`
  run their full sequence in CI; observable side-effects on the
  gateway side (events table rows, audit rows) must be identical
  modulo `id`/`ts`. A divergence means the SDK is doing something
  the wire alone can't, which is exactly R2's failure mode.
- **Public API surface diff** (`cargo-public-api` or equivalent),
  with semver matrix gating per the R4 table:

  | Release type | Allowed surface change |
  |---|---|
  | patch (`0.1.4 → 0.1.5`) | none — PR with surface change is rejected |
  | minor (`0.1.x → 0.2.0`) | additive only — removals/renames must be deprecated in the prior release |
  | major (`0.x → 1.0`, `1.x → 2.0`) | anything goes |

### In rubix CI

- **R1 enforcement via `cargo metadata`** (not textual grep — a
  `# hackline-agent runs on the device` comment or a third-party
  crate that transitively pulls `hackline-core` would slip past a
  grep):
  ```bash
  cargo metadata --format-version 1 \
    | jq -r '.packages[].dependencies[].name' \
    | grep -E '^hackline-(core|gateway|agent)$' \
    && exit 1 || exit 0
  ```
  This catches transitive imports too, which is the whole point.
- `cargo tree -p rubix-agent` — must show `hackline-client` and
  `hackline-proto` and *only* those two from the hackline tree.
- Integration test: spin up a `hackline-gateway` on loopback, a
  `hackline-agent` on loopback, a rubix-agent that opens a
  `hackline_client::Session`; assert that publishing an event on the
  rubix side reaches the gateway's `events` table.

---

## 7. Open questions (rubix-side)

These don't block hackline development; they need answering before
rubix steps 1–3 land.

1. **Where in rubix-agent does `hackline_client::Session` live?**
   `apps/agent/src/main.rs` constructs it and stuffs it into
   `AppState` is the strawman. Confirm during step 1.
2. **Does the existing rubix `agent_id` map 1:1 to the hackline ZID?**
   They serve the same role ("which device am I"). If yes, drop one
   of them — and do the rename **before Step 1** (see Step 0). Two
   names for the same concept during a migration is the kind of
   ambiguity that produces field-mapping bugs nobody catches until
   prod. If no, document the mapping in rubix's CODELESS.md before
   Step 1 starts. Likely answer: hackline ZID = rubix `agent_id`,
   renamed to `zid`.
3. **Does `sys.agent.fleet` keep its current slot schema?** Slots like
   `messages_in` / `messages_out` make sense for both transports.
   `backend` enum changes from `[none, zenoh, mqtt]` to `[none, hackline]`.
4. **What does `agent --offline` mean post-migration?** Today it
   means "don't open fleet transport." Post-migration it means
   "don't construct the hackline session." Same shape, different
   crate. Critically: the gating must short-circuit before
   `Session::open` is called (see §5 "Hackline is down").
5. **L1+L2 token model — hackline-native or rubix-modelled?** §9 covers
   the question; pick before Phase 2 of hackline locks in.
6. **Auth seam — α, β, or γ?** §9 covers the question; pick before
   any rubix `role: edge` device is exposed to a real customer through
   a hackline tunnel.
7. **`X-Rubix-User` signing key — per-device asymmetric or shared HMAC?**
   Only relevant if Option α wins in Q6. Per-device asymmetric (rotated
   on enrollment) is safer; shared HMAC is cheaper. Operator input
   needed.
8. **First-touch user provisioning** — when a customer JWT arrives at
   a device that has never seen this `sub` before, does the device
   auto-create the `sys.auth.user` row, or refuse with `403
   user_not_provisioned`? Today's rubix model is pre-provision.
   Auto-create is more usable for customer-facing flows; pre-provision
   matches existing semantics. Decide before Phase 2.

---

## 8. Pointers

- Hackline scope: [`SCOPE.md`](./SCOPE.md)
- Hackline decisions: [`DECISIONS.md`](./DECISIONS.md) — specifically the
  `standalone-vs-in-tree — 2026-05` entry, which is the load-bearing
  ADR for this whole document.
- Rubix fleet (parked): `../../rubix-workspace/rubix-agent/docs/design/fleet/FLEET-TRANSPORT.md`
- Rubix overview: `../../rubix-workspace/rubix-agent/docs/design/OVERVIEW.md`
- Rubix auth model: `../../rubix-workspace/rubix-agent/docs/design/auth/RAUTHY-MIGRATION.md`, `../../rubix-workspace/rubix-agent/docs/design/auth/SIDEBAR-ACCESS.md`, `../../rubix-workspace/rubix-agent/docs/design/auth/AUTH.md` — mandatory reading before touching anything in §9.
- The cautionary tale: `../../rubix-workspace/rubix-extensions/com.rubix.fleet/FLEET-REMOVAL.md` — worth re-reading every time the temptation to short-cut R1 surfaces.

**Path verification:** the rubix paths in §3.1 / §3.2 reflect the
rubix-agent repo as audited 2026-05. Re-verify with `git ls-files`
in the rubix repo before executing each migration step — the rubix
codebase moves and a stale path here would mislead the deletion list.

---

## 9. Auth seam — device access, tunnel access, in-device authz

> Auth has **three independent layers** between a customer's browser
> and a graph node on edge-42. Conflating them is how every
> previous fleet-auth design got tangled. This section names the
> layers, says which system owns each, and identifies the one
> genuinely-new thing that has to be built: the seam that carries
> a Rauthy-issued user identity through hackline-gateway into a
> rubix `role: edge` agent that does **not** run Rauthy.

### 9.1 The three layers

| Layer | Question | Owner | Storage |
|---|---|---|---|
| **L1 — device access** | Can user X reach device 42 *at all*? | hackline-gateway | `users.device_scope` (SCOPE §6.2 / §7.2) |
| **L2 — tunnel access** | Can user X reach this specific tunnel (port 22, hostname `device-42.cloud.com`) on device 42? | hackline-gateway | `users.tunnel_scope` (SCOPE §6.2 / §7.2) |
| **L3 — in-device authz** | Once on device 42, can user X read `/sidebar/extensions/com.nube.plm`? | rubix (`Authz::can`, `sys.auth.grant`) | rubix's existing graph |

These are **independent**. A customer with L1+L2 to a device's HTTP
tunnel can still be locked out of half the sidebar by L3. An L3
admin role on the device is meaningless if L1 says "you can't see
this device." Both checks must pass; neither subsumes the other.

### 9.2 What's already designed

**L1 + L2 are already in hackline.** `hackline-proto::ScopedToken`
(per SCOPE §6.2) carries `device_scope: Vec<DeviceId> | "*"` and
`tunnel_scope: Vec<TunnelId> | "*"`, enforced by hackline-gateway at
the REST + tunnel-listener edge. The audit row in `audit` (SCOPE
§7.2) records who reached what. Nothing new to build here — it's the
existing hackline design.

**L3 is already in rubix.** `domain-auth`'s `Authz::can`,
`sys.auth.grant` rows, role baselines, and the SIDEBAR-ACCESS
machinery all stay exactly as they are today. Hackline does not
know what a sidebar route is and must not learn.

### 9.3 The seam that has to be built

When a customer reaches `https://device-42.cloud.com/api/v1/...`
through a hackline tunnel, **the device's axum receives the
request**. The device must know who the user is so `Authz::can` can
run. Today's rubix auth model says:

- `role: cloud` agents verify Rauthy JWTs via `auth-oidc`.
- `role: edge` agents use `StaticTokenProvider` only.
  RAUTHY-MIGRATION.md is explicit: **Rauthy stays out of the edge
  boot path.** That promise is load-bearing for offline-capable
  edges.

So a Rauthy JWT cannot arrive at the device unmodified — the device
wouldn't (and shouldn't have to) verify it. Three options:

#### Option α — gateway terminates auth, signs a downstream identity header *(recommended)*

The oauth2-proxy pattern, well-trodden:

1. Customer logs into hackline-gateway via Rauthy (gateway is a
   Rauthy OIDC client).
2. Gateway maps Rauthy JWT → hackline `ScopedToken` (L1+L2 enforced
   here).
3. When proxying through the tunnel to `device-42`, gateway adds
   two headers:
   - `Authorization: Bearer <device's static token>` — what edge
     already trusts via `StaticTokenProvider`.
   - `X-Rubix-User: <signed JSON: { sub, roles, name, email }>` —
     signed with a key the device pre-trusts at enrollment.
4. Edge auth middleware (new provider in `crates/auth/`) verifies
   the header signature against the pre-trusted key and constructs
   `AuthContext` from it. `Authz::can` runs unchanged.

**Why this wins:** keeps RAUTHY-MIGRATION's edge promise intact;
lets devices run fully disconnected from Rauthy after enrollment;
preserves rubix's `AuthContext` shape; lets Studio talk directly to
`https://device-42.cloud.com/` over HTTPS-through-tunnel without a
mediation hop; lets the device's audit log record real user identity.

**The new key:** at device enrollment hackline-gateway hands the
device its `X-Rubix-User` verification key (per-device asymmetric
strongly preferred over shared HMAC — see §7 Q7). Rotation = re-enroll.

#### Option β — push Rauthy verification onto the edge

Give every edge enough Rauthy config (JWKS URL, issuer, audience) to
verify JWTs offline. Rubix's `auth-oidc` already does this work.

**Why we reject it:** contradicts RAUTHY-MIGRATION's explicit
"Rauthy stays cloud-only" stance; requires every edge to reach (or
freshly cache) the gateway's JWKS, which couples edge auth to cloud
availability in a way the static-token model deliberately avoided;
stale-cache semantics on disconnected edges introduce a class of
bugs the static-token model doesn't have. The RAUTHY-MIGRATION
author explicitly set this constraint; we honour it.

#### Option γ — gateway is the only thing that ever calls the device

Gateway makes the HTTP request to the device on the user's behalf;
device sees only the gateway's static token; user identity lives
entirely in the gateway's audit log.

**Why we reject it:** Studio's React SPA can no longer talk
directly to the device's axum — every API call routes through
gateway code, and that code grows with every rubix REST route
(every `*_core` fn we just deleted in Step 2 comes back as a
gateway shim). The device's audit log loses user identity entirely,
a regression from rubix today. This is the FleetRequestTransport
shape we're explicitly walking away from.

### 9.4 Recommended split, in one diagram

```
  customer browser
        |
        | https://device-42.cloud.com/...
        v
  Caddy (TLS)  ----+
        |          |  Rauthy login (cloud-side OIDC, today)
        v          v
  hackline-gateway --[L1: device-42 in token.device_scope?]--> 403 if no
        |        --[L2: this tunnel in token.tunnel_scope?]--> 403 if no
        |
        |  proxies through Zenoh tunnel,
        |  injecting headers:
        |    Authorization: Bearer <device static token>
        |    X-Rubix-User: <signed { sub, roles, ... }>
        v
  hackline-agent (tunnel plane only, byte-blind)
        |
        v
  device's existing axum (rubix-agent on edge)
        |
        +-- StaticTokenProvider verifies the bearer (today)
        +-- NEW: GatewayHeaderProvider verifies X-Rubix-User signature,
        |   constructs AuthContext { sub, roles, ... }
        v
  Authz::can(user, Read, "/sidebar/...")  --> 403 if no [L3]
        |
        v
  handler runs
```

L1 and L2 are hackline-gateway's job, enforced before bytes leave
the gateway. L3 is rubix's job, enforced inside the device. The
seam between them is one signed header.

### 9.5 Concrete things to build

**In hackline (Phase 2-ish, alongside HTTP host-routing):**

- `hackline-gateway` becomes a Rauthy OIDC client (or generic OIDC —
  see below). New gateway config block `[auth.oidc]` with issuer,
  client_id, client_secret, redirect_uri.
- New REST flow: `GET /v1/auth/login` → redirect to IdP;
  `GET /v1/auth/callback` → exchange code, mint a hackline
  `ScopedToken`, set cookie scoped to `*.cloud.example.com`.
- New gateway primitive: `IdentityHeaderSigner` that, given a
  `ScopedToken` and the per-device key, produces an `X-Rubix-User`
  header with the device's expected schema. The schema is small and
  stable: `{ "sub": "...", "name": "...", "email": "...", "roles":
  ["..."], "iat": ... }` plus a signature. Signature scheme: Ed25519
  detached signature over canonical JSON, base64url-encoded into a
  second header `X-Rubix-User-Sig`.
- Per-device key issuance during device enrollment: gateway generates
  an Ed25519 keypair per device, stores the private half in `devices`
  table, ships the public half to the device in its enrollment
  bundle.

**Keep this generic.** Don't import `rauthy_client`. Use any OIDC
library; `oauth2` + `openidconnect` crates are the obvious choices.
The gateway must work behind Auth0, Keycloak, Authentik, or a future
non-Rauthy IdP without code changes.

**In rubix (Phase 2 of the migration, no earlier):**

- New `crates/auth/src/gateway_header.rs` — `GatewayHeaderProvider`
  implementing the same trait as `StaticTokenProvider`. Verifies
  Ed25519 signature against the per-device public key (loaded at
  boot from `/etc/rubix/gateway-pubkey.pem` or wherever enrollment
  lands it).
- The auth middleware tries providers in order: `StaticTokenProvider`
  first (so existing CLI/SDK paths unchanged), then
  `GatewayHeaderProvider`. First success wins.
- No change to `Authz::can`. No change to `sys.auth.grant`. No change
  to SIDEBAR-ACCESS.
- Optional: a `Source` enum on `AuthContext` (`{ Static, Gateway,
  Oidc }`) so audit rows can record which provider authorised the
  request. Not required for correctness; useful for forensics.

### 9.6 What this seam deliberately does not handle

- **End-to-end mutual auth between customer browser and device.**
  TLS terminates at Caddy; the gateway sees plaintext; the gateway
  is trusted (per SCOPE §3.5 "the gateway is a privileged single
  point of compromise by design"). Anyone wanting true E2E talks
  directly to the device over Zenoh, which is its own engineering
  exercise and not v0.1.
- **Customer-to-customer isolation.** Single-tenant per deployment,
  per RAUTHY-MIGRATION's stance. Multi-tenant is Phase 4 of hackline.
- **Per-action tokens** (e.g. "this token can install block X but
  not block Y"). L3 is rubix's existing per-resource grant model;
  we don't add a new layer.
- **Token introspection through the tunnel** ("is this customer's
  session still valid right now?"). Tokens are JWTs with
  `exp`/`iat`; the device honours them until expiry. Revocation
  faster than expiry needs OIDC introspection, which is gateway-side
  only — the gateway refuses to mint `X-Rubix-User` for a revoked
  session, and the device sees no further requests.

### 9.7 Decision deadline

The α/β/γ choice gates Phase 2 of hackline (HTTP host-routing) and
the rubix migration's Step 2 (origin-change shakedown). It does not
block Phase 1 (TCP tunnels), Phase 1.5 (events), or rubix migration
Step 1 (event publisher) — those use the existing static-token paths
and never put a customer JWT on the wire.

**Don't ship customer-facing per-device URLs without resolving §9.**
A customer hitting `https://device-42.cloud.com/` against an
undecided seam will accidentally fix one of the three options by
shipping it. Make the choice deliberately.
