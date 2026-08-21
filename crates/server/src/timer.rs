//! The server-owned effort timer — one per process, behind one lock.
//!
//! A running timer lives in the running app, never in a file: it
//! survives page reloads, is the same timer in every tab, and is lost
//! when the server stops. Its state is a phase — idle, a work interval
//! on an item, or a pomodoro break — plus the sticky mode of the last
//! started session. Elapsed time is current time minus the phase's
//! start, computed fresh whenever asked, so nothing ever ticks in
//! here: a countdown reaching zero changes nothing until a user acts.
//!
//! The single lock is what makes two tabs pressing stop at the same
//! moment safe: one stop takes the timer and writes, the other is told
//! no timer is running. [`TimerService::stop_with`] runs the write
//! *under* the lock — taking the timer and writing are one indivisible
//! step, so no double write is possible and a failed write leaves the
//! work interval running (a transient failure must not discard
//! measured time). For the same reason, start during a break is one
//! transition under the lock: the break ends and the work interval
//! begins with no moment in between.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};

use workdown_core::model::WorkItemId;
use workdown_core::timer_data::{rounded_write_seconds, TimerMode};

/// The clock the timer reads. Injected so tests never wait.
///
/// Wall-clock time, deliberately not the machine's monotonic uptime
/// counter: that one freezes while a laptop sleeps, and the forgotten
/// weekend timer must keep counting. The price is that a backwards
/// clock jump could make elapsed time negative — clamped to zero in
/// the snapshots instead.
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

/// A point-in-time view of the whole timer: the phase with elapsed
/// time already computed, and the sticky mode.
#[derive(Debug, Clone)]
pub struct TimerSnapshot {
    pub phase: PhaseSnapshot,
    pub last_mode: TimerMode,
}

/// The phase half of [`TimerSnapshot`].
#[derive(Debug, Clone)]
pub enum PhaseSnapshot {
    Idle,
    Work(WorkSnapshot),
    Break(BreakSnapshot),
}

/// A running work interval — stopwatch or pomodoro, per `mode`.
#[derive(Debug, Clone)]
pub struct WorkSnapshot {
    pub item_id: WorkItemId,
    pub started_at: DateTime<Utc>,
    /// Seconds since start, clamped at zero against backwards clock jumps.
    pub elapsed_seconds: u64,
    pub mode: TimerMode,
}

/// A running pomodoro break — counted, but timing no item and never
/// recorded.
#[derive(Debug, Clone)]
pub struct BreakSnapshot {
    /// The item whose stop began this break.
    pub followed_item: WorkItemId,
    pub started_at: DateTime<Utc>,
    pub elapsed_seconds: u64,
}

/// Why a stop produced no write result. `NotRunning` and `BreakRunning`
/// are the clean refusals — stop's contract is "take the work interval
/// and write effort", and a break has nothing to write. `Write` carries
/// the caller's own error out of [`TimerService::stop_with`] — the work
/// interval is still running in that case.
#[derive(Debug)]
pub enum StopError<E> {
    NotRunning,
    BreakRunning,
    Write(E),
}

/// Why ending a break was refused: there is no break — the timer is
/// idle, or a work interval runs (and stop is that one's exit).
#[derive(Debug)]
pub enum BreakEndError {
    NotRunning,
    WorkRunning,
}

enum Phase {
    Idle,
    Work {
        item_id: WorkItemId,
        started_at: DateTime<Utc>,
        mode: TimerMode,
    },
    Break {
        followed_item: WorkItemId,
        started_at: DateTime<Utc>,
    },
}

struct TimerMemory {
    phase: Phase,
    /// The mode of the last started session — stopwatch until a
    /// pomodoro session has ever started. Set by every start, changed
    /// by nothing else.
    last_mode: TimerMode,
}

pub struct TimerService {
    clock: Arc<dyn Clock>,
    memory: Mutex<TimerMemory>,
}

impl TimerService {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            memory: Mutex::new(TimerMemory {
                phase: Phase::Idle,
                last_mode: TimerMode::Stopwatch,
            }),
        }
    }

    /// The production configuration: wall clock, no timer running.
    pub fn system() -> Self {
        Self::new(Arc::new(SystemClock))
    }

    /// The whole timer right now.
    pub fn snapshot(&self) -> TimerSnapshot {
        let memory = self.memory.lock().expect("timer lock never poisoned");
        TimerSnapshot {
            phase: self.phase_snapshot(&memory.phase),
            last_mode: memory.last_mode,
        }
    }

    /// Start timing an item. During a break this is one transition —
    /// the break ends and the work interval begins. Refused while a
    /// work interval runs — even on the same item — because every write
    /// must be the result of an explicit stop; the refusal carries the
    /// running interval so the caller can name it.
    pub fn start(
        &self,
        item_id: WorkItemId,
        mode: TimerMode,
    ) -> Result<WorkSnapshot, WorkSnapshot> {
        let mut memory = self.memory.lock().expect("timer lock never poisoned");
        if let Phase::Work {
            item_id: running_item,
            started_at,
            mode: running_mode,
        } = &memory.phase
        {
            return Err(self.work_snapshot(running_item, *started_at, *running_mode));
        }
        let started_at = self.clock.now();
        memory.phase = Phase::Work {
            item_id: item_id.clone(),
            started_at,
            mode,
        };
        memory.last_mode = mode;
        Ok(self.work_snapshot(&item_id, started_at, mode))
    }

    /// Stop the work interval, running `write` under the lock. On `Ok`
    /// the timer moves on: to a break after a pomodoro interval whose
    /// write landed, to idle otherwise — the stopwatch always, and the
    /// assumed misclick, a pomodoro session whose rounded write is zero
    /// and which therefore wrote nothing and starts no break. On `Err`
    /// the work interval keeps running (stop again after fixing the
    /// cause, or start another timer to abandon the session
    /// deliberately). Returns the snapshot the write was based on
    /// alongside the write's own result.
    pub fn stop_with<T, E>(
        &self,
        write: impl FnOnce(&WorkSnapshot) -> Result<T, E>,
    ) -> Result<(WorkSnapshot, T), StopError<E>> {
        let mut memory = self.memory.lock().expect("timer lock never poisoned");
        let snapshot = match &memory.phase {
            Phase::Idle => return Err(StopError::NotRunning),
            Phase::Break { .. } => return Err(StopError::BreakRunning),
            Phase::Work {
                item_id,
                started_at,
                mode,
            } => self.work_snapshot(item_id, *started_at, *mode),
        };
        match write(&snapshot) {
            Ok(result) => {
                // The break begins as the write lands — but only when
                // something was written: rounded-to-zero means nothing
                // happened, and a stop that did nothing leaves nothing
                // behind.
                let begins_break = snapshot.mode == TimerMode::Pomodoro
                    && rounded_write_seconds(snapshot.elapsed_seconds) > 0;
                memory.phase = if begins_break {
                    Phase::Break {
                        followed_item: snapshot.item_id.clone(),
                        started_at: self.clock.now(),
                    }
                } else {
                    Phase::Idle
                };
                Ok((snapshot, result))
            }
            Err(error) => Err(StopError::Write(error)),
        }
    }

    /// End a running break: back to idle, nothing written. Refused in
    /// any other phase — a work interval's exit is stop.
    pub fn end_break(&self) -> Result<(), BreakEndError> {
        let mut memory = self.memory.lock().expect("timer lock never poisoned");
        match &memory.phase {
            Phase::Break { .. } => {
                memory.phase = Phase::Idle;
                Ok(())
            }
            Phase::Idle => Err(BreakEndError::NotRunning),
            Phase::Work { .. } => Err(BreakEndError::WorkRunning),
        }
    }

    fn phase_snapshot(&self, phase: &Phase) -> PhaseSnapshot {
        match phase {
            Phase::Idle => PhaseSnapshot::Idle,
            Phase::Work {
                item_id,
                started_at,
                mode,
            } => PhaseSnapshot::Work(self.work_snapshot(item_id, *started_at, *mode)),
            Phase::Break {
                followed_item,
                started_at,
            } => PhaseSnapshot::Break(BreakSnapshot {
                followed_item: followed_item.clone(),
                started_at: *started_at,
                elapsed_seconds: self.elapsed_since(*started_at),
            }),
        }
    }

    fn work_snapshot(
        &self,
        item_id: &WorkItemId,
        started_at: DateTime<Utc>,
        mode: TimerMode,
    ) -> WorkSnapshot {
        WorkSnapshot {
            item_id: item_id.clone(),
            started_at,
            elapsed_seconds: self.elapsed_since(started_at),
            mode,
        }
    }

    fn elapsed_since(&self, started_at: DateTime<Utc>) -> u64 {
        let elapsed = (self.clock.now() - started_at).num_seconds();
        u64::try_from(elapsed).unwrap_or(0)
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

    fn service_on(clock: &Arc<ManualClock>) -> TimerService {
        TimerService::new(Arc::clone(clock) as Arc<dyn Clock>)
    }

    fn id(value: &str) -> WorkItemId {
        WorkItemId::from(value.to_owned())
    }

    fn work(service: &TimerService) -> WorkSnapshot {
        match service.snapshot().phase {
            PhaseSnapshot::Work(snapshot) => snapshot,
            other => panic!("expected a work phase, found {other:?}"),
        }
    }

    fn running_break(service: &TimerService) -> BreakSnapshot {
        match service.snapshot().phase {
            PhaseSnapshot::Break(snapshot) => snapshot,
            other => panic!("expected a break, found {other:?}"),
        }
    }

    fn assert_idle(service: &TimerService) {
        assert!(matches!(service.snapshot().phase, PhaseSnapshot::Idle));
    }

    /// Start a pomodoro interval, run it past a writable length, and
    /// stop it — leaving the service in a break following `item`.
    fn run_into_a_break(service: &TimerService, clock: &ManualClock, item: &str) {
        service.start(id(item), TimerMode::Pomodoro).unwrap();
        clock.advance(chrono::Duration::seconds(25 * 60));
        service.stop_with(|_| Ok::<_, ()>(())).unwrap();
    }

    #[test]
    fn starts_idle_with_the_stopwatch_as_sticky_mode() {
        let service = TimerService::new(clock());
        let snapshot = service.snapshot();
        assert!(matches!(snapshot.phase, PhaseSnapshot::Idle));
        assert_eq!(snapshot.last_mode, TimerMode::Stopwatch);
    }

    #[test]
    fn elapsed_is_now_minus_start() {
        let clock = clock();
        let service = service_on(&clock);
        service.start(id("task-a"), TimerMode::Stopwatch).unwrap();

        clock.advance(chrono::Duration::seconds(95));
        let snapshot = work(&service);
        assert_eq!(snapshot.elapsed_seconds, 95);
        assert_eq!(snapshot.item_id.as_str(), "task-a");
        assert_eq!(snapshot.mode, TimerMode::Stopwatch);
    }

    #[test]
    fn backwards_clock_jump_clamps_elapsed_at_zero() {
        let clock = clock();
        let service = service_on(&clock);
        service.start(id("task-a"), TimerMode::Stopwatch).unwrap();

        clock.advance(chrono::Duration::seconds(-3600));
        assert_eq!(work(&service).elapsed_seconds, 0);
    }

    #[test]
    fn start_while_a_work_interval_runs_is_refused_with_the_running_one() {
        let service = TimerService::new(clock());
        service.start(id("task-a"), TimerMode::Stopwatch).unwrap();

        let refused = service
            .start(id("task-b"), TimerMode::Stopwatch)
            .unwrap_err();
        assert_eq!(refused.item_id.as_str(), "task-a");
        // Same item too — a second start is never a silent restart.
        let refused = service
            .start(id("task-a"), TimerMode::Pomodoro)
            .unwrap_err();
        assert_eq!(refused.item_id.as_str(), "task-a");
    }

    #[test]
    fn stop_with_no_timer_is_refused() {
        let service = TimerService::new(clock());
        let result = service.stop_with(|_| Ok::<_, ()>(()));
        assert!(matches!(result, Err(StopError::NotRunning)));
    }

    #[test]
    fn stopwatch_stop_returns_to_idle() {
        let clock = clock();
        let service = service_on(&clock);
        service.start(id("task-a"), TimerMode::Stopwatch).unwrap();
        clock.advance(chrono::Duration::seconds(120));

        let (snapshot, written) = service
            .stop_with(|snapshot| Ok::<_, ()>(snapshot.elapsed_seconds))
            .unwrap();
        assert_eq!(snapshot.elapsed_seconds, 120);
        assert_eq!(written, 120);
        assert_idle(&service);
    }

    #[test]
    fn failed_write_keeps_the_work_interval_running() {
        let clock = clock();
        let service = service_on(&clock);
        service.start(id("task-a"), TimerMode::Stopwatch).unwrap();
        clock.advance(chrono::Duration::seconds(60));

        let result = service.stop_with(|_| Err::<(), _>("disk on fire"));
        assert!(matches!(result, Err(StopError::Write("disk on fire"))));

        // Still running, still counting — measured time was not discarded.
        clock.advance(chrono::Duration::seconds(60));
        assert_eq!(work(&service).elapsed_seconds, 120);

        // And a later stop succeeds with the full elapsed time.
        let (snapshot, _) = service.stop_with(|_| Ok::<_, ()>(())).unwrap();
        assert_eq!(snapshot.elapsed_seconds, 120);
        assert_idle(&service);
    }

    #[test]
    fn pomodoro_stop_begins_a_break_following_the_item() {
        let clock = clock();
        let service = service_on(&clock);
        service.start(id("task-a"), TimerMode::Pomodoro).unwrap();
        // 32 minutes — overrun is stopped like any other interval.
        clock.advance(chrono::Duration::seconds(32 * 60));

        let (snapshot, _) = service.stop_with(|_| Ok::<_, ()>(())).unwrap();
        assert_eq!(snapshot.elapsed_seconds, 32 * 60);

        // The break begins as the write lands and counts from there.
        let snapshot = running_break(&service);
        assert_eq!(snapshot.followed_item.as_str(), "task-a");
        assert_eq!(snapshot.elapsed_seconds, 0);
        clock.advance(chrono::Duration::seconds(60));
        assert_eq!(running_break(&service).elapsed_seconds, 60);
    }

    #[test]
    fn pomodoro_stop_under_half_a_minute_goes_idle_without_a_break() {
        let clock = clock();
        let service = service_on(&clock);
        service.start(id("task-a"), TimerMode::Pomodoro).unwrap();
        clock.advance(chrono::Duration::seconds(29));

        service.stop_with(|_| Ok::<_, ()>(())).unwrap();
        // The write rounded to zero: nothing happened, nothing remains.
        assert_idle(&service);
    }

    #[test]
    fn failed_pomodoro_write_keeps_the_work_interval_and_starts_no_break() {
        let clock = clock();
        let service = service_on(&clock);
        service.start(id("task-a"), TimerMode::Pomodoro).unwrap();
        clock.advance(chrono::Duration::seconds(60));

        let result = service.stop_with(|_| Err::<(), _>("disk on fire"));
        assert!(matches!(result, Err(StopError::Write("disk on fire"))));
        assert_eq!(work(&service).item_id.as_str(), "task-a");
    }

    #[test]
    fn stop_during_a_break_is_refused() {
        let clock = clock();
        let service = service_on(&clock);
        run_into_a_break(&service, &clock, "task-a");

        let result = service.stop_with(|_| Ok::<_, ()>(()));
        assert!(matches!(result, Err(StopError::BreakRunning)));
        // The break survived the refused stop.
        assert_eq!(running_break(&service).followed_item.as_str(), "task-a");
    }

    #[test]
    fn end_break_returns_to_idle_and_only_a_break_can_end() {
        let clock = clock();
        let service = service_on(&clock);

        // Idle: nothing to end.
        assert!(matches!(
            service.end_break(),
            Err(BreakEndError::NotRunning)
        ));

        // A work interval's exit is stop, not break end.
        service.start(id("task-a"), TimerMode::Stopwatch).unwrap();
        assert!(matches!(
            service.end_break(),
            Err(BreakEndError::WorkRunning)
        ));
        service.stop_with(|_| Ok::<_, ()>(())).unwrap();

        run_into_a_break(&service, &clock, "task-a");
        service.end_break().unwrap();
        assert_idle(&service);
    }

    #[test]
    fn start_during_a_break_is_one_transition_into_work() {
        let clock = clock();
        let service = service_on(&clock);
        run_into_a_break(&service, &clock, "task-a");

        // Any item works — the followed one is a default, not a rule.
        let started = service.start(id("task-b"), TimerMode::Pomodoro).unwrap();
        assert_eq!(started.item_id.as_str(), "task-b");
        assert_eq!(work(&service).item_id.as_str(), "task-b");
    }

    #[test]
    fn the_sticky_mode_follows_every_start_and_nothing_else() {
        let clock = clock();
        let service = service_on(&clock);

        service.start(id("task-a"), TimerMode::Stopwatch).unwrap();
        assert_eq!(service.snapshot().last_mode, TimerMode::Stopwatch);
        clock.advance(chrono::Duration::seconds(60));
        service.stop_with(|_| Ok::<_, ()>(())).unwrap();

        run_into_a_break(&service, &clock, "task-a");
        assert_eq!(service.snapshot().last_mode, TimerMode::Pomodoro);
        // Neither the stop's break nor ending it changes the mode.
        service.end_break().unwrap();
        assert_eq!(service.snapshot().last_mode, TimerMode::Pomodoro);

        service.start(id("task-a"), TimerMode::Stopwatch).unwrap();
        assert_eq!(service.snapshot().last_mode, TimerMode::Stopwatch);
    }
}
