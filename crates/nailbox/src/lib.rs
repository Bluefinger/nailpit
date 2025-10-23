//! Utility crate for deferred initialisation of allocated containers, to lessen stack pressure
//! and initialise values directly on the heap. Rust _does_ optimise boxing to not require the
//! stack and can initialise directly to the heap, but this does not occur all the time. These
//! utils ensure this does happen, by deferring the initialisation of the allocation explicitly.

use core::pin::Pin;
use std::sync::Arc;

/// Util for deferred initialisation of boxed futures, to lessen stack pressure and initialise
/// the future directly on the heap
#[inline]
pub fn boxed_future_within<'a, T, F, Fut>(fut: F) -> Pin<Box<Fut>>
where
    Fut: Future<Output = T> + Send + 'a,
    F: FnOnce() -> Fut,
{
    let mut boxed = Box::new_uninit();

    boxed.write(fut());

    // SAFETY: We have initialised the Box correctly by writing to
    // the memory in the line above.
    unsafe { Box::into_pin(boxed.assume_init()) }
}

#[inline(always)]
pub fn try_arc_within<T, F, E>(f: F) -> Result<Arc<T>, E>
where
    F: FnOnce() -> Result<T, E>,
{
    let mut arced = Arc::new_uninit();

    Arc::get_mut(&mut arced).unwrap().write(f()?);

    // SAFETY: We have initialised the Arc correctly by writing to
    // the memory in the line above.
    Ok(unsafe { arced.assume_init() })
}

#[inline(always)]
pub fn arc_within<T, F>(f: F) -> Arc<T>
where
    F: FnOnce() -> T,
{
    let mut arced = Arc::new_uninit();

    Arc::get_mut(&mut arced).unwrap().write(f());

    // SAFETY: We have initialised the Arc correctly by writing to
    // the memory in the line above.
    unsafe { arced.assume_init() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxed_futures() {
        let boxed = boxed_future_within(async || { 42 });

        assert_eq!(size_of_val(&boxed), 8);
    }

    #[test]
    fn arc_init() {
        let arced = arc_within(|| 128u128);

        assert_ne!(size_of_val(&arced), 16);
        assert_eq!(*arced, 128);
    }

    #[test]
    fn try_arc_init() {
        #[derive(Debug, PartialEq, Eq)]
        struct Errored;

        let arced: Result<Arc<u128>, Errored> = try_arc_within(|| Ok(128u128));

        assert_eq!(*arced.unwrap(), 128);

        let arced: Result<Arc<u128>, Errored> = try_arc_within(|| Err(Errored));

        assert_eq!(arced, Err(Errored));
    }
}
