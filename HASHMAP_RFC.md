Feature Name: Using Rust's std::collections::HashMap to implement Lisp_Hash_Table
Start Date: 7/25/2017
RFC PR:

## Summary

We currently have an oustanding PR to bring over "copy-hash-table" from C to Rust. We are currently in a position to bring over additional hash table functionality to Rust. In doing this, it has been suggested that instead of maintaining our own hash table, we instead leverage Rust's std::collections::HashMap. Attempting to do this brings up two large problems that remacs has yet to discuss, Rust based allocation of LispObjects, and the Rustification (oxidation? heh) of the Lisp GC. 

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

Emacs Lisp has a basic mark and sweep GC, implemented in alloc.c. Like it's name implies, it has two stages, a mark stage where it records if an object is not garbage, and a sweep stage where it frees all non-marked objects.

Adding support for marking should be straight forward. In the `mark_object` function in alloc.c, we have a case for a Lisp_Hash_Table:

``` c
case PVEC_HASH_TABLE:
{
    struct Lisp_Hash_Table *h = (struct Lisp_Hash_Table *) ptr;

    mark_vectorlike (ptr);
    // ...
}
```

This logic can be delegated to a Rust function called from the C, so now the above becomes

``` c
case PVEC_HASH_TABLE:
{
    rust_mark_hash_table(ptr);
}
```

The sweep is a little more complex. Vectors (and vector-likes) are swept in the `sweep_vectors` in alloc.c. Without getting to into the weeds on how this function works, we can summarize this function as

``` c
void sweep_vectors() {
    for (/* each vector in global record */) {
       if (/* vector not marked */) {
           cleanup_vector(vector);
       }
    }
}
```

This is actually a very large simplification, as vectors are using a custom block allocator for smaller vectors, while maintaining a list of "large vectors" that have there own rules. However, at a high level we only need to care about the fact that there is a structure of exsisting vectors that will be swept a certain point in the gc flow. I see no reason why we couldn't add something like


``` c
void sweep_vectors() {
    rust_sweep_vectors(); // Cleans all rust allocated vectors, including LispHashTables etc.
    for (/* each vector in global record */) {
       if (/* vector not marked */) {
           cleanup_vector(vector);
       }
    }    
}
```

We could maintain a structure of Rust allocated vectors, and when we notice are unmarked, we will drop them. There are a number of ways we can accomplish this, each with trade offs, but I would like to discuss a different approach that I believe will set us up better in the future.

Looking at the marking code, it seems that is is emulating somethign that would fit very well in a Rust trait. My idea is that first, we have a trait along the lines of: (Note this won't compile, but is just a rough API sketch. We would need to clarify certain trait bounds in order to make this API work). 

``` rust
pub trait GCThing {
    fn allocate() -> Self;
    
    fn mark(&mut self);
    fn is_marked(&self);
    
    fn is_rooted(&self);
    fn set_rooted(&mut self, rooted: bool);
}
```

We would then create an object along the lines of:

``` rust
struct LispGarbageCollector {
    managed_objects: Vec<Box<GCThing>>,
}
```

With an implementation along the lines of:

``` rust
impl LispGarbageCollector {
    pub fn mark_object(&mut self, obj: LispObject);
    pub fn sweep(&mut self);
    pub fn manage<T: GCThing + Sized>(&mut self, t: T);
}

```

# Rooting
If we look back at a code sample I posted eariler, with this new GC api, it would look something like

``` rust
impl LispHashTableRef {
     pub fn allocate() -> LispHashTableRef {
     	 let map = LispHashTable::new();
	 let ptr = alloc::garbage_collector().manage(map); 
	 LispHashTableRef::new(ptr) // This isn't safe!! :O
     }
}
```

This reason why this isn't safe is because the GC could clean up that pointer, and if we store it around, or use it in between calls of something that could trigger a GC, we could end up with a use after free bug.

``` rust
#[lisp_fn]
fn my_binding(table: LispObject) -> LispObject {
    let object = table.as_hash_table_or_error().get_other_thing();
    other_binding_that_may_trigger_gc(object);
    object.foo(); // This isn't good!
    // ...
}
```

In order to solve this problem, I propose we use an API based on the Spidermonkey rooting API. The idea is we have a struct:

``` rust
struct Rooted<T> where T: GCThing {
  raw: ExternalPtr<T>
}
```

Rooted<T> will use RAII to root the GCThing for the duration of it's exsistence. That way if you have a Rooted<T> to an object, you will have safetey that object will be valid. We would enforce that for Rust allocated objects, we would not use raw external ptrs, but instead use Rooted<T>s.  

## Drawbacks

This is a lot of complexity! However, I believe that the complexity is stemming from problems that remacs will eventually have to solve. The problems of moving the GC over to Rust, and using Rust data structures aren't going to go away, and we will have to face them eventually. It may not yet be time to face these problems.

Rust's HashMap includes support for custom hashing functions, however emacs HashTables having support for controling the `rehash_threshold` and `rehash_size`, which as far as I can tell, are not supported in Rust's HashMap.

## Alternatives

We can port the exsisting hashmap code over 1:1 from C to Rust, not using any Rust specific data structures. 

## Unresolved Questions

The "pure" property for the Lisp_Hash_Table, and if using a Rust data structure will have any negative impact on the unexec codepath.
If we will be introducing a performance regression by using this.