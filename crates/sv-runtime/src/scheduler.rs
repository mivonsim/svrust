// Tanggung jawab: event scheduler dengan region active/NBA/postponed.
use crate::time::SimTime;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

pub type ProcessId = usize;
pub type SignalId = usize;

/// Region event sesuai IEEE 1800 bab 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Region {
    Active,
    Inactive,
    NonBlocking,
    Monitor,
    Postponed,
}

impl Region {
    pub fn index(self) -> usize {
        self as usize
    }

    pub const ALL: [Region; 5] = [
        Region::Active,
        Region::Inactive,
        Region::NonBlocking,
        Region::Monitor,
        Region::Postponed,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Process(ProcessId),
    SignalWrite(SignalId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledEvent {
    pub time: SimTime,
    pub region: Region,
    pub seq: u64,
    pub kind: EventKind,
}

// BinaryHeap adalah max-heap. Balik urutan agar event paling awal keluar pertama.
impl Ord for ScheduledEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .time
            .cmp(&self.time)
            .then_with(|| other.region.cmp(&self.region))
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

impl PartialOrd for ScheduledEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Default)]
struct RegionQueue {
    events: VecDeque<ScheduledEvent>,
}

impl RegionQueue {
    fn push(&mut self, event: ScheduledEvent) {
        self.events.push_back(event);
    }

    fn pop(&mut self) -> Option<ScheduledEvent> {
        self.events.pop_front()
    }

    fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// Scheduler eventSimulation: Manage region delta cycle dan lompatan waktu.
#[derive(Debug)]
pub struct Scheduler {
    current_time: SimTime,
    seq: u64,
    regions: Vec<RegionQueue>,
    future: BinaryHeap<ScheduledEvent>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            current_time: SimTime::ZERO,
            seq: 0,
            regions: Region::ALL.iter().map(|_| RegionQueue::default()).collect(),
            future: BinaryHeap::new(),
        }
    }

    pub fn now(&self) -> SimTime {
        self.current_time
    }

    fn next_seq(&mut self) -> u64 {
        let seq = self.seq;
        self.seq += 1;
        seq
    }

    /// Jadwalkan event pada waktu tertentu.
    /// Waktu di masa lalu dijepit ke waktu sekarang agar simulasi deterministik.
    pub fn schedule_at(&mut self, time: SimTime, region: Region, kind: EventKind) {
        let when = if time < self.current_time {
            self.current_time
        } else {
            time
        };
        let event = ScheduledEvent {
            time: when,
            region,
            seq: self.next_seq(),
            kind,
        };
        if when == self.current_time {
            self.regions[region.index()].push(event);
        } else {
            self.future.push(event);
        }
    }

    pub fn schedule_after(&mut self, delta: SimTime, region: Region, kind: EventKind) {
        let when = self.current_time.saturating_add(delta);
        self.schedule_at(when, region, kind);
    }

    fn move_to_active(&mut self, region: Region) {
        let source = &mut self.regions[region.index()];
        let drained: Vec<ScheduledEvent> = source.events.drain(..).collect();
        let target = &mut self.regions[Region::Active.index()];
        target.events.extend(drained);
    }

    /// Ambil event berikutnya yang harus dieksekusi.
    /// Mengembalikan None saat simulasi sudah-quiescent.
    pub fn pop_next(&mut self) -> Option<ScheduledEvent> {
        loop {
            if let Some(event) = self.regions[Region::Active.index()].pop() {
                return Some(event);
            }

            let mut promoted = false;
            for region in [
                Region::Inactive,
                Region::NonBlocking,
                Region::Monitor,
                Region::Postponed,
            ] {
                if !self.regions[region.index()].is_empty() {
                    self.move_to_active(region);
                    promoted = true;
                    break;
                }
            }
            if promoted {
                continue;
            }

            let event = self.future.pop()?;
            self.current_time = event.time;
            let idx = event.region.index();
            self.regions[idx].push(event);
        }
    }

    pub fn is_idle(&self) -> bool {
        self.regions.iter().all(|q| q.is_empty()) && self.future.is_empty()
    }

    pub fn pending_in(&self, region: Region) -> usize {
        self.regions[region.index()].events.len()
    }

    pub fn total_pending(&self) -> usize {
        self.regions.iter().map(|q| q.events.len()).sum::<usize>() + self.future.len()
    }
}
