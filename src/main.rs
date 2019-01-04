#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use std::ffi::CString;
use std::ptr;

/// Temporary function as POC, runs provided code
/// and prints the integer it returns.
fn execute_and_print_int(code: &str) {
    unsafe {
        let context = duk_create_heap(None, None, None, ptr::null_mut(), None);

        let code = CString::new(code).unwrap(); // @FIXME unwrap
        let code_ptr = code.as_ptr();

        duk_eval_raw(
            context,
            code_ptr,
            0,
            0 | DUK_COMPILE_EVAL
                | DUK_COMPILE_NOSOURCE
                | DUK_COMPILE_SAFE
                | DUK_COMPILE_STRLEN
                | DUK_COMPILE_NOFILENAME,
        );
        let value = duk_get_int(context, -1);
        duk_destroy_heap(context);

        println!("finished, value: {}", value);
    }
}

fn main() {
    let code = "
        const a = 24;

        const double = function(n) {
            return n * 2;
        };

        double(a)
    ";

    execute_and_print_int(code);
}
