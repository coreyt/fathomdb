//! Lifecycle observability data types.
//!
//! Pure data types and the subscriber boundary trait. Public type shape,
//! phase semantics, diagnostic source/category taxonomy, counter snapshot
//! key set, profile record shape, and stress-failure payload are owned by
//! `dev/design/lifecycle.md`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use crate::CounterSnapshot;

/// Lifecycle phase tag.
///
/// Five-value enum locked by AC-001 / AC-008 and `dev/design/lifecycle.md`
/// § Phase enum.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Phase {
    Started,
    Slow,
    Heartbeat,
    Finished,
    Failed,
}

/// Origin of a structured diagnostic.
///
/// Pinned by `dev/design/lifecycle.md` § Diagnostic source and category.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum EventSource {
    Engine,
    SqliteInternal,
}

/// Stable diagnostic category.
///
/// `Writer`, `Search`, `Admin`, `Error` pair with `EventSource::Engine`.
/// `Corruption`, `Recovery`, `Io` pair with `EventSource::SqliteInternal`.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum EventCategory {
    Writer,
    Search,
    Admin,
    Error,
    Corruption,
    Recovery,
    Io,
}

/// Public lifecycle event payload.
///
/// The required public shape in 0.6.0 is the typed phase + source + category
/// triple. Producing surfaces own additional non-required envelope fields
/// per `dev/design/lifecycle.md` § Public event contract.
#[derive(Debug, Clone)]
pub struct Event {
    pub phase: Phase,
    pub source: EventSource,
    pub category: EventCategory,
}

/// Host-routed subscriber boundary.
///
/// `dev/design/lifecycle.md` § Host-routed diagnostics requires that all
/// engine and SQLite-internal diagnostics flow through the host's chosen
/// subscriber. No private sink, no stderr fallback.
pub trait Subscriber: Send + Sync {
    fn on_event(&self, event: &Event);
}

/// Engine-side registry of attached subscribers.
///
/// Holds attached subscribers behind a `Mutex<Vec<...>>`. Dispatch fans
/// the event out to every live subscriber. Drop of a [`Subscription`]
/// detaches that subscriber by id.
#[derive(Default)]
pub(crate) struct SubscriberRegistry {
    next_id: AtomicU64,
    entries: Mutex<Vec<(u64, Arc<dyn Subscriber>)>>,
}

impl std::fmt::Debug for SubscriberRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.entries.lock().map(|e| e.len()).unwrap_or(0);
        f.debug_struct("SubscriberRegistry").field("subscribers", &count).finish()
    }
}

impl SubscriberRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn attach(self: &Arc<Self>, subscriber: Arc<dyn Subscriber>) -> Subscription {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut entries) = self.entries.lock() {
            entries.push((id, subscriber));
        }
        Subscription { id, registry: Arc::downgrade(self) }
    }

    fn detach(&self, id: u64) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|(eid, _)| *eid != id);
        }
    }

    pub(crate) fn dispatch(&self, event: &Event) {
        // Snapshot the subscriber list so callbacks may not call back into the
        // registry while we hold the lock.
        let snapshot: Vec<Arc<dyn Subscriber>> = match self.entries.lock() {
            Ok(entries) => entries.iter().map(|(_, s)| Arc::clone(s)).collect(),
            Err(_) => return,
        };
        for sub in snapshot {
            sub.on_event(event);
        }
    }
}

/// Handle returned by `Engine::subscribe`.
///
/// Dropping the handle detaches the subscriber. Subscriber payload
/// semantics are owned by `dev/design/lifecycle.md` and
/// `dev/design/migrations.md`.
#[derive(Debug)]
pub struct Subscription {
    id: u64,
    registry: Weak<SubscriberRegistry>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(registry) = self.registry.upgrade() {
            registry.detach(self.id);
        }
    }
}

/// Per-statement profile record shape.
///
/// Field set locked by AC-005b / `dev/design/lifecycle.md` § Per-statement
/// profiling. `cache_delta` is signed because cache counters can decrease
/// across a statement window when SQLite evicts.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ProfileRecord {
    pub wall_clock_ms: u64,
    pub step_count: u64,
    pub cache_delta: i64,
}

/// Projection-status enum surfaced by the projection-status query.
///
/// Locked by AC-010.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ProjectionStatus {
    Pending,
    Failed,
    UpToDate,
}

/// Stress-failure context payload.
///
/// Required field set locked by AC-009 / REQ-007. The payload exists so
/// stress / robustness failure events do not degrade into ad hoc free-text
/// metadata; consumers must be able to reach all four fields without
/// message parsing.
#[derive(Debug, Clone)]
pub struct StressFailureContext {
    pub thread_group_id: u64,
    pub op_kind: String,
    pub last_error_chain: Vec<String>,
    pub projection_state: String,
}

/// Internal cumulative counters backing [`CounterSnapshot`].
///
/// Public snapshot key set is owned by `dev/design/lifecycle.md` § Public
/// key set. Snapshotting performs only atomic loads and a map clone — it
/// must not perturb counters (AC-004c).
#[derive(Debug)]
pub(crate) struct Counters {
    queries: AtomicU64,
    writes: AtomicU64,
    write_rows: AtomicU64,
    admin_ops: AtomicU64,
    cache_hit: AtomicU64,
    cache_miss: AtomicU64,
    errors_by_code: Mutex<BTreeMap<String, u64>>,
}

impl Counters {
    pub(crate) fn new() -> Self {
        Self {
            queries: AtomicU64::new(0),
            writes: AtomicU64::new(0),
            write_rows: AtomicU64::new(0),
            admin_ops: AtomicU64::new(0),
            cache_hit: AtomicU64::new(0),
            cache_miss: AtomicU64::new(0),
            errors_by_code: Mutex::new(BTreeMap::new()),
        }
    }

    pub(crate) fn record_write(&self, rows: u64) {
        self.writes.fetch_add(1, Ordering::Relaxed);
        self.write_rows.fetch_add(rows, Ordering::Relaxed);
    }

    pub(crate) fn record_query(&self) {
        self.queries.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_admin(&self) {
        self.admin_ops.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_error(&self, code: &str) {
        if let Ok(mut map) = self.errors_by_code.lock() {
            *map.entry(code.to_string()).or_insert(0) += 1;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn record_cache_hit(&self) {
        self.cache_hit.fetch_add(1, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub(crate) fn record_cache_miss(&self) {
        self.cache_miss.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(&self) -> CounterSnapshot {
        // Treat poisoned lock as zero-error snapshot — prefer non-perturbing read over panic.
        let errors_by_code = self.errors_by_code.lock().map(|map| map.clone()).unwrap_or_default();
        CounterSnapshot {
            queries: self.queries.load(Ordering::Relaxed),
            writes: self.writes.load(Ordering::Relaxed),
            write_rows: self.write_rows.load(Ordering::Relaxed),
            errors_by_code,
            admin_ops: self.admin_ops.load(Ordering::Relaxed),
            cache_hit: self.cache_hit.load(Ordering::Relaxed),
            cache_miss: self.cache_miss.load(Ordering::Relaxed),
        }
    }
}
