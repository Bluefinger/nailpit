use std::time::Instant;

use parking_lot::Mutex;

use crate::PEER_TIMEOUT;

pub(crate) struct Scheduler {
    value: Mutex<Option<Instant>>,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }

    #[inline]
    pub(crate) fn schedule<F, T>(&self, task: F) -> Option<T>
    where
        F: Fn() -> T,
    {
        let mut inner = self.value.lock();
        let elapsed = inner.get_or_insert_with(Instant::now).elapsed();

        if elapsed >= PEER_TIMEOUT {
            inner.take();
            drop(inner);

            Some(task())
        } else {
            None
        }
    }
}

pub(crate) static PRUNING_SCHEDULER: Scheduler = Scheduler::new();
