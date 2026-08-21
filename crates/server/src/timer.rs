//! The server-owned effort timer — one per process, behind one lock.
//!
//! A running timer lives in the running app, never in a file: it
//! survives page reloads, is the same timer in every tab, and is lost
//! when the server stops. Its whole state is *which item* and *when
//! started* — elapsed time is current time minus start time, computed
//! fresh whenever asked, so nothing ever ticks in here.
//!
//! The single lock is what makes two tabs pressing stop at the same
//! moment safe: one stop takes the timer and writes, the other is told
//! no timer is running. [`TimerService::stop_with`] runs the write
//! *under* the lock — taking the timer and writing are one indivisible
//! step, so no double write is possible and a failed write leaves the
//! timer running (a transient failure must not discard measured time).

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};

use workdown_core::model::WorkItemId;

/// The clock the timer reads. Injected so tests never wait.
///
/// Wall-clock time, deliberately not the machine's monotonic uptime
/// counter: that one freezes while a laptop sleeps, and the forgotten
/// weekend timer must keep counting. The price is that a backwards
/// clock jump could make elapsed time negative — clamped to zero in
/// [`TimerService::snapshot`] instead.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// The real clock — what `workdown serve` runs on.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A hand-cranked clock for tests: starts where you set it, moves only
/// when told to (forwards or backwards).
pub struct ManualClock {
    now: Mutex<DateTime<Utc>>,
}

impl ManualClock {
    pub fn starting_at(now: DateTime<Utc>) -> Self {
        Self {
            now: Mutex::new(now),
        }
    }

    pub fn advance(&self, duration: chrono::Duration) {
        let mut now = self.now.lock().expect("clock lock never poisoned");
        *now += duration;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock().expect("clock lock never poisoned")
    }
}

/// A point-in-time view of the running timer, elapsed already computed.
#[derive(Debug, Clone)]
pub struct TimerSnapshot {
    pub item_id: WorkItemId,
    pub started_at: DateTime<Utc>,
    /// Seconds since start, clamped at zero against backwards clock jumps.
    pub elapsed_seconds: u64,
}

/// Why a stop produced no write result. `NotRunning` is the clean
/// refusal for stop with no timer; `Write` carries the caller's own
/// error out of [`TimerService::stop_with`] — the timer is still
/// running in that case.
#[derive(Debug)]
pub enum StopError<E> {
    NotRunning,
    Write(E),
}

struct Session {
    item_id: WorkItemId,
    started_at: DateTime<Utc>,
}

pub struct TimerService {
    clock: Arc<dyn Clock>,
    session: Mutex<Option<Session>>,
}

impl TimerService {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            session: Mutex::new(None),
        }
    }

    /// The production configuration: wall clock, no timer running.
    pub fn system() -> Self {
        Self::new(Arc::new(SystemClock))
    }

    /// The running timer right now, if any.
    pub fn snapshot(&self) -> Option<TimerSnapshot> {
        let session = self.session.lock().expect("timer lock never poisoned");
        session.as_ref().map(|session| self.snapshot_of(session))
    }

    /// Start timing an item. Refused when a timer already runs — even on
    /// the same item — because every write must be the result of an
    /// explicit stop; the refusal carries the running timer so the
    /// caller can name it.
    pub fn start(&self, item_id: WorkItemId) -> Result<TimerSnapshot, TimerSnapshot> {
        let mut session = self.session.lock().expect("timer lock never poisoned");
        if let Some(running) = session.as_ref() {
            return Err(self.snapshot_of(running));
        }
        let new_session = Session {
            item_id,
            started_at: self.clock.now(),
        };
        let snapshot = self.snapshot_of(&new_session);
        *session = Some(new_session);
        Ok(snapshot)
    }

    /// Stop the timer, running `write` under the lock. On `Ok` the timer
    /// is cleared; on `Err` it keeps running (stop again after fixing
    /// the cause, or start another timer to abandon the session
    /// deliberately). Returns the snapshot the write was based on
    /// alongside the write's own result.
    pub fn stop_with<T, E>(
        &self,
        write: impl FnOnce(&TimerSnapshot) -> Result<T, E>,
    ) -> Result<(TimerSnapshot, T), StopError<E>> {
        let mut session = self.session.lock().expect("timer lock never poisoned");
        let Some(running) = session.as_ref() else {
            return Err(StopError::NotRunning);
        };
        let snapshot = self.snapshot_of(running);
        match write(&snapshot) {
            Ok(result) => {
                *session = None;
                Ok((snapshot, result))
            }
            Err(error) => Err(StopError::Write(error)),
        }
    }

    fn snapshot_of(&self, session: &Session) -> TimerSnapshot {
        let elapsed = (self.clock.now() - session.started_at).num_seconds();
        TimerSnapshot {
            item_id: session.item_id.clone(),
            started_at: session.started_at,
            elapsed_seconds: u64::try_from(elapsed).unwrap_or(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn clock() -> Arc<ManualClock> {
        Arc::new(ManualClock::starting_at(
            Utc.with_ymd_and_hms(2026, 8, 21, 9, 0, 0).unwrap(),
        ))
    }

    fn id(value: &str) -> WorkItemId {
        WorkItemId::from(value.to_owned())
    }

    #[test]
    fn no_timer_means_no_snapshot() {
        let service = TimerService::new(clock());
        assert!(service.snapshot().is_none());
    }

    #[test]
    fn elapsed_is_now_minus_start() {
        let clock = clock();
        let service = TimerService::new(Arc::clone(&clock) as Arc<dyn Clock>);
        service.start(id("task-a")).unwrap();

        clock.advance(chrono::Duration::seconds(95));
        let snapshot = service.snapshot().unwrap();
        assert_eq!(snapshot.elapsed_seconds, 95);
        assert_eq!(snapshot.item_id.as_str(), "task-a");
    }

    #[test]
    fn backwards_clock_jump_clamps_elapsed_at_zero() {
        let clock = clock();
        let service = TimerService::new(Arc::clone(&clock) as Arc<dyn Clock>);
        service.start(id("task-a")).unwrap();

        clock.advance(chrono::Duration::seconds(-3600));
        assert_eq!(service.snapshot().unwrap().elapsed_seconds, 0);
    }

    #[test]
    fn start_while_running_is_refused_with_the_running_timer() {
        let service = TimerService::new(clock());
        service.start(id("task-a")).unwrap();

        let refused = service.start(id("task-b")).unwrap_err();
        assert_eq!(refused.item_id.as_str(), "task-a");
        // Same item too — a second start is never a silent restart.
        let refused = service.start(id("task-a")).unwrap_err();
        assert_eq!(refused.item_id.as_str(), "task-a");
    }

    #[test]
    fn stop_with_no_timer_is_refused() {
        let service = TimerService::new(clock());
        let result = service.stop_with(|_| Ok::<_, ()>(()));
        assert!(matches!(result, Err(StopError::NotRunning)));
    }

    #[test]
    fn successful_stop_clears_the_timer() {
        let clock = clock();
        let service = TimerService::new(Arc::clone(&clock) as Arc<dyn Clock>);
        service.start(id("task-a")).unwrap();
        clock.advance(chrono::Duration::seconds(120));

        let (snapshot, written) = service
            .stop_with(|snapshot| Ok::<_, ()>(snapshot.elapsed_seconds))
            .unwrap();
        assert_eq!(snapshot.elapsed_seconds, 120);
        assert_eq!(written, 120);
        assert!(service.snapshot().is_none());
    }

    #[test]
    fn failed_write_keeps_the_timer_running() {
        let clock = clock();
        let service = TimerService::new(Arc::clone(&clock) as Arc<dyn Clock>);
        service.start(id("task-a")).unwrap();
        clock.advance(chrono::Duration::seconds(60));

        let result = service.stop_with(|_| Err::<(), _>("disk on fire"));
        assert!(matches!(result, Err(StopError::Write("disk on fire"))));

        // Still running, still counting — measured time was not discarded.
        clock.advance(chrono::Duration::seconds(60));
        assert_eq!(service.snapshot().unwrap().elapsed_seconds, 120);

        // And a later stop succeeds with the full elapsed time.
        let (snapshot, _) = service.stop_with(|_| Ok::<_, ()>(())).unwrap();
        assert_eq!(snapshot.elapsed_seconds, 120);
        assert!(service.snapshot().is_none());
    }
}
