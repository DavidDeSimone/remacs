use libc::size_t;
use std::mem;
use super::*;

macro_rules! sizeof_fn {
    ($fn_name: ident, $type: ty) => {
        #[no_mangle]
        pub extern "C" fn $fn_name() -> size_t {
            mem::size_of::<$type>()
        }
    }
}

macro_rules! offsetof_fn {
    ($fn_name: ident, $type: ty, $field: ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $fn_name() -> size_t {
            &(*(0 as *const $type)).$field as *const _ as size_t
        }
    }
}

sizeof_fn!(sizeof_lisp_subr, Lisp_Subr);
sizeof_fn!(sizeof_lisp_string, Lisp_String);
sizeof_fn!(sizeof_lisp_symbol, Lisp_Symbol);
sizeof_fn!(sizeof_lisp_misc_any, Lisp_Misc_Any);
sizeof_fn!(sizeof_lisp_marker, Lisp_Marker);
sizeof_fn!(sizeof_lisp_overlay, Lisp_Overlay);
sizeof_fn!(sizeof_lisp_hash_table, Lisp_Hash_Table);

offsetof_fn!(offsetof_symbol_name, Lisp_Symbol, name);
offsetof_fn!(offsetof_symbol_function, Lisp_Symbol, function);
offsetof_fn!(offsetof_marker_buffer, Lisp_Marker, buffer);
offsetof_fn!(offsetof_overlay_next, Lisp_Overlay, next);
