use std::{cell::UnsafeCell, ptr::NonNull, rc::Rc};

use rand_core::RngCore;
use wyrand::WyRand;

thread_local! {
    static SOURCE: Rc<UnsafeCell<WyRand>> = Rc::new(UnsafeCell::new(WyRand::new(getrandom::u64().expect("Failed to source entropy"))))
}

pub struct FastRng(WyRand);

impl Default for FastRng {
    fn default() -> Self {
        SOURCE.with(|source| {
            let mut ptr = unsafe { NonNull::new_unchecked(source.get()) };

            FastRng(WyRand::new(unsafe { ptr.as_mut().rand() }))
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
