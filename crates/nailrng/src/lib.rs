//! A very fast, user-space RNG source in the same vein as `rand`'s `ThreadRng`. Not cryptographically secure,
//! is meant to be a very fast entropy source.

use std::cell::UnsafeCell;

use rand_core::RngCore;
use wyrand::WyRand;

thread_local! {
    static SOURCE: UnsafeCell<WyRand> = UnsafeCell::new(WyRand::new(getrandom::u64().expect("Failed to source entropy")))
}

pub struct FastRng(WyRand);

impl FastRng {
    #[inline]
    pub fn fork(&mut self) -> Self {
        Self(WyRand::new(self.next_u64()))
    }
}

impl Default for FastRng {
    fn default() -> Self {
        SOURCE.with(|source| {
            // SAFETY: Dereferencing this cell is safe as the value has
            // been initialised, so it will not be null, and the mut reference
            // we create here only lives as long as this function's scope. Since
            // this is thread local, there is only one mut reference alive at any
            // given moment.
            let ptr = unsafe { &mut *source.get() };

            FastRng(WyRand::new(ptr.rand()))
        })
    }
}

impl RngCore for FastRng {
    #[inline(always)]
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    #[inline(always)]
    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    #[inline(always)]
    fn fill_bytes(&mut self, dst: &mut [u8]) {
        self.0.fill_bytes(dst);
    }
}
