Feature Name: Using Rust's std::collections::HashMap to implement Lisp_Hash_Table
Start Date: 7/25/2017
RFC PR:

## Summary

We currently have an oustanding PR to bring over "copy-hash-table" from C to Rust. We are currently in a position to bring over additional hash table functionality to Rust. In doing this, it has been suggested that instead of maintaining our own hash table, we instead leverage Rust's std::collections::HashMap.

We could simplify our HashTable struct to something like

``` rust
struct HashTableTest {
    name: LispObject,
    user_hash_function: LispObject,
    user_comp_function: LispObject,
}

pub struct LispHashTable {
    header: Lisp_Vectorlike_Header,
    weak: LispObject,
    is_pure: bool,
    table_test: HashTableTest,
    map: HashMap<LispObject, LispObject>,
}
```

## Motivation

We can greatly simplify the implementation of a Lisp Hash Table by leveraging Rust's standard hash map. Functions like 'hash_clear' and 'hash_put' can be greatly simplified, boiled down to 1-2 liners that should be easy for anyone to understand.

## Detailed Design

The key idea here is remove the Lisp_Hash_Table type completely. The type 'Lisp_Hash_Table' will be removed from the C layer entirely, and replaced by a Rust defined type 'LispHashTable'. For tagging and identification, LispHashTable will still be `#[repr(C)]`, and will still begin with a vectorlike header. Besides accessing this vectorlike header, a LispHashTable will be an opaque type to the C layer, and the C layer will not directly manipulate the hash table in anyway.

The current skeleton of hashtable.rs should be able to easily be preserved, updated for this new structure. We currently have code that looks like:
``` rust
pub type LispHashTableRef = ExternalPtr<Lisp_Hash_Table>;

impl LispHashTableRef {    
    pub fn allocate() -> LispHashTableRef {
        let vec_ptr =
            allocate_pseudovector!(Lisp_Hash_Table, count, PseudovecType::PVEC_HASH_TABLE);
        LispHashTableRef::new(vec_ptr)
    }
    // ... setters and getters 

}
```

Now COULD have something that looks like:

``` rust

struct HashTableTest {
    pub name: LispObject,
    pub user_hash_function: LispObject,
    pub user_comp_function: LispObject,
}

#[repr(C)]
pub struct LispHashTable {
    header: Lisp_Vectorlike_Header,
    weak: LispObject,
    is_pure: bool,
    table_test: HashTableTest,
    map: HashMap<LispObject, LispObject>,
}

impl HashTableRef {
     pub fn new() // ...
}

pub type LispHashTableRef = ExternalPtr<LispHashTable>;

impl LispHashTableRef {
     pub fn allocate() -> LispHashTableRef {
     	 let map = LispHashTable::new();
	 let ptr = alloc::manage_vector(map); // New API that will be elaborated on
	 LispHashTableRef::new(ptr)
     }
}

#[lisp_fn]
fn make_hash_table(...) -> LispObject {
   let table = LispHashTableRef::allocate();
   // ...
   
   LispObject::from_hash_table(table)
}

```

The code above looks sane and straight forward, until we get into alloc::manage_vector. At this point in remacs history, all of our lisp objects have been allocated and managed in the C layer. To use a Rust HashMap, we will need to allocate a Rust object that will be managed by the Lisp GC.

# Discussion on Emacs Lisp GC and allocation.







## Drawbacks

## Alternatives

## Unresolved Questions