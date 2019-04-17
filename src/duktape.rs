#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

#[allow(dead_code)]
#[allow(improper_ctypes)]
mod duktape {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/duktape-bindings.rs"));
}

use std::ffi::{c_void, CStr, CString};
use std::{io, ptr, time};

#[no_mangle]
/// Upon setup, this library configures each Duktape heap with a udata pointing to
/// a `Vec<EvaluateContext>`. This vector will only ever have one entry in it, which
/// contains the data for the latest call for this context.
///
/// The Duktape source is configured to call into this function (see build.rs)
/// occasionally to allow our library to determine if execution should be stopped
/// due to timeout.
///
/// In essence, this mechanism guards against infinite loops by bounding their
/// execution time against a "wall clock" (ish -- time cannot go backwards).
extern "C" fn jsaas_duk_exec_timeout_check(udata: *mut c_void) -> duktape::duk_bool_t {
    let ctx = udata as *const Vec<EvaluateContext>;

    let cont = unsafe {
        if (*ctx).len() == 1 {
            (*(ctx))[0].start.elapsed() <= (*(ctx))[0].limit
        } else {
            false
        }
    };

    if cont {
        0
    } else {
        1
    }
}

extern "C" fn jsaas_btoa(ctx: *mut duktape::duk_context) -> duktape::duk_ret_t {
    unsafe { duktape::duk_base64_encode(ctx, -1) };

    1
}

extern "C" fn jsaas_atob(ctx: *mut duktape::duk_context) -> duktape::duk_ret_t {
    let result = unsafe {
        duktape::duk_require_string(ctx, -1);
        duktape::duk_base64_decode(ctx, -1);

        // duk_buffer_to_string and duk_push_lstring seem to be doing some
        // unicode interpretation (or maybe something else) that is causing
        // bytestrings to be truncated, so as a workaround we push the numeric
        // values onto the stack and invoke String.fromCharCode to match
        // typical browser behavior

        let mut buffer_size: duktape::duk_size_t = 0;
        let buffer_ptr =
            duktape::duk_get_buffer_data(ctx, -1, &mut buffer_size as *mut duktape::duk_size_t)
                as *mut i8;

        // these are the only allocations we are doing, so we have
        // to be careful to ensure that they'll be freed
        let name_string = CString::new("String").unwrap_or_default().into_raw();
        let name_from_char_code = CString::new("fromCharCode").unwrap_or_default().into_raw();

        // these cannot fail (barring Duktape bugs) given that the
        // arguments and stack size are always the same.
        duktape::duk_get_global_string(ctx, name_string);
        duktape::duk_get_prop_string(ctx, -1, name_from_char_code);

        // thus, this will always be executed, meaning this code is
        // leak free
        drop(CString::from_raw(name_string));
        drop(CString::from_raw(name_from_char_code));

        // anything past this could fail, but we've already freed
        // the memory allocated for the `CString`s, so we should be
        // leak free
        duktape::duk_dup(ctx, -2);

        for offset in 0..buffer_size {
            let byte = *(buffer_ptr.offset(offset as isize) as *mut u8) as u32;
            duktape::duk_push_uint(ctx, byte);
        }

        duktape::duk_pcall_method(ctx, buffer_size as i32)
    };

    match result {
        0 => 1,
        _ => 0,
    }
}

const GLOBAL_FN_BTOA: *const u8 = b"btoa\0" as *const u8;
const GLOBAL_FN_ATOB: *const u8 = b"atob\0" as *const u8;

struct EvaluateContext {
    limit: time::Duration,
    start: time::Instant,
}

pub(crate) struct Context {
    ctx: *mut duktape::duk_hthread,
    latest_evaluate_context: *mut Vec<EvaluateContext>,
}

impl Context {
    /// Creates a new `Context` that can be used to evaluate JavaScript functions.
    pub(crate) fn new() -> io::Result<Context> {
        let latest_evaluate_context = Box::into_raw(Box::new(vec![]));

        let ctx = unsafe {
            duktape::duk_create_heap(
                None,
                None,
                None,
                latest_evaluate_context as *mut c_void,
                None,
            )
        };

        unsafe {
            duktape::duk_push_global_object(ctx);
            duktape::duk_push_c_function(ctx, Some(jsaas_btoa), 1);
            duktape::duk_put_prop_string(ctx, -2, GLOBAL_FN_BTOA as *const std::os::raw::c_char);
            duktape::duk_push_c_function(ctx, Some(jsaas_atob), 1);
            duktape::duk_put_prop_string(ctx, -2, GLOBAL_FN_ATOB as *const std::os::raw::c_char);
            duktape::duk_pop(ctx);
        }

        if ctx.is_null() {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "error initializing Duktape heap",
            ))
        } else {
            Ok(Context {
                ctx,
                latest_evaluate_context,
            })
        }
    }

    /// Evaluates a JavaScript function given its definition and a JSON-encoded array of
    /// arguments for the function.
    pub(crate) fn evaluate<S: AsRef<str>>(
        &mut self,
        code: S,
        args: S,
        limit: time::Duration,
    ) -> io::Result<String> {
        // @FIXME note the comment below:
        // my assumption is that it's still secure to reuse contexts given an empty stack
        // secure as in an execution can't reference any data in the heap from a
        // previous execution, given there are no globals and no stack frames
        // if this is not the case, we'd want to initialize a new heap on each call, but
        // this has overheads.

        if !args.as_ref().trim_start().starts_with('[') {
            // a simple validation hack, given that we require args to be an array, not
            // simply any parseable JSON value
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "args must be a JSON-encoded array",
            ));
        }

        let code = CString::new(code.as_ref())?;
        let code_ptr = code.as_ptr();

        let args = CString::new(args.as_ref())?;
        let args_ptr = args.as_ptr();

        let function = CString::new("function")?;
        let function_ptr = function.as_ptr();

        let loader =
            CString::new("function(fn, args) { return fn.apply(null, JSON.parse(args)); }")?;
        let loader_ptr = loader.as_ptr();

        // Clear the existing stack and setup a new evaluation context (timestamp)
        self.duk_clear_stack();

        unsafe {
            (*self.latest_evaluate_context).clear();
            (*self.latest_evaluate_context).push(EvaluateContext {
                start: time::Instant::now(),
                limit,
            });
        }

        // Load our functions onto the stack
        {
            unsafe {
                duktape::duk_push_string(self.ctx, loader_ptr);
                duktape::duk_push_string(self.ctx, function_ptr);
            }

            self.duk_compile().map_err(|e| {
                self.duk_clear_stack();
                e
            })?;
        }

        {
            unsafe {
                duktape::duk_push_string(self.ctx, code_ptr);
                duktape::duk_push_string(self.ctx, function_ptr);
            }

            self.duk_compile().map_err(|e| {
                self.duk_clear_stack();
                e
            })?;
        }

        // Execute
        let result = unsafe {
            duktape::duk_push_string(self.ctx, args_ptr);

            duktape::duk_pcall(self.ctx, 2) // 2 arguments
        };

        if result == 0 {
            // we've successfully executed, thus the stack is non-empty. Attempt to
            // encode it as JSON, and if successful, copy it to an owned String

            let json_ptr = unsafe { duktape::duk_json_encode(self.ctx, 0) };

            if json_ptr.is_null() {
                self.duk_clear_stack();

                Err(io::Error::new(io::ErrorKind::Other, "undefined"))
            } else {
                let json_cstr = unsafe { CStr::from_ptr(json_ptr) };

                let json_string: String = json_cstr
                    .to_str()
                    .map_err(|_e| io::Error::new(io::ErrorKind::Other, "undefined"))?
                    .to_string();

                self.duk_clear_stack();

                Ok(json_string)
            }
        } else {
            let error_result = self
                .duk_error_message()
                .and_then(|e| Err(io::Error::new(io::ErrorKind::Other, e)));

            self.duk_clear_stack();

            error_result
        }
    }

    fn duk_clear_stack(&mut self) {
        unsafe {
            duktape::duk_pop_n(self.ctx, duktape::duk_get_top(self.ctx));
        }
    }

    fn duk_compile(&mut self) -> io::Result<()> {
        let result = unsafe {
            duktape::duk_compile_raw(
                self.ctx,
                ptr::null_mut(),
                0,
                2 | duktape::DUK_COMPILE_FUNCTION | duktape::DUK_COMPILE_SAFE,
            )
        };

        if result != 0 {
            self.duk_error_message()
                .and_then(|e| Err(io::Error::new(io::ErrorKind::Other, e)))
        } else {
            Ok(())
        }
    }

    fn duk_error_message(&mut self) -> io::Result<String> {
        let error_cstr =
            unsafe { CStr::from_ptr(duktape::duk_safe_to_lstring(self.ctx, -1, ptr::null_mut())) };

        error_cstr
            .to_str()
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("error decoding Duktape error message: {}", e),
                )
            })
            .map(|s| s.to_string())
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        self.duk_clear_stack();

        unsafe {
            duktape::duk_destroy_heap(self.ctx);
            Box::from_raw(self.latest_evaluate_context);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_duktake_add_result_number() {
        let mut ctx = Context::new().unwrap();
        let r = ctx
            .evaluate(
                "function(a, b) { return a + b; }",
                "[2, 4]",
                time::Duration::from_millis(5000),
            )
            .unwrap();
        assert_eq!(r, "6");
    }

    #[test]
    fn test_duktake_add_mult_result_obj() {
        let mut ctx = Context::new().unwrap();
        let r = ctx
            .evaluate(
                "function(a, b) { return { sum: a + b, product: a * b };}",
                "[2, 4]",
                time::Duration::from_millis(5000),
            )
            .unwrap();
        assert_eq!(r, "{\"sum\":6,\"product\":8}");
    }

    #[test]
    fn test_duktape_bad_code() {
        let mut ctx = Context::new().unwrap();
        match ctx.evaluate(
            "function()) { return 0; }}",
            "[]",
            time::Duration::from_millis(5000),
        ) {
            Ok(_) => panic!("should have failed"),

            Err(e) => assert_eq!(e.to_string(), "SyntaxError: parse error (line 1)"),
        };
    }

    #[test]
    fn test_duktape_bad_args() {
        let mut ctx = Context::new().unwrap();
        let r = ctx.evaluate(
            "function(a, b) { return { sum: a + b, product: a * b };}",
            "[{{",
            time::Duration::from_millis(5000),
        );
        match r {
            Ok(_) => panic!("should have failed"),

            Err(e) => assert_eq!(e.to_string(), "SyntaxError: invalid json (at offset 3)"),
        };
    }

    #[test]
    fn test_duktape_wrong_args() {
        let mut ctx = Context::new().unwrap();
        let r = ctx.evaluate(
            "function(a, b) { return { sum: a + b, product: a * b };}",
            " {}",
            time::Duration::from_millis(5000),
        );
        assert!(r.is_err());
    }

    #[test]
    fn test_duktape_no_return() {
        let mut ctx = Context::new().unwrap();
        let r = ctx
            .evaluate("function() {}", "[]", time::Duration::from_millis(5000))
            .err()
            .unwrap();
        assert_eq!(r.description(), "undefined");
    }

    #[test]
    fn test_duktape_return_undefined() {
        let mut ctx = Context::new().unwrap();
        let r = ctx
            .evaluate(
                "function() { return undefined; }",
                "[]",
                time::Duration::from_millis(5000),
            )
            .err()
            .unwrap();
        assert_eq!(r.description(), "undefined");
    }

    #[test]
    fn test_duktape_return_null() {
        let mut ctx = Context::new().unwrap();
        let r = ctx
            .evaluate(
                "function() { return null; }",
                "[]",
                time::Duration::from_millis(5000),
            )
            .unwrap();
        assert_eq!(r, "null");
    }

    #[test]
    fn test_duktape_return_func() {
        let mut ctx = Context::new().unwrap();
        let r = ctx
            .evaluate(
                "function() { return function() {}; }",
                "[]",
                time::Duration::from_millis(5000),
            )
            .err()
            .unwrap();
        assert_eq!(r.description(), "undefined");
    }

    #[test]
    fn test_duktape_usable_after_error() {
        let mut ctx = Context::new().unwrap();
        let r = ctx.evaluate(
            "funktion()) { return 0; }}",
            "[]",
            time::Duration::from_millis(5000),
        );
        assert!(r.is_err());
        let r = ctx
            .evaluate(
                "function(a, b) { return a + b; }",
                "[2, 4]",
                time::Duration::from_millis(5000),
            )
            .unwrap();
        assert_eq!(r, "6");
    }

    #[test]
    fn test_duktake_while_true_recoverable() {
        let mut ctx = Context::new().unwrap();
        let r = ctx.evaluate(
            "function() { while(true) {}",
            "[]",
            time::Duration::from_millis(100),
        );

        assert!(r.is_err());

        let r = ctx
            .evaluate(
                "function(a, b) { return a + b; }",
                "[2, 4]",
                time::Duration::from_millis(5000),
            )
            .unwrap();
        assert_eq!(r, "6");
    }

    #[test]
    fn test_duktape_btoa() {
        let mut ctx = Context::new().unwrap();

        let r = ctx
            .evaluate(
                "
                function() {
                    return {
                        string: btoa('hello'),
                        number: btoa(1234),
                        undefined: btoa(),
                        undefinedExplicit: btoa(undefined),
                        true: btoa(true),
                        false: btoa(false),
                        null: btoa(null),
                        obj: btoa({ test: 42 })
                    };
                }
            ",
                "[]",
                time::Duration::from_millis(5000),
            )
            .unwrap();

        assert_eq!(r, r#"{"string":"aGVsbG8=","number":"MTIzNA==","undefined":"dW5kZWZpbmVk","undefinedExplicit":"dW5kZWZpbmVk","true":"dHJ1ZQ==","false":"ZmFsc2U=","null":"bnVsbA==","obj":"W29iamVjdCBPYmplY3Rd"}"#);
    }

    #[test]
    fn test_duktape_atob() {
        let mut ctx = Context::new().unwrap();

        let r = ctx
            .evaluate(
                r#"
                function() {
                    return {
                      one: atob("aGVsbG8="),
                      two: [0, 1, 2, 3, 4, 5, 6, 7].map(function(i) {
                        var value = atob("AacABdxfoCQ=");

                        return value.charCodeAt(i);
                      }),
                      three: atob("")
                    };
                }
                "#,
                "[]",
                time::Duration::from_millis(5000),
            )
            .unwrap();

        assert_eq!(
            r,
            r#"{"one":"hello","two":[1,167,0,5,220,95,160,36],"three":""}"#
        );

        let r = ctx.evaluate(
            "function() { return atob(1234); }",
            "[]",
            time::Duration::from_millis(5000),
        );

        assert!(r.is_err());

        let r = ctx.evaluate(
            r#"function() { return atob("Z"); }"#,
            "[]",
            time::Duration::from_millis(5000),
        );

        assert!(r.is_err());
    }
}
