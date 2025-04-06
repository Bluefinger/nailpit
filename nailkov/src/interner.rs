use std::ops::Deref;

use hashbrown::{Equivalent, HashMap};
use wyrand::RandomWyHashState;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct InternedString(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StringPtr(*const str);

impl StringPtr {
    fn cast(&self) -> &str {
        // SAFETY: The pointer is stable as it points to memory that is never
        // moved/invalidated while this struct lives, therefore can be safely
        // dereferenced back to a string slice.
        unsafe { &*self.0 }
    }
}

impl core::hash::Hash for StringPtr {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.cast().hash(state);
    }
}

unsafe impl Send for StringPtr {}
unsafe impl Sync for StringPtr {}

impl Deref for StringPtr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.cast()
    }
}

impl AsRef<str> for StringPtr {
    fn as_ref(&self) -> &str {
        self.cast()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Interner {
    collected: HashMap<StringPtr, InternedString, RandomWyHashState>,
    vec: Vec<StringPtr>,
    active: usize,
    buffers: Vec<String>,
}

impl Default for Interner {
    fn default() -> Self {
        Self::with_capacity(256)
    }
}

impl Interner {
    pub fn lookup(&self, id: impl Into<InternedString>) -> Option<&str> {
        self.vec.get(id.into().0 as usize).map(StringPtr::cast)
    }

    pub fn with_capacity(cap: usize) -> Interner {
        assert!(cap <= u32::MAX as usize);
        let cap = cap.next_power_of_two();
        Interner {
            collected: HashMap::with_hasher(RandomWyHashState::new()),
            vec: Vec::new(),
            buffers: vec![String::with_capacity(cap)],
            active: 0,
        }
    }

    pub fn intern(&mut self, name: &str) -> InternedString {
        if let Some(&id) = self.collected.get(name) {
            return id;
        }

        let name = unsafe { self.alloc(name) };
        let id = InternedString(self.collected.len() as u32);
        self.collected.insert(name, id);
        self.vec.push(name);

        debug_assert!(self.lookup(id).is_some_and(|id| id.equivalent(&name)));
        debug_assert!(self.intern(name.cast()) == id);

        id
    }

    unsafe fn alloc(&mut self, name: &str) -> StringPtr {
        assert!(name.len() < u32::MAX as usize);

        let cur_buf = &self.buffers[self.active];

        let cap = cur_buf.capacity();

        if cap < cur_buf.len() + name.len() {
            let new_cap = (cap.max(name.len()) + 1)
                .next_power_of_two()
                .min(u32::MAX as usize);
            let new_buf = String::with_capacity(new_cap);
            self.active = self.buffers.len();
            self.buffers.push(new_buf);
        }

        // Construct raw pointer to eliminate lifetime tracking
        let interned = {
            let active_buf = &mut self.buffers[self.active];
            let start = active_buf.len();
            active_buf.push_str(name);

            &raw const active_buf[start..]
        };

        StringPtr(interned)
    }
}

impl Equivalent<StringPtr> for str {
    fn equivalent(&self, key: &StringPtr) -> bool {
        key.cast().eq(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_able_to_intern_one_string() {
        let mut interner = Interner::default();

        let text = "Lorem ipsum";

        let id = interner.intern(text);

        assert_eq!(Some(text), interner.lookup(id));

        let again = interner.intern(text);

        assert_eq!(id, again);
    }

    #[test]
    fn is_able_to_intern_many_strings() {
        let mut interner = Interner::with_capacity(32);

        let texts = [
            "Lorem ipsum",
            "dolor sit amet",
            "duplicated",
            "Other text",
            "Elevenses",
            "duplicated",
            "Gibberish",
        ];

        let interned: Vec<InternedString> =
            texts.iter().map(|&text| interner.intern(text)).collect();

        assert_eq!(
            interned.as_slice(),
            &[
                InternedString(0),
                InternedString(1),
                InternedString(2),
                InternedString(3),
                InternedString(4),
                InternedString(2),
                InternedString(5)
            ]
        )
    }

    #[test]
    fn is_thread_safe() {
        let mut interner = Interner::with_capacity(32);

        let texts = [
            "Lorem ipsum",
            "dolor sit amet",
            "duplicated",
            "Other text",
            "Elevenses",
            "duplicated",
            "Gibberish",
        ];

        let interned: Vec<InternedString> =
            texts.iter().map(|&text| interner.intern(text)).collect();

        std::thread::scope(|s| {
            s.spawn(move || {
                for (id, text) in interned.into_iter().zip(texts) {
                    assert_eq!(Some(text), interner.lookup(id));
                }
            });
        });
    }
}
