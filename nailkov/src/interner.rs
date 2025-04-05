use hashbrown::{Equivalent, HashMap};
use wyrand::RandomWyHashState;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct InternedString(u32);

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
struct StringPtr(*const str);

unsafe impl Send for StringPtr {}
unsafe impl Sync for StringPtr {}

impl StringPtr {
    fn as_str<'a>(&'a self) -> &'a str {
        unsafe { &*self.0 }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Interner {
    collected: HashMap<StringPtr, InternedString, RandomWyHashState>,
    vec: Vec<StringPtr>,
    buf: String,
    full: Vec<String>,
}

impl Default for Interner {
    fn default() -> Self {
        Self::with_capacity(128)
    }
}

impl Interner {
    pub fn lookup(&self, id: InternedString) -> &str {
        self.vec[id.0 as usize].as_str()
    }

    pub fn with_capacity(cap: usize) -> Interner {
        let cap = cap.next_power_of_two();
        Interner {
            collected: HashMap::with_hasher(RandomWyHashState::new()),
            vec: Vec::new(),
            buf: String::with_capacity(cap),
            full: Vec::new(),
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

        debug_assert!(self.lookup(id).equivalent(&name));
        debug_assert!(self.intern(name.as_str()) == id);

        id
    }

    // pub fn from_tokens<'token>(tokens: impl Iterator<Item = &'token str>) -> Self {
    //     let mut interner = Self::default();

    //     for token in tokens {
    //         interner.intern(token);
    //     }

    //     interner
    // }

    unsafe fn alloc(&mut self, name: &str) -> StringPtr {
        let cap = self.buf.capacity();

        if cap < self.buf.len() + name.len() {
            let new_cap = (cap.max(name.len()) + 1)
                .next_power_of_two();
            let new_buf = String::with_capacity(new_cap);
            let old_buf = core::mem::replace(&mut self.buf, new_buf);
            self.full.push(old_buf);
        }

        let interned = {
            let start = self.buf.len();
            self.buf.push_str(name);
            &self.buf[start..]
        };

        StringPtr(interned as *const str)
    }
}

impl Equivalent<StringPtr> for str {
    fn equivalent(&self, key: &StringPtr) -> bool {
        unsafe { &*key.0 }.eq(self)
    }
}
