#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

#[allow(dead_code)]
mod duktape {
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

        if !args.as_ref().trim_start().starts_with("[") {
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
            // we've successfully executed, so now we encode the result as JSON and obtain a
            // pointer to it. then, we copy it to a String that is owned by Rust.
            let json_cstr = unsafe { CStr::from_ptr(duktape::duk_json_encode(self.ctx, 0)) };

            let json_string: String = json_cstr
                .to_str()
                .map_err(|e| {
                    self.duk_clear_stack();
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("error encoding Duktape result as JSON: {}", e),
                    )
                })?
                .to_string();

            self.duk_clear_stack();

            Ok(json_string)
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
