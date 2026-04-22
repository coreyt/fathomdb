//! Background actor that drives the `vector_projection_work` queue.
//!
//! Modeled on [`crate::rebuild_actor::RebuildActor`]: one OS thread,
//! `std::sync::mpsc` wakeups, `JoinHandle` for shutdown.  All writes to
//! `vec_<kind>` and to `vector_projection_work` state are issued through
//! [`crate::writer::WriterActor`] — the actor never opens a second write
//! connection.
//!
//! Drop order invariant (enforced by field order in
//! [`crate::runtime::EngineRuntime`]):
//! readers (`coordinator`) → `writer` → `vector_actor` → `rebuild` → `lock`.
//! The vector actor drops BEFORE the rebuild actor so that any in-flight
//! writer submissions from its thread are already rejected by the time the
//! rebuild thread's connection is torn down.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde::Serialize;

use crate::AdminService;
use crate::EngineError;
use crate::embedder::BatchEmbedder;
use crate::writer::{
    VectorProjectionApplyRequest, VectorProjectionClaimRequest, VectorProjectionDiscard,
    VectorProjectionSuccess, VectorWorkClaim, WriterActor,
};

/// Target batch size for incremental (priority >= 1000) work rows.
pub(crate) const INCREMENTAL_BATCH: usize = 64;
/// Target batch size for backfill (priority < 1000) work rows.
pub(crate) const BACKFILL_SLICE: usize = 32;
/// Idle-loop polling interval.
const IDLE_POLL: Duration = Duration::from_millis(250);

/// Signals sent to the projection actor's channel.
#[derive(Debug)]
pub(crate) enum VectorWorkSignal {
    /// Best-effort wakeup notification.  Reserved for Pack F/G scheduler.
    #[allow(dead_code)]
    Wakeup,
    /// Terminate the actor loop.
    Shutdown,
}

/// Report returned from [`AdminService::drain_vector_projection`](crate::AdminService::drain_vector_projection).
#[derive(Clone, Debug, Default, Serialize)]
pub struct DrainReport {
    /// Number of incremental (priority >= 1000) work rows that produced a
    /// vec row in this drain.
    pub incremental_processed: u64,
    /// Number of backfill (priority < 1000) work rows that produced a vec
    /// row in this drain.
    pub backfill_processed: u64,
    /// Number of rows that produced a hard failure (e.g. embedder output
    /// wrong dimension).
    pub failed: u64,
    /// Number of rows whose `canonical_hash` mismatched the current chunk
    /// and were marked `discarded`.
    pub discarded_stale: u64,
    /// Number of ticks that were aborted because the embedder was
    /// unavailable.
    pub embedder_unavailable_ticks: u64,
}

/// Background actor that serializes projection work ticks.
#[derive(Debug)]
pub struct VectorProjectionActor {
    thread_handle: Option<thread::JoinHandle<()>>,
    sender: Option<mpsc::SyncSender<VectorWorkSignal>>,
}

impl VectorProjectionActor {
    /// Start the actor thread.
    ///
    /// The actor holds a clone of the writer-actor handle so it can submit
    /// claim/apply transactions.  It does NOT receive an embedder at
    /// construction time — production flows (Pack G) wire one via
    /// `AdminService::drain_vector_projection` or future hooks.  Until an
    /// embedder is provided, the actor loop idles.
    ///
    /// # Errors
    /// Returns [`EngineError::Io`] if the thread cannot be spawned.
    pub fn start(_writer: &WriterActor) -> Result<Self, EngineError> {
        let (sender, receiver) = mpsc::sync_channel::<VectorWorkSignal>(16);
        // The production loop currently only reacts to shutdown and tick
        // wakeups — actual drain work is driven on-demand by
        // `AdminService::drain_vector_projection` because embedders are
        // supplied at call-time.  This keeps the actor alive for drop-order
        // discipline and future scheduler work (Pack F/G).
        let handle = thread::Builder::new()
            .name("fathomdb-vector-projection".to_owned())
            .spawn(move || {
                vector_projection_loop(&receiver);
            })
            .map_err(EngineError::Io)?;
        Ok(Self {
            thread_handle: Some(handle),
            sender: Some(sender),
        })
    }
}

impl Drop for VectorProjectionActor {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            // Best-effort shutdown signal; ignore send failures (thread may
            // already be exiting).
            let _ = sender.try_send(VectorWorkSignal::Shutdown);
            drop(sender);
        }
        if let Some(handle) = self.thread_handle.take() {
            match handle.join() {
                Ok(()) => {}
                Err(payload) => {
                    if std::thread::panicking() {
                        trace_warn!(
                            "vector projection thread panicked during shutdown (suppressed: already panicking)"
                        );
                    } else {
                        std::panic::resume_unwind(payload);
                    }
                }
            }
        }
    }
}

fn vector_projection_loop(receiver: &mpsc::Receiver<VectorWorkSignal>) {
    trace_info!("vector projection thread started");
    loop {
        match receiver.recv_timeout(IDLE_POLL) {
            Ok(VectorWorkSignal::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            // Wakeup/timeout tick: no-op today. Production scheduling is
            // currently driven by admin drain calls; future commits will
            // invoke `run_tick` here once an embedder resolver is wired.
            Ok(VectorWorkSignal::Wakeup) | Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    trace_info!("vector projection thread exiting");
}

/// Run a single scheduling tick: claim up to `INCREMENTAL_BATCH` incremental
/// rows; if none found, claim up to `BACKFILL_SLICE` backfill rows.  Embed
/// them via `embedder`, apply the results through `writer`.
///
/// Returns the per-tick accounting used to build [`DrainReport`].
///
/// # Errors
/// Returns [`EngineError`] if the writer claim/apply steps fail.
#[allow(clippy::too_many_lines)]
pub(crate) fn run_tick(
    admin: &AdminService,
    writer: &WriterActor,
    embedder: &dyn BatchEmbedder,
) -> Result<TickReport, EngineError> {
    // Step 1: claim incremental first.
    let mut claims = writer.claim_vector_projection(VectorProjectionClaimRequest {
        min_priority: 1000,
        limit: INCREMENTAL_BATCH,
    })?;
    let mut is_incremental = true;

    if claims.is_empty() {
        claims = writer.claim_vector_projection(VectorProjectionClaimRequest {
            min_priority: i64::MIN,
            limit: BACKFILL_SLICE,
        })?;
        is_incremental = false;
    }

    if claims.is_empty() {
        return Ok(TickReport {
            processed_incremental: 0,
            processed_backfill: 0,
            failed: 0,
            discarded_stale: 0,
            embedder_unavailable: false,
            idle: true,
        });
    }

    // Step 2: determine which claims are immediately discardable
    // (hash mismatch, chunk missing, profile mismatch).
    let active_profile_id: Option<i64> = admin.active_embedding_profile_id()?;

    let mut successes: Vec<VectorProjectionSuccess> = Vec::new();
    let mut discards: Vec<VectorProjectionDiscard> = Vec::new();
    let mut embeddable: Vec<VectorWorkClaim> = Vec::new();

    for claim in claims {
        if claim.chunk_missing {
            discards.push(VectorProjectionDiscard {
                work_id: claim.work_id,
                reason: Some("chunk no longer exists".to_owned()),
            });
            continue;
        }
        let current_hash = crate::admin::canonical_chunk_hash(&claim.chunk_id, &claim.text_content);
        if current_hash != claim.canonical_hash {
            discards.push(VectorProjectionDiscard {
                work_id: claim.work_id,
                reason: Some("canonical_hash mismatch".to_owned()),
            });
            continue;
        }
        if let Some(pid) = active_profile_id
            && claim.embedding_profile_id != pid
        {
            discards.push(VectorProjectionDiscard {
                work_id: claim.work_id,
                reason: Some("embedding profile changed".to_owned()),
            });
            continue;
        }
        embeddable.push(claim);
    }

    // Step 3: embed (if any embeddable rows).
    let mut embedder_unavailable = false;
    let mut failed_count: u64 = 0;
    if !embeddable.is_empty() {
        let texts: Vec<String> = embeddable.iter().map(|c| c.text_content.clone()).collect();
        match embedder.batch_embed(&texts) {
            Ok(vectors) if vectors.len() == embeddable.len() => {
                let identity = embedder.identity();
                for (claim, vector) in embeddable.iter().zip(vectors) {
                    if vector.len() != identity.dimension || vector.iter().any(|v| !v.is_finite()) {
                        discards.push(VectorProjectionDiscard {
                            work_id: claim.work_id,
                            reason: Some("embedder returned invalid vector".to_owned()),
                        });
                        failed_count += 1;
                        continue;
                    }
                    successes.push(VectorProjectionSuccess {
                        work_id: claim.work_id,
                        kind: claim.kind.clone(),
                        chunk_id: claim.chunk_id.clone(),
                        embedding: vector,
                    });
                }
            }
            // Size-mismatch OR explicit error: treat as embedder failure and
            // revert claimed rows to pending.
            Ok(_) | Err(_) => {
                embedder_unavailable = true;
            }
        }
    }

    // Step 4: build the apply request (reverts = all embeddable rows if
    // embedder unavailable).
    let reverts: Vec<i64> = if embedder_unavailable {
        embeddable.iter().map(|c| c.work_id).collect()
    } else {
        Vec::new()
    };
    let revert_error = if embedder_unavailable {
        Some("embedder unavailable".to_owned())
    } else {
        None
    };

    let apply = VectorProjectionApplyRequest {
        successes,
        discards,
        reverts,
        revert_error,
    };

    let processed_successes = u64::try_from(apply.successes.len()).unwrap_or(0);
    let discarded = u64::try_from(apply.discards.len()).unwrap_or(0);

    writer.apply_vector_projection(apply)?;

    Ok(TickReport {
        processed_incremental: if is_incremental {
            processed_successes
        } else {
            0
        },
        processed_backfill: if is_incremental {
            0
        } else {
            processed_successes
        },
        failed: failed_count,
        discarded_stale: discarded - failed_count,
        embedder_unavailable,
        idle: false,
    })
}

/// Outcome of a single tick.
#[derive(Clone, Debug, Default)]
pub(crate) struct TickReport {
    pub processed_incremental: u64,
    pub processed_backfill: u64,
    pub failed: u64,
    pub discarded_stale: u64,
    pub embedder_unavailable: bool,
    pub idle: bool,
}
