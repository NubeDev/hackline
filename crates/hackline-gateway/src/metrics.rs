//! In-process Prometheus counters / gauges, formatted on demand by
//! `GET /metrics` (SCOPE.md §10.2). Hand-rolled instead of pulling
//! the `prometheus` crate — every metric in §10.2 is either a
//! plain counter or a small set of labelled counters, which a
//! `RwLock<HashMap<labels, u64>>` covers without a new dependency.
//!
//! Cardinality of label values is bounded by the caller; the
//! events-received counter folds anything outside an operator-defined
//! allowlist into an `_other` bucket per SCOPE.md §10.2.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, RwLock};

/// Outcome label for `hackline_tunnel_sessions_total` and
/// `hackline_api_calls_total`.
#[derive(Debug, Clone, Copy)]
pub enum Outcome {
    Ok,
    Error,
}

impl Outcome {
    fn as_str(&self) -> &'static str {
        match self {
            Outcome::Ok => "ok",
            Outcome::Error => "error",
        }
    }
}

#[derive(Default)]
struct Inner {
    /// `(kind, outcome) -> count`
    tunnel_sessions_total: BTreeMap<(String, String), u64>,
    /// `kind -> active gauge`
    tunnel_active: BTreeMap<String, i64>,
    /// `direction -> bytes` (`up`/`down`)
    tunnel_bytes_total: BTreeMap<String, u64>,
    /// `outcome -> count` — `accepted|rejected|failed|done|expired|cancelled`
    cmd_total: BTreeMap<String, u64>,
    /// `topic -> count` (allowlisted via `set_event_topic_allowlist`)
    events_received_total: BTreeMap<String, u64>,
    /// `level -> count`
    logs_received_total: BTreeMap<String, u64>,
    /// `(topic, outcome) -> count`
    api_calls_total: BTreeMap<(String, String), u64>,
    event_topic_allow: Option<Vec<String>>,
}

#[derive(Clone, Default)]
pub struct Metrics {
    inner: Arc<RwLock<Inner>>,
    /// `hackline_devices_online{}` — written by liveliness watchers,
    /// read by the metrics formatter. Atomic so the read path doesn't
    /// take the inner lock.
    devices_online: Arc<AtomicI64>,
    /// `hackline_audit_rows{}` — refreshed by `set_audit_rows` from a
    /// `SELECT COUNT(*)` immediately before formatting.
    audit_rows: Arc<AtomicI64>,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_devices_online(&self, n: i64) {
        self.devices_online.store(n, Ordering::Relaxed);
    }

    pub fn set_audit_rows(&self, n: i64) {
        self.audit_rows.store(n, Ordering::Relaxed);
    }

    /// Operator-configured set of event topics to expose with their
    /// real label value; anything outside the allowlist collapses
    /// into a single `_other` bucket per SCOPE.md §10.2 to keep
    /// Prometheus cardinality finite. `None` (the v0.1 default)
    /// disables folding — every topic gets its own bucket.
    pub fn set_event_topic_allowlist(&self, topics: Option<Vec<String>>) {
        self.inner.write().unwrap().event_topic_allow = topics;
    }

    pub fn inc_tunnel_session(&self, kind: &str, outcome: Outcome) {
        let mut g = self.inner.write().unwrap();
        *g.tunnel_sessions_total
            .entry((kind.to_owned(), outcome.as_str().to_owned()))
            .or_insert(0) += 1;
    }

    pub fn inc_tunnel_active(&self, kind: &str) {
        let mut g = self.inner.write().unwrap();
        *g.tunnel_active.entry(kind.to_owned()).or_insert(0) += 1;
    }

    pub fn dec_tunnel_active(&self, kind: &str) {
        let mut g = self.inner.write().unwrap();
        let entry = g.tunnel_active.entry(kind.to_owned()).or_insert(0);
        if *entry > 0 {
            *entry -= 1;
        }
    }

    pub fn add_tunnel_bytes(&self, up: u64, down: u64) {
        let mut g = self.inner.write().unwrap();
        *g.tunnel_bytes_total.entry("up".into()).or_insert(0) += up;
        *g.tunnel_bytes_total.entry("down".into()).or_insert(0) += down;
    }

    pub fn inc_cmd(&self, outcome: &str) {
        let mut g = self.inner.write().unwrap();
        *g.cmd_total.entry(outcome.to_owned()).or_insert(0) += 1;
    }

    pub fn inc_event(&self, topic: &str) {
        let mut g = self.inner.write().unwrap();
        let bucket = match &g.event_topic_allow {
            Some(allow) if !allow.iter().any(|t| t == topic) => "_other".to_owned(),
            _ => topic.to_owned(),
        };
        *g.events_received_total.entry(bucket).or_insert(0) += 1;
    }

    pub fn inc_log(&self, level: &str) {
        let mut g = self.inner.write().unwrap();
        *g.logs_received_total.entry(level.to_owned()).or_insert(0) += 1;
    }

    pub fn inc_api_call(&self, topic: &str, outcome: Outcome) {
        let mut g = self.inner.write().unwrap();
        *g.api_calls_total
            .entry((topic.to_owned(), outcome.as_str().to_owned()))
            .or_insert(0) += 1;
    }

    /// Render Prometheus text-format. One TYPE+HELP per metric family
    /// per the exposition format.
    pub fn render(&self, cmd_outbox_depth: &BTreeMap<String, i64>) -> String {
        let g = self.inner.read().unwrap();
        let mut out = String::with_capacity(2048);

        out.push_str(
            "# HELP hackline_devices_online Devices currently observed via Zenoh liveliness.\n",
        );
        out.push_str("# TYPE hackline_devices_online gauge\n");
        out.push_str(&format!(
            "hackline_devices_online {}\n",
            self.devices_online.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP hackline_tunnel_sessions_total Bridged TCP sessions completed, by kind and outcome.\n");
        out.push_str("# TYPE hackline_tunnel_sessions_total counter\n");
        for ((kind, outcome), v) in &g.tunnel_sessions_total {
            out.push_str(&format!(
                "hackline_tunnel_sessions_total{{kind=\"{kind}\",outcome=\"{outcome}\"}} {v}\n"
            ));
        }

        out.push_str("# HELP hackline_tunnel_active Currently active bridged sessions, by kind.\n");
        out.push_str("# TYPE hackline_tunnel_active gauge\n");
        for (kind, v) in &g.tunnel_active {
            out.push_str(&format!("hackline_tunnel_active{{kind=\"{kind}\"}} {v}\n"));
        }

        out.push_str("# HELP hackline_tunnel_bytes_total Bytes pumped through the tunnel plane, by direction.\n");
        out.push_str("# TYPE hackline_tunnel_bytes_total counter\n");
        for (dir, v) in &g.tunnel_bytes_total {
            out.push_str(&format!(
                "hackline_tunnel_bytes_total{{direction=\"{dir}\"}} {v}\n"
            ));
        }

        out.push_str(
            "# HELP hackline_cmd_outbox_depth Pending (un-acked) cmd_outbox rows per device.\n",
        );
        out.push_str("# TYPE hackline_cmd_outbox_depth gauge\n");
        for (dev, v) in cmd_outbox_depth {
            out.push_str(&format!(
                "hackline_cmd_outbox_depth{{device=\"{dev}\"}} {v}\n"
            ));
        }

        out.push_str("# HELP hackline_cmd_total Cmd lifecycle transitions, by outcome.\n");
        out.push_str("# TYPE hackline_cmd_total counter\n");
        for (outcome, v) in &g.cmd_total {
            out.push_str(&format!(
                "hackline_cmd_total{{outcome=\"{outcome}\"}} {v}\n"
            ));
        }

        out.push_str(
            "# HELP hackline_events_received_total Message-plane events received via fan-in.\n",
        );
        out.push_str("# TYPE hackline_events_received_total counter\n");
        for (topic, v) in &g.events_received_total {
            out.push_str(&format!(
                "hackline_events_received_total{{topic=\"{topic}\"}} {v}\n"
            ));
        }

        out.push_str(
            "# HELP hackline_logs_received_total Message-plane logs received via fan-in.\n",
        );
        out.push_str("# TYPE hackline_logs_received_total counter\n");
        for (level, v) in &g.logs_received_total {
            out.push_str(&format!(
                "hackline_logs_received_total{{level=\"{level}\"}} {v}\n"
            ));
        }

        out.push_str(
            "# HELP hackline_api_calls_total Synchronous api/* RPC calls, by topic and outcome.\n",
        );
        out.push_str("# TYPE hackline_api_calls_total counter\n");
        for ((topic, outcome), v) in &g.api_calls_total {
            out.push_str(&format!(
                "hackline_api_calls_total{{topic=\"{topic}\",outcome=\"{outcome}\"}} {v}\n"
            ));
        }

        out.push_str("# HELP hackline_audit_rows Total rows in the audit table.\n");
        out.push_str("# TYPE hackline_audit_rows gauge\n");
        out.push_str(&format!(
            "hackline_audit_rows {}\n",
            self.audit_rows.load(Ordering::Relaxed)
        ));

        out
    }
}
