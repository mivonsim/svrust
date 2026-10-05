// Tanggung jawab: unit test event scheduler.
#![cfg(test)]

use crate::scheduler::{EventKind, ProcessId, Region, Scheduler};
use crate::time::SimTime;

fn process_event(id: ProcessId) -> EventKind {
    EventKind::Process(id)
}

#[test]
fn starts_idle_at_time_zero() {
    let scheduler = Scheduler::new();
    assert!(scheduler.is_idle());
    assert_eq!(scheduler.now(), SimTime::ZERO);
}

#[test]
fn active_runs_before_nba() {
    let mut scheduler = Scheduler::new();
    scheduler.schedule_at(SimTime::ZERO, Region::NonBlocking, process_event(1));
    scheduler.schedule_at(SimTime::ZERO, Region::Active, process_event(0));

    assert_eq!(scheduler.pop_next().unwrap().kind, process_event(0));
    assert_eq!(scheduler.pop_next().unwrap().kind, process_event(1));
    assert!(scheduler.pop_next().is_none());
}

#[test]
fn time_advances_to_next_scheduled_event() {
    let mut scheduler = Scheduler::new();
    scheduler.schedule_at(SimTime::from_nanos(100), Region::Active, process_event(7));

    let event = scheduler.pop_next().unwrap();
    assert_eq!(event.time, SimTime::from_nanos(100));
    assert_eq!(scheduler.now(), SimTime::from_nanos(100));
}

#[test]
fn earlier_time_is_processed_first() {
    let mut scheduler = Scheduler::new();
    scheduler.schedule_at(SimTime::from_nanos(20), Region::Active, process_event(2));
    scheduler.schedule_at(SimTime::from_nanos(10), Region::Active, process_event(1));

    assert_eq!(scheduler.pop_next().unwrap().kind, process_event(1));
    assert_eq!(scheduler.pop_next().unwrap().kind, process_event(2));
}

#[test]
fn insertion_order_breaks_ties() {
    let mut scheduler = Scheduler::new();
    for id in 0..4 {
        scheduler.schedule_at(SimTime::ZERO, Region::Active, process_event(id));
    }
    for id in 0..4 {
        assert_eq!(scheduler.pop_next().unwrap().kind, process_event(id));
    }
}

#[test]
fn nba_generated_during_active_still_runs_same_timestep() {
    let mut scheduler = Scheduler::new();
    scheduler.schedule_at(SimTime::ZERO, Region::Active, process_event(0));

    scheduler.pop_next();
    scheduler.schedule_at(scheduler.now(), Region::NonBlocking, process_event(1));

    let next = scheduler.pop_next().unwrap();
    assert_eq!(next.kind, process_event(1));
    assert_eq!(next.time, SimTime::ZERO);
}

#[test]
fn past_time_is_clamped_to_now() {
    let mut scheduler = Scheduler::new();
    scheduler.schedule_at(SimTime::from_nanos(50), Region::Active, process_event(0));
    scheduler.pop_next();

    scheduler.schedule_at(SimTime::from_nanos(10), Region::Active, process_event(1));
    let event = scheduler.pop_next().unwrap();
    assert_eq!(event.time, SimTime::from_nanos(50));
}

#[test]
fn pending_count_reflects_queue_state() {
    let mut scheduler = Scheduler::new();
    assert_eq!(scheduler.total_pending(), 0);

    scheduler.schedule_at(SimTime::ZERO, Region::Active, process_event(0));
    assert_eq!(scheduler.pending_in(Region::Active), 1);
    assert_eq!(scheduler.total_pending(), 1);
}

#[test]
fn becomes_idle_after_draining() {
    let mut scheduler = Scheduler::new();
    scheduler.schedule_at(SimTime::from_nanos(5), Region::Active, process_event(0));
    scheduler.schedule_at(SimTime::from_nanos(5), Region::Postponed, process_event(1));

    assert!(scheduler.pop_next().is_some());
    assert!(scheduler.pop_next().is_some());
    assert!(scheduler.is_idle());
}
