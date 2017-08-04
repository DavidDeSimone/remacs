use remacs_macros::lisp_fn;
use lisp::{LispObject, ExternalPtr};
use vectors::LispVectorlikeHeader;
use remacs_sys::{Lisp_Hash_Table, PseudovecType, Fcopy_sequence, Lisp_Type,
                 QCtest, Qeq, Qeql, Qequal, QCpurecopy,
                 QCsize, QCweakness, sxhash, EmacsInt};
use std::ptr;
use fnv::FnvHashMap;
#[cfg(test)]
use bincode::{serialize, deserialize, Infinite};
use std::hash::{Hash, Hasher};

pub type LispHashTableRef = ExternalPtr<Lisp_Hash_Table>;

#[derive(Eq, PartialEq, Serialize, Deserialize, Copy, Clone)]
enum HashFunction {
    Eq,
    Eql,
    Equal,
    UserFunc(LispObject, LispObject, LispObject)
}

// @TODO manually derive Eq and PartialEq
#[derive(Eq, PartialEq, Serialize, Deserialize, Clone)]
struct HashableLispObject {
    object: LispObject,
    func: HashFunction
}

impl HashableLispObject {
    fn with_hashfunc_and_object(o: LispObject, f: HashFunction) -> HashableLispObject {
        HashableLispObject {
            object: o,
            func: f
        }
    }
}

impl Hash for HashableLispObject {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.func {
            HashFunction::Eq => {
                self.object.hash(state);
            },
            HashFunction::Eql => {
                if self.object.is_float() {
                    let hash = unsafe { sxhash(ptr::null_mut(), self.object.to_raw()) } as u64;
                    state.write_u64(hash);
                } else {
                    self.object.hash(state);
                }
            },
            HashFunction::Equal => {
                let hash = unsafe { sxhash(ptr::null_mut(), self.object.to_raw()) } as u64;
                state.write_u64(hash);
            },
            HashFunction::UserFunc(_, _, _) => { /* @TODO */ },
        }

        state.finish();
    }
}

// @TODO add pure copy functionality. We will use a binary serializer, dump the memory into
// pure alloc space, while calling purecopy on all underlying objects.
// This should allow us to easily serialize/deserialize the hash table even though it has Rust objects.
#[allow(dead_code)] // @TODO remove
#[derive(Serialize, Deserialize, Clone)]
#[repr(C)]
pub struct LispHashTable {
    header: LispVectorlikeHeader,
    weak: bool,
    is_pure: bool,
    func: HashFunction,
    map: FnvHashMap<HashableLispObject, HashableLispObject>,
}

impl LispHashTable {
    pub fn new() -> LispHashTable {
        Self::with_capacity(65)
    }

    pub fn with_capacity(cap: usize) -> LispHashTable {
        LispHashTable {
            header: LispVectorlikeHeader::new(),
            weak: false,
            is_pure: false,
            func: HashFunction::Eq,
            map: FnvHashMap::with_capacity_and_hasher(cap, Default::default()),
        }
    }
}

impl LispHashTableRef {
    pub fn allocate() -> LispHashTableRef {
        let vec_ptr =
            allocate_pseudovector!(Lisp_Hash_Table, count, PseudovecType::PVEC_HASH_TABLE);
        LispHashTableRef::new(vec_ptr)
    }

    pub unsafe fn copy(&mut self, other: LispHashTableRef) {
        ptr::copy_nonoverlapping(other.as_ptr(), self.as_mut(), 1);
    }

    pub fn set_next_weak(&mut self, other: LispHashTableRef) {
        self.next_weak = other.as_ptr() as *mut Lisp_Hash_Table;
    }

    pub fn get_next_weak(&self) -> LispHashTableRef {
        LispHashTableRef::new(self.next_weak)
    }

    pub fn set_hash(&mut self, hash: LispObject) {
        self.hash = hash.to_raw();
    }

    pub fn get_hash(&self) -> LispObject {
        LispObject::from_raw(self.hash)
    }

    pub fn set_next(&mut self, next: LispObject) {
        self.next = next.to_raw();
    }

    pub fn get_next(&self) -> LispObject {
        LispObject::from_raw(self.next)
    }

    pub fn set_index(&mut self, index: LispObject) {
        self.index = index.to_raw();
    }

    pub fn get_index(&self) -> LispObject {
        LispObject::from_raw(self.index)
    }

    pub fn get_key_and_value(&self) -> LispObject {
        LispObject::from_raw(self.key_and_value)
    }

    pub fn set_key_and_value(&mut self, key_and_value: LispObject) {
        self.key_and_value = key_and_value.to_raw();
    }

    pub fn get_weak(&self) -> LispObject {
        LispObject::from_raw(self.weak)
    }
}

/// Return a copy of hash table TABLE.
/// Keys and values are not copied, only the table itself is.
#[lisp_fn]
fn copy_hash_table(htable: LispObject) -> LispObject {
    let mut table = htable.as_hash_table_or_error();
    let mut new_table = LispHashTableRef::allocate();
    unsafe { new_table.copy(table) };
    assert!(new_table.as_ptr() != table.as_ptr());

    let key_and_value = LispObject::from_raw(unsafe {
        Fcopy_sequence(new_table.get_key_and_value().to_raw())
    });
    let hash = LispObject::from_raw(unsafe { Fcopy_sequence(new_table.get_hash().to_raw()) });
    let next = LispObject::from_raw(unsafe { Fcopy_sequence(new_table.get_next().to_raw()) });
    let index = LispObject::from_raw(unsafe { Fcopy_sequence(new_table.get_index().to_raw()) });
    new_table.set_key_and_value(key_and_value);
    new_table.set_hash(hash);
    new_table.set_next(next);
    new_table.set_index(index);

    if new_table.get_weak().is_not_nil() {
        new_table.set_next_weak(table.get_next_weak());
        table.set_next_weak(new_table);
    }

    LispObject::from_hash_table(new_table)
}

#[lisp_fn]
fn make_hash_map(args: &mut [LispObject]) -> LispObject {
    // @TODO this needs to be managed by the GC, we are just leaking this for testing right now.
    let mut ptr = ExternalPtr::new(Box::into_raw(Box::new(LispHashTable::new())));
    let len = args.len();
    let mut i = 0;
    while i < len {
        if i + 1 > len {
            panic!("Inproper args list"); // @TODO make this a signal_error
        }

        let key = args[i];
        let value = args[i + 1];
        i += 2;
        if key.to_raw() == unsafe { QCtest } {
            if value.to_raw() == unsafe { Qeq } {
                ptr.func = HashFunction::Eq;
            } else if value.to_raw() == unsafe { Qeql } {
                ptr.func = HashFunction::Eql;
            } else if value.to_raw() == unsafe { Qequal } {
                ptr.func = HashFunction::Equal;
            } else {
                // Custom hash table test
            }
        } else if key.to_raw() == unsafe { QCpurecopy } {
            ptr.is_pure = true;
        } else if key.to_raw() == unsafe { QCsize } {
            let size = value.as_natnum_or_error() as usize;
            ptr.map.reserve(size);
        } else if key.to_raw() == unsafe { QCweakness } {
            ptr.weak = true;
        }
    }

    // @TODO handle if there are unused args
    // @TODO Examine this tagging API. This is 'if false'd because if we tag as it as hashmap, it
    // will be treated like a Lisp_Hash_Table in other places in the code, which will cause
    // memory errors
    if false {
        ptr.header.tag(pseudovector_tag_for!(Lisp_Hash_Table, count, PseudovecType::PVEC_HASH_TABLE));
    }
    LispObject::tag_ptr(ptr, Lisp_Type::Lisp_Vectorlike)
}

#[lisp_fn]
fn map_put(map: LispObject, k: LispObject, v: LispObject) -> LispObject {
    let mut hashmap = ExternalPtr::new(map.get_untaggedptr() as *mut LispHashTable); // @TODO replace with with haashtable or erorr
    let key = HashableLispObject::with_hashfunc_and_object(k, hashmap.func);
    let value = HashableLispObject::with_hashfunc_and_object(v, hashmap.func);
    hashmap.map.insert(key, value);
    v 
}

#[lisp_fn]
fn map_get(map: LispObject, k: LispObject) -> LispObject {
    let hashmap = ExternalPtr::new(map.get_untaggedptr() as *mut LispHashTable);
    let key = HashableLispObject::with_hashfunc_and_object(k, hashmap.func);
    hashmap.map.get(&key).map_or(LispObject::constant_nil(), |key| key.object)
}

#[lisp_fn]
fn map_rm(map: LispObject, k: LispObject) -> LispObject {
    let mut hashmap = ExternalPtr::new(map.get_untaggedptr() as *mut LispHashTable);
    let key = HashableLispObject::with_hashfunc_and_object(k, hashmap.func);
    hashmap.map.remove(&key);
    map
}

#[lisp_fn]
fn map_clear(map: LispObject) -> LispObject {
    let mut hashmap = ExternalPtr::new(map.get_untaggedptr() as *mut LispHashTable);
    hashmap.map.clear();
    map
}

#[lisp_fn]
fn map_count(map: LispObject) -> LispObject {
    let hashmap = ExternalPtr::new(map.get_untaggedptr() as *mut LispHashTable);
    LispObject::from_natnum(hashmap.map.len() as EmacsInt)
}

// @TODO have this use things managed by the GC.
#[lisp_fn]
fn map_copy(map: LispObject) -> LispObject {
    let hashmap = ExternalPtr::new(map.get_untaggedptr() as *mut LispHashTable);
    // @TODO if table is weak, add it to weak table data structure.
    let new_map = ExternalPtr::new(Box::into_raw(Box::new(hashmap.clone())));
    LispObject::tag_ptr(new_map, Lisp_Type::Lisp_Vectorlike)
}

#[test]
fn bin_dump() {
    let mut table = LispHashTable::new();
    let encoded: Vec<u8> = serialize(&table, Infinite).unwrap();
    let decoded: LispHashTable = deserialize(&encoded[..]).unwrap();
}
