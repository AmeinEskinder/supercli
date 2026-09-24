//! SQL-lease claiming for scheduled triggers, with fencing tokens.
//!
//! Multiple scheduler workers (processes, or one day tenants of a control
//! plane) coordinate through a `schedule_leases` SQL table instead of an
//! in-process guard. A worker fires a schedule only while it holds that
//! schedule's lease:
//!
//! * **Claim** runs inside one `BEGIN IMMEDIATE` transaction: read the
//!   current row, then `INSERT` (fresh) or a conditional `UPDATE ... WHERE
//!   (owner = '' OR expires_at_ms <= <db-now>)` (takeover of a lapsed
//!   lease, or reclaim after a clean release). The expiry comparison
//!   lives inside SQL against the database clock; exactly one worker
//!   wins and the loser skips the trigger (single-flight across
//!   processes).
//! * **Fencing tokens.** Every successful claim bumps a monotonic
//!   `lease_generation`: 1 on a fresh insert, `previous + 1` on every
//!   later claim — takeovers and reclaims after a clean release alike.
//!   (The released row is kept as a tombstone precisely so the sequence
//!   never restarts at 1.) The claim returns the generation, and the
//!   scheduled runner carries it as a [`LeaseFence`]: before every
//!   side-effecting step — each connector tool call, the review/attempt
//!   writes, run completion — the worker checks one indexed SQL
//!   predicate (`tenant, schedule, owner, generation, expires_at_ms >
//!   <db-now>`) and aborts with a distinct stale-lease error unless it
//!   matches. This is what stops the classic split-brain: A claims gen 1,
//!   stalls past expiry (GC pause, suspended VM, slow disk), B takes over
//!   at gen 2 and fires; when A wakes, its fence no longer matches and
//!   every one of its tool calls is refused before executing — no
//!   review is written, no attempt recorded, no connector called.
//! * **Database-side time.** Expiry is compared and computed with the
//!   *database's* clock, never a `now` computed in Rust and passed in.
//!   On SQLite (this module's database, via `rusqlite`'s bundled SQLite)
//!   the current time in whole milliseconds is
//!   `CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)`
//!   (`strftime('%s','now')` has only second resolution, which is too
//!   coarse for short test TTLs; the `julianday` form is millisecond
//!   accurate). All lease decisions — the liveness check in `claim`, the
//!   new `expires_at_ms`, the `is_current` fence check — derive `now`
//!   from that expression inside SQL, so clock skew between application
//!   machines can never grant or steal a lease. Porting notes:
//!   * **Postgres:** replace the expression with
//!     `(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint`.
//!     Use `clock_timestamp()` (statement time), not `now()` /
//!     `transaction_timestamp()` (transaction start): a long-lived
//!     transaction must see expiry against the moment of the statement,
//!     not the moment the transaction began.
//!   * **D1 (Cloudflare):** D1 is SQLite-compatible, so the same
//!     `julianday` expression works — but keep the read of the row and
//!     the conditional write inside a *single* batch/transaction; D1
//!     applies a batch atomically, which is what preserves the
//!     exactly-one-winner guarantee.
//! * **Renew** extends a lease the worker still owns; **release**
//!   tombstones it on the clean path (the row stays, unowned, so the
//!   generation sequence stays monotonic). Renew checks `owner =
//!   worker_id`, so a worker can never extend a lease it lost. Neither
//!   renew nor release changes the generation; only a successful claim
//!   does.
//! * **Multi-tenant**: `(tenant, schedule_id)` is the primary key. The
//!   local daemon uses tenant `"default"`; a future control plane passes
//!   its own tenant and reuses the same table and protocol.
//!
//! Fail-safe posture: every error is returned, never swallowed. A worker
//! that cannot reach the lease database does not fire — firing without a
//! lease would break single-flight. [`LeaseFence::is_current`] fails
//! closed: any database error reads as stale.

use std::path::Path;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use rusqlite::{Connection, OptionalExtension};

/// Tenancy for the local single-user daemon. A control plane passes its
/// own tenant id and shares the table.
pub const DEFAULT_TENANT: &str = "default";

/// How long a claimed lease lasts without renewal (10 min). Runs are
/// bounded well under this; a crashed worker's lease lapsing is the
/// takeover mechanism.
pub const DEFAULT_LEASE_TTL_MS: u64 = 10 * 60 * 1000;

/// Path of the lease database: `<home>/schedule-leases.db`.
pub fn leases_db_path(home: &Path) -> std::path::PathBuf {
    home.join("schedule-leases.db")
}

/// Distinguishes `ScheduleLeases` instances inside one process: the
/// owner id is `pid<n>-<seq>`, unique per store as well as per process,
/// so two workers in one process (tests, or a future multi-scheduler
/// daemon) never share an owner identity.
static WORKER_SEQ: AtomicU64 = AtomicU64::new(0);

/// Why a lease operation failed.
#[derive(Debug)]
pub enum LeaseError {
    Sql(rusqlite::Error),
    /// The database file could not be created/opened.
    Open(String),
}

impl std::fmt::Display for LeaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LeaseError::Sql(e) => write!(f, "lease database: {e}"),
            LeaseError::Open(e) => write!(f, "cannot open lease database: {e}"),
        }
    }
}

impl std::error::Error for LeaseError {}
impl From<rusqlite::Error> for LeaseError {
    fn from(e: rusqlite::Error) -> Self {
        LeaseError::Sql(e)
    }
}

/// The database's own current time in whole milliseconds, as a SQL
/// expression. See the module docs for why this (and not a Rust-computed
/// `now`) drives every lease decision, and for the Postgres/D1
/// equivalents.
const DB_NOW_MS: &str = "(CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))";

/// Current contents of one lease row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseRow {
    pub owner: String,
    pub generation: u64,
    pub expires_at_ms: u64,
    pub claimed_at_ms: u64,
}

/// What a successful [`ScheduleLeases::claim`] won.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimInfo {
    /// The fencing-token generation this claim holds. Fresh claims start
    /// at 1; every takeover of a lapsed lease increments.
    pub generation: u64,
    /// `Some((previous_owner, previous_claimed_at_ms))` when this claim
    /// took over a lapsed lease; `None` for a fresh claim and for a
    /// reclaim after a clean release (that run finished — there is no
    /// lapsed run to inspect). The scheduler uses this for the P5-1
    /// takeover check: the lapsed run's attempt log is scanned for
    /// uncertain attempts that no later human-approved replacement has
    /// superseded.
    pub took_over: Option<(String, u64)>,
}

/// Fencing token proving a scheduled run still holds the lease it
/// claimed. The runner carries one per run; every side-effecting step
/// calls [`LeaseFence::is_current`] and aborts if the lease moved on.
///
/// Cheap to clone; the check is one indexed row read. `Send + Sync` so
/// executors stored in statics can carry one; the mutex never contends
/// in practice (the scheduler is single-threaded) and a poisoned lock
/// fails closed as stale.
#[derive(Clone)]
pub struct LeaseFence {
    leases: Arc<Mutex<ScheduleLeases>>,
    schedule_id: String,
    generation: u64,
}

impl LeaseFence {
    pub(crate) fn new(
        leases: Arc<Mutex<ScheduleLeases>>,
        schedule_id: String,
        generation: u64,
    ) -> Self {
        Self {
            leases,
            schedule_id,
            generation,
        }
    }

    /// True only if the lease row still shows this worker as owner with
    /// this generation, and the lease has not expired. One indexed SQL
    /// predicate checks tenant, schedule, owner, generation, and
    /// `expires_at_ms > <db-now>` atomically — no separate row read and
    /// clock read that could race. Fails closed: any database error, a
    /// missing row, a different owner/generation, an expired lease, or a
    /// poisoned lock reads as stale.
    pub fn is_current(&self) -> bool {
        let leases = match self.leases.lock() {
            Ok(guard) => guard,
            Err(_) => return false,
        };
        let count: i64 = match leases.conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM schedule_leases
                 WHERE tenant = ?1 AND schedule_id = ?2 AND owner = ?3
                   AND lease_generation = ?4 AND expires_at_ms > {DB_NOW_MS}"
            ),
            rusqlite::params![
                leases.tenant,
                self.schedule_id,
                leases.owner,
                self.generation as i64
            ],
            |row| row.get(0),
        ) {
            Ok(count) => count,
            Err(_) => return false,
        };
        count == 1
    }
}

/// Cross-process lease store over SQLite. Not `Sync`
/// (`rusqlite::Connection`); holders share it through
/// `Arc<std::sync::Mutex<_>>` (see [`LeaseFence`]).
pub struct ScheduleLeases {
    conn: Connection,
    tenant: String,
    owner: String,
    ttl_ms: u64,
}

impl ScheduleLeases {
    /// Open (creating) the lease database under `home`, as `tenant`.
    /// Refuses to continue when the database cannot be opened — the
    /// caller must not schedule without leases.
    pub fn open(home: &Path, tenant: &str) -> Result<Self, LeaseError> {
        let path = leases_db_path(home);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| LeaseError::Open(format!("{}: {e}", path.display())))?;
        }
        let conn = Connection::open(&path)
            .map_err(|e| LeaseError::Open(format!("{}: {e}", path.display())))?;
        let seq = WORKER_SEQ.fetch_add(1, Ordering::SeqCst);
        let store = Self {
            conn,
            tenant: tenant.to_string(),
            owner: format!("pid{}-{seq}", std::process::id()),
            ttl_ms: DEFAULT_LEASE_TTL_MS,
        };
        store.init()?;
        Ok(store)
    }

    /// Override the TTL (tests use short leases to simulate expiry).
    pub fn with_ttl(mut self, ttl_ms: u64) -> Self {
        self.ttl_ms = ttl_ms;
        self
    }

    /// This worker's owner id, for diagnostics.
    pub fn worker_id(&self) -> &str {
        &self.owner
    }

    fn init(&self) -> Result<(), LeaseError> {
        self.conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             CREATE TABLE IF NOT EXISTS schedule_leases (
                 tenant TEXT NOT NULL,
                 schedule_id TEXT NOT NULL,
                 owner TEXT NOT NULL,
                 expires_at_ms INTEGER NOT NULL,
                 claimed_at_ms INTEGER NOT NULL,
                 lease_generation INTEGER NOT NULL DEFAULT 1,
                 PRIMARY KEY (tenant, schedule_id)
             );",
        )?;
        // Migration for databases created before the fencing-token
        // column existed: `CREATE TABLE IF NOT EXISTS` alone will not
        // add it.
        let has_generation: bool = self.conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('schedule_leases')
             WHERE name = 'lease_generation'",
            [],
            |row| row.get(0),
        )?;
        if !has_generation {
            self.conn.execute(
                "ALTER TABLE schedule_leases
                 ADD COLUMN lease_generation INTEGER NOT NULL DEFAULT 1",
                [],
            )?;
        }
        Ok(())
    }

    /// Try to claim the lease for `schedule_id`.
    ///
    /// Runs in one `BEGIN IMMEDIATE` transaction. Every successful claim
    /// bumps `lease_generation` — 1 on a fresh insert, `previous + 1` on
    /// a takeover of a lapsed lease *and* on a reclaim after a clean
    /// release (the released row is kept as a tombstone precisely so the
    /// sequence stays monotonic). Returns `Ok(None)` when another worker
    /// holds a live lease — or when this worker already holds it
    /// (re-claiming a live lease is a no-op, never an extension).
    ///
    /// The liveness decision is a conditional write whose `WHERE`
    /// compares `expires_at_ms` against the *database* clock inside SQL;
    /// no Rust-computed `now` participates. The returned [`ClaimInfo`]
    /// carries the generation for the run's [`LeaseFence`] and, on
    /// takeover, the lapsed run's `(owner, claimed_at_ms)` for the P5-1
    /// attempt-log check.
    pub fn claim(&self, schedule_id: &str) -> Result<Option<ClaimInfo>, LeaseError> {
        // IMMEDIATE takes the write lock up front, so the read-then-write
        // below is atomic against other claimants: exactly one worker
        // wins. (The old single-statement upsert could not also return
        // the lapsed row's claimed_at, which the takeover check needs.)
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = self.claim_locked(schedule_id);
        match result {
            Ok(info) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(info)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    fn claim_locked(&self, schedule_id: &str) -> Result<Option<ClaimInfo>, LeaseError> {
        let prev: Option<(String, i64, i64)> = self
            .conn
            .query_row(
                "SELECT owner, lease_generation, claimed_at_ms
                 FROM schedule_leases
                 WHERE tenant = ?1 AND schedule_id = ?2",
                rusqlite::params![self.tenant, schedule_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        match prev {
            None => {
                // Fresh insert, generation 1. Timestamps come from the
                // database clock inside SQL, never from Rust.
                self.conn.execute(
                    &format!(
                        "INSERT INTO schedule_leases
                         (tenant, schedule_id, owner, expires_at_ms, claimed_at_ms, lease_generation)
                         VALUES (?1, ?2, ?3, {DB_NOW_MS} + ?4, {DB_NOW_MS}, 1)"
                    ),
                    rusqlite::params![self.tenant, schedule_id, self.owner, self.ttl_ms as i64],
                )?;
                Ok(Some(ClaimInfo {
                    generation: 1,
                    took_over: None,
                }))
            }
            Some((prev_owner, prev_generation, prev_claimed_at)) => {
                // Takeover of a lapsed lease, or reclaim of a cleanly
                // released one (tombstone: `owner = ''`). The expiry
                // comparison is the `WHERE` clause, evaluated by SQLite
                // against its own clock: `changed == 0` means another
                // worker holds a live lease and we lose. Under
                // BEGIN IMMEDIATE no other writer can interleave, so the
                // generation bump below agrees with the row.
                let changed = self.conn.execute(
                    &format!(
                        "UPDATE schedule_leases
                         SET owner = ?3,
                             expires_at_ms = {DB_NOW_MS} + ?4,
                             claimed_at_ms = {DB_NOW_MS},
                             lease_generation = lease_generation + 1
                         WHERE tenant = ?1 AND schedule_id = ?2
                           AND (owner = '' OR expires_at_ms <= {DB_NOW_MS})"
                    ),
                    rusqlite::params![self.tenant, schedule_id, self.owner, self.ttl_ms as i64],
                )?;
                if changed == 0 {
                    return Ok(None);
                }
                let generation = (prev_generation.max(0) as u64) + 1;
                // A clean release means the previous run finished: there
                // is no lapsed run to inspect.
                let took_over = if prev_owner.is_empty() {
                    None
                } else {
                    Some((prev_owner, prev_claimed_at.max(0) as u64))
                };
                Ok(Some(ClaimInfo {
                    generation,
                    took_over,
                }))
            }
        }
    }

    /// Extend a lease this worker still owns. Returns false when the
    /// lease is gone or belongs to someone else — a worker can never
    /// extend a lease it lost. Expiry is evaluated on the database
    /// clock.
    pub fn renew(&self, schedule_id: &str) -> Result<bool, LeaseError> {
        let changed = self.conn.execute(
            &format!(
                "UPDATE schedule_leases
                 SET expires_at_ms = {DB_NOW_MS} + ?3
                 WHERE tenant = ?1 AND schedule_id = ?2 AND owner = ?4
                   AND expires_at_ms > {DB_NOW_MS}"
            ),
            rusqlite::params![self.tenant, schedule_id, self.ttl_ms as i64, self.owner],
        )?;
        Ok(changed == 1)
    }

    /// Release the lease on the clean path. The row is kept as a
    /// tombstone (`owner = ''`, expiry zeroed) rather than deleted, so
    /// the next claim continues the monotonic `lease_generation`
    /// sequence instead of restarting at 1 — a stale fence from the
    /// released run can never validate against the reclaimed row. Only
    /// the owning worker's release takes effect: a stale worker's
    /// release is a silent no-op and can never drop the new holder's
    /// row. A tombstoned row is unowned: `renew` cannot extend it and
    /// `is_current` never matches it.
    pub fn release(&self, schedule_id: &str) -> Result<(), LeaseError> {
        self.conn.execute(
            "UPDATE schedule_leases
             SET owner = '', expires_at_ms = 0
             WHERE tenant = ?1 AND schedule_id = ?2 AND owner = ?3",
            rusqlite::params![self.tenant, schedule_id, self.owner],
        )?;
        Ok(())
    }

    /// Current row for a schedule, if any. Observability for operators;
    /// tests use it to assert takeover. [`LeaseFence::is_current`] is the
    /// primitive the runner actually gates on.
    pub fn lease_row(&self, schedule_id: &str) -> Result<Option<LeaseRow>, LeaseError> {
        self.conn
            .query_row(
                "SELECT owner, lease_generation, expires_at_ms, claimed_at_ms
                 FROM schedule_leases
                 WHERE tenant = ?1 AND schedule_id = ?2",
                rusqlite::params![self.tenant, schedule_id],
                |row| {
                    Ok(LeaseRow {
                        owner: row.get(0)?,
                        generation: row.get::<_, i64>(1)?.max(0) as u64,
                        expires_at_ms: row.get::<_, i64>(2)?.max(0) as u64,
                        claimed_at_ms: row.get::<_, i64>(3)?.max(0) as u64,
                    })
                },
            )
            .optional()
            .map_err(LeaseError::from)
    }

    /// Test hook: force a lease to lapse without waiting for the TTL.
    /// Production time always comes from the database clock; this just
    /// backdates the row's expiry so tests stay deterministic.
    #[cfg(test)]
    pub(crate) fn force_expire(&self, schedule_id: &str) -> Result<(), LeaseError> {
        self.conn.execute(
            &format!(
                "UPDATE schedule_leases
                 SET expires_at_ms = {DB_NOW_MS} - 1
                 WHERE tenant = ?1 AND schedule_id = ?2"
            ),
            rusqlite::params![self.tenant, schedule_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("unpeel-lease-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn claim_gives_generation_one_and_second_worker_loses() {
        let home = temp_home("gen1");
        let a = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        let b = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        let info = a.claim("sched").unwrap().expect("A wins the first claim");
        assert_eq!(info.generation, 1);
        assert_eq!(info.took_over, None);
        assert!(b.claim("sched").unwrap().is_none(), "B loses to live lease");
        // Re-claiming our own live lease is a no-op, not an extension.
        assert!(a.claim("sched").unwrap().is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn takeover_bumps_generation_and_reports_lapsed_run() {
        let home = temp_home("takeover");
        let a = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        let b = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        let first = a.claim("sched").unwrap().unwrap();
        assert_eq!(first.generation, 1);
        let lapsed_claimed_at = a.lease_row("sched").unwrap().unwrap().claimed_at_ms;
        a.force_expire("sched").unwrap();
        let second = b
            .claim("sched")
            .unwrap()
            .expect("B takes over the lapsed lease");
        assert_eq!(
            second.generation, 2,
            "takeover increments the fencing token"
        );
        let (prev_owner, prev_claimed_at) =
            second.took_over.expect("takeover reports the lapsed run");
        assert_eq!(prev_owner, a.worker_id());
        assert_eq!(prev_claimed_at, lapsed_claimed_at);
        let row = b.lease_row("sched").unwrap().unwrap();
        assert_eq!(row.owner, b.worker_id());
        assert_eq!(row.generation, 2);
        // A's fence is now stale: owner and generation both moved on.
        let fence_a = LeaseFence::new(Arc::new(Mutex::new(a)), "sched".to_string(), 1);
        assert!(!fence_a.is_current());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn fence_is_current_only_for_owner_generation_and_live_lease() {
        let home = temp_home("fence");
        let a = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        let info = a.claim("sched").unwrap().unwrap();
        let shared = Arc::new(Mutex::new(a));
        let fence = LeaseFence::new(Arc::clone(&shared), "sched".to_string(), info.generation);
        assert!(fence.is_current(), "fresh claim validates");
        // Wrong generation never validates.
        let wrong = LeaseFence::new(
            Arc::clone(&shared),
            "sched".to_string(),
            info.generation + 1,
        );
        assert!(!wrong.is_current());
        // Expiry alone (nobody took over yet) reads as stale: fail safe.
        shared.lock().unwrap().force_expire("sched").unwrap();
        assert!(!fence.is_current(), "expired lease is not current");
        // A released (tombstoned) row reads as stale: owner no longer matches.
        shared.lock().unwrap().release("sched").unwrap();
        assert!(!fence.is_current());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn renew_extends_only_owned_lease_without_touching_generation() {
        let home = temp_home("renew");
        let a = ScheduleLeases::open(&home, DEFAULT_TENANT)
            .unwrap()
            .with_ttl(60_000);
        let b = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        a.claim("sched").unwrap().unwrap();
        let before = a.lease_row("sched").unwrap().unwrap().expires_at_ms;
        assert!(a.renew("sched").unwrap());
        let after = a.lease_row("sched").unwrap().unwrap();
        assert!(after.expires_at_ms >= before, "renew extends the lease");
        assert_eq!(after.generation, 1, "renew never bumps the fencing token");
        // B cannot extend A's lease.
        assert!(!b.renew("sched").unwrap());
        a.force_expire("sched").unwrap();
        b.claim("sched").unwrap().unwrap();
        assert!(
            !a.renew("sched").unwrap(),
            "A cannot extend a lease it lost"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn release_tombstones_and_reclaim_continues_the_generation_sequence() {
        let home = temp_home("release");
        let a = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        let b = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        a.claim("sched").unwrap().unwrap();
        a.release("sched").unwrap();
        // The row survives as a tombstone: unowned, so fences and renews
        // fail against it, but the generation is preserved.
        let row = a.lease_row("sched").unwrap().expect("tombstone row kept");
        assert_eq!(row.owner, "");
        assert_eq!(row.generation, 1);
        assert!(!a.renew("sched").unwrap(), "cannot renew a released lease");
        // Reclaim after clean release continues the fencing sequence —
        // it must NOT restart at 1, or a stale fence from the released
        // run could validate against the reclaimed row.
        let reclaim = b.claim("sched").unwrap().expect("reclaim wins");
        assert_eq!(reclaim.generation, 2);
        assert_eq!(reclaim.took_over, None, "clean release: no lapsed run");
        // A stale worker's release cannot drop the new holder's row.
        a.release("sched").unwrap();
        let row = b.lease_row("sched").unwrap().unwrap();
        assert_eq!(row.owner, b.worker_id());
        assert_eq!(row.generation, 2);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn migration_adds_generation_to_pre_fencing_databases() {
        let home = temp_home("migrate");
        // Simulate a database created before the fencing-token column:
        // create the old schema by hand, then open through the store.
        let path = leases_db_path(&home);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schedule_leases (
                 tenant TEXT NOT NULL,
                 schedule_id TEXT NOT NULL,
                 owner TEXT NOT NULL,
                 expires_at_ms INTEGER NOT NULL,
                 claimed_at_ms INTEGER NOT NULL,
                 PRIMARY KEY (tenant, schedule_id)
             );
             INSERT INTO schedule_leases
                 (tenant, schedule_id, owner, expires_at_ms, claimed_at_ms)
             VALUES ('default', 'old', 'someone', 9999999999999, 1);",
        )
        .unwrap();
        drop(conn);
        let store = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        let row = store
            .lease_row("old")
            .unwrap()
            .expect("old row survives migration");
        assert_eq!(row.generation, 1, "backfilled default generation");
        // And the migrated row participates in fencing normally.
        store.force_expire("old").unwrap();
        let info = store.claim("old").unwrap().unwrap();
        assert_eq!(info.generation, 2);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn lease_time_comes_from_the_database_clock() {
        let home = temp_home("dbtime");
        let a = ScheduleLeases::open(&home, DEFAULT_TENANT).unwrap();
        a.claim("sched").unwrap().unwrap();
        let row = a.lease_row("sched").unwrap().unwrap();
        let app_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        // claimed_at comes from the DB clock: close to the app clock on
        // this machine, but never *passed in* by it.
        assert!(row.claimed_at_ms.abs_diff(app_now) < 5_000);
        assert!(row.expires_at_ms > row.claimed_at_ms);
        let _ = std::fs::remove_dir_all(&home);
    }
}

// Phase 9 H1 — model-based property test for the lease state machine.
// A pure-Rust reference model tracks (owner, generation, live) per
// schedule; a seeded RNG drives 2000 claim/renew/release/expire/fence
// operations across three workers and two schedules, and every
// operation's result plus the observable row must agree with the model.
// This pins: claim/renew/expire/fence semantics, generation monotonicity
// across takeovers AND clean-release reclaims (tombstones), and the
// took_over payload (lapsed owner + claimed_at) for the P5-1 check.
#[cfg(test)]
mod model_tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n.max(1) as u64) as usize
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct ModelRow {
        owner: Option<usize>,
        generation: u64,
        live: bool,
    }

    fn model_home() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("unpeel-lease-model-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lease_state_machine_matches_reference_model() {
        let mut rng = Rng(0xD1B5_4A35_7E97_6B21);
        let home = model_home();
        const N_WORKERS: usize = 3;
        const N_SCHEDS: usize = 2;
        let workers: Vec<Arc<Mutex<ScheduleLeases>>> = (0..N_WORKERS)
            .map(|_| {
                Arc::new(Mutex::new(
                    ScheduleLeases::open(&home, DEFAULT_TENANT)
                        .unwrap()
                        .with_ttl(3_600_000),
                ))
            })
            .collect();
        let ids: Vec<String> = workers
            .iter()
            .map(|w| w.lock().unwrap().worker_id().to_string())
            .collect();
        let mut model: Vec<Option<ModelRow>> = vec![None; N_SCHEDS];
        // Last generation each worker obtained per schedule (0 = never).
        let mut last_gen = vec![vec![0u64; N_SCHEDS]; N_WORKERS];

        for step in 0..2000 {
            let w = rng.below(N_WORKERS);
            let s = rng.below(N_SCHEDS);
            let sid = format!("sched-{s}");
            match rng.below(5) {
                // Claim.
                0 => {
                    let before = workers[w].lock().unwrap().lease_row(&sid).unwrap();
                    let got = workers[w].lock().unwrap().claim(&sid).unwrap();
                    let row = &mut model[s];
                    let expect: Option<(u64, Option<usize>)> = match row {
                        None => {
                            *row = Some(ModelRow {
                                owner: Some(w),
                                generation: 1,
                                live: true,
                            });
                            Some((1, None))
                        }
                        Some(r) if r.owner.is_none() || !r.live => {
                            let g = r.generation + 1;
                            let took = r.owner; // lapsed owner, or None on tombstone
                            *r = ModelRow {
                                owner: Some(w),
                                generation: g,
                                live: true,
                            };
                            Some((g, took))
                        }
                        _ => None,
                    };
                    match (got, expect) {
                        (None, None) => {}
                        (Some(info), Some((g, took))) => {
                            assert_eq!(info.generation, g, "step {step}: generation");
                            assert_eq!(
                                info.took_over.as_ref().map(|(o, _)| o),
                                took.map(|t| &ids[t]),
                                "step {step}: took_over owner"
                            );
                            if let (Some((_, prev_claimed)), Some(b)) =
                                (info.took_over.as_ref(), before.as_ref())
                            {
                                // The takeover payload must name the lapsed
                                // run's claimed_at for the P5-1 check.
                                assert_eq!(*prev_claimed, b.claimed_at_ms);
                                assert_eq!(b.owner, ids[took.unwrap()]);
                            } else {
                                assert!(
                                    info.took_over.is_none(),
                                    "step {step}: fresh/tombstone claim must not take over"
                                );
                            }
                            last_gen[w][s] = g;
                        }
                        (got, expect) => {
                            panic!("step {step}: claim({w},{s}) mismatch: got {got:?}, model {expect:?}")
                        }
                    }
                }
                // Renew.
                1 => {
                    let got = workers[w].lock().unwrap().renew(&sid).unwrap();
                    let expect = matches!(&model[s], Some(r) if r.owner == Some(w) && r.live);
                    assert_eq!(got, expect, "step {step}: renew({w},{s})");
                }
                // Release.
                2 => {
                    workers[w].lock().unwrap().release(&sid).unwrap();
                    if let Some(r) = &mut model[s] {
                        if r.owner == Some(w) {
                            // Tombstone: owner cleared, generation kept.
                            r.owner = None;
                            r.live = false;
                        }
                    }
                }
                // Expire (test-only clock jump).
                3 => {
                    workers[w].lock().unwrap().force_expire(&sid).unwrap();
                    if let Some(r) = &mut model[s] {
                        r.live = false;
                    }
                }
                // Fence check.
                _ => {
                    let gen = last_gen[w][s];
                    let fence = LeaseFence::new(Arc::clone(&workers[w]), sid.clone(), gen);
                    let got = fence.is_current();
                    let expect = matches!(&model[s], Some(r)
                        if r.owner == Some(w) && r.generation == gen && r.live);
                    assert_eq!(got, expect, "step {step}: fence({w},{s},gen={gen})");
                }
            }
            // After every op, the observable row must agree with the model.
            let row = workers[w].lock().unwrap().lease_row(&sid).unwrap();
            match (&row, &model[s]) {
                (None, None) => {}
                (Some(r), Some(m)) => {
                    assert_eq!(r.generation, m.generation, "step {step}: row generation");
                    match m.owner {
                        Some(o) => assert_eq!(r.owner, ids[o], "step {step}: row owner"),
                        None => assert!(
                            r.owner.is_empty(),
                            "step {step}: tombstone owner must be empty"
                        ),
                    }
                }
                (row, m) => {
                    panic!("step {step}: row/model shape mismatch: row={row:?} model={m:?}")
                }
            }
        }
        eprintln!("lease_state_machine_matches_reference_model: 2000 ops, model agrees");
        let _ = std::fs::remove_dir_all(&home);
    }
}
