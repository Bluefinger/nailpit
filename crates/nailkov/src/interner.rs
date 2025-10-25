use hashbrown::{Equivalent, HashMap};
use rapidhash::fast::RandomState;

use crate::token::Token;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct StringPtr(*const str);

impl StringPtr {
    #[inline(always)]
    const fn cast(&self) -> &str {
        // SAFETY: The pointer is stable as it points to memory that is never
        // moved/invalidated while this struct lives, therefore can be safely
        // dereferenced back to a string slice. We own the String instance this
        // references, and all StringPtrs are used within the same scope as the
        // String instances, so when String drops, these will be dropped too.
        unsafe { &*self.0 }
    }
}

impl core::hash::Hash for StringPtr {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.cast().hash(state);
    }
}
// SAFETY: StringPtr contains a ptr to the heap, that is never moved or invalidated
// while Interner lives, and all instances of StringPtr live as long as Interner.
// Since the String type is `Send`, so is StringPtr
unsafe impl Send for StringPtr {}
// SAFETY: StringPtr contains a ptr to the heap, that is never moved or invalidated
// while Interner lives, and all instances of StringPtr live as long as Interner.
// Since the String type is `Sync`, so is StringPtr
unsafe impl Sync for StringPtr {}

#[derive(Debug, Clone)]
pub struct Interner {
    collected: HashMap<StringPtr, Token, RandomState>,
    index: Vec<StringPtr>,
    buffer: String,
    stored: Vec<String>,
}

impl Default for Interner {
    fn default() -> Self {
        Self::with_capacity(256)
    }
}

impl Interner {
    /// # Safety
    /// The caller must ensure that the [`Token`] being passed in was allocated
    /// from the same [`Interner`] instance.
    #[inline(always)]
    pub unsafe fn lookup(&self, id: Token) -> &str {
        // SAFETY: Safety is upheld by the caller ensuring the id was allocated
        // from the same interner.
        unsafe { self.index.get_unchecked(id.index()).cast() }
    }

    pub fn with_capacity(cap: usize) -> Interner {
        // This will get us just under 64KiB of interned storage before we
        // need to allocate more space for buffer storage.
        let stored = Vec::with_capacity(8);

        Interner {
            collected: HashMap::with_hasher(RandomState::new()),
            index: Vec::new(),
            stored,
            buffer: String::with_capacity(cap.next_power_of_two()),
        }
    }

    pub fn intern(&mut self, text: &str) -> Token {
        if let Some(&id) = self.collected.get(text) {
            return id;
        }

        // SAFETY: `alloc`` is never called elsewhere, nor the properties it controls
        // are modified outside of the method. Here we get a new StringPtr for `text` that
        // hasn't been stored before.
        let name = unsafe { self.alloc(text) };
        let id = Token::new(self.index.len() as u32);
        self.collected.insert(name, id);
        self.index.push(name);

        // SAFETY: We are using the id allocated within the same function scope,
        // so it is always from the same source.
        unsafe {
            debug_assert!(self.lookup(id).equivalent(&name));
        }
        debug_assert!(self.intern(name.cast()) == id);

        id
    }

    /// Allocates a new [`StringPtr`] for the given string input. If there is no more room
    /// in the current buffer, it allocates a new buffer and creates the StringPtr to reference
    /// the stored string in the new buffer, storing the old one.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `self.buffers` and `self.active` are never modified elsewhere,
    /// and that this is called only for new instances of `text`.
    unsafe fn alloc(&mut self, text: &str) -> StringPtr {
        let capacity = self.buffer.capacity();

        if capacity < self.buffer.len() + text.len() {
            // If we ran out of capacity in our storage, allocate a new buffer with
            // larger capacity.
            let new_cap = (capacity.max(text.len()) + 1).next_power_of_two();
            let old_buf = core::mem::replace(&mut self.buffer, String::with_capacity(new_cap));

            self.stored.push(old_buf);
        }

        // Construct raw str slice to eliminate lifetime tracking as we manage its
        // lifetime within the Interner instance.
        let interned = {
            let start = self.buffer.len();
            self.buffer.push_str(text);

            &raw const self.buffer[start..]
        };

        StringPtr(interned)
    }
}

impl Equivalent<StringPtr> for str {
    #[inline(always)]
    fn equivalent(&self, key: &StringPtr) -> bool {
        key.cast().eq(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_ptr_comparisons() {
        let one = "one";
        let two = "two";

        let one_ptr = StringPtr(one);
        let two_ptr = StringPtr(two);

        assert_ne!(one_ptr, two_ptr);

        assert!(one.equivalent(&one_ptr));
    }

    #[test]
    fn is_able_to_intern_one_string() {
        let mut interner = Interner::default();

        assert!(interner.buffer.is_empty());

        let text = "Lorem ipsum";

        let id = interner.intern(text);

        // SAFETY: It comes from the same source
        unsafe {
            assert_eq!(text, interner.lookup(id));
        }
        assert_eq!(interner.buffer.len(), 11);

        let again = interner.intern(text);

        assert_eq!(id, again);
        assert_eq!(interner.buffer.len(), 11);
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

        let interned: Vec<Token> = texts.iter().map(|&text| interner.intern(text)).collect();

        assert_eq!(
            interned.as_slice(),
            &[
                Token::new(0),
                Token::new(1),
                Token::new(2),
                Token::new(3),
                Token::new(4),
                Token::new(2),
                Token::new(5)
            ]
        );
        assert_eq!(interner.buffer.capacity(), 64);
        assert_eq!(interner.stored.len(), 1);
        assert_eq!(interner.stored[0].capacity(), 32);
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

        let interned: Vec<Token> = texts.iter().map(|&text| interner.intern(text)).collect();

        std::thread::scope(|s| {
            s.spawn(move || {
                for (id, expected) in interned.into_iter().zip(texts) {
                    // SAFETY: It comes from the same source
                    unsafe {
                        assert_eq!(expected, interner.lookup(id));
                    }
                }
            });
        });
    }
}
