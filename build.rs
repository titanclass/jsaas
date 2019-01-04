extern crate bindgen;
extern crate cc;

use std::collections::HashSet;
use std::env;
use std::path::PathBuf;

// @FIXME thanks to @stillinbeta for this fix
// see https://github.com/rust-lang/rust-bindgen/issues/687#issuecomment-450750547

#[derive(Debug)]
struct IgnoreMacros(HashSet<String>);

impl bindgen::callbacks::ParseCallbacks for IgnoreMacros {
    fn will_parse_macro(&self, name: &str) -> bindgen::callbacks::MacroParsingBehavior {
        if self.0.contains(name) {
            bindgen::callbacks::MacroParsingBehavior::Ignore
        } else {
            bindgen::callbacks::MacroParsingBehavior::Default
        }
    }
}

fn main() {
    // @TODO FIXME make a build step to download the code

    // Note that this is always necessary -- the C compiler
    // must be invoked on every step. Luckily it is very fast.

    cc::Build::new()
        .file("/home/longshorej/downloads/duktape-2.3.0/duktape-src/duktape.c")
        .include("/home/longshorej/downloads/duktape-2.3.0/duktape-src")
        .compile("libduktape.a");

    // This is expensive, as it needs to compile a giant Rust file
    // that is generated from the Duktape source code, so only invoke
    // if it isn't there. If there are any changes to Duktape source,
    // this implies a cargo clean must be run. Perhaps name it according
    // to sha or something.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs");

    if !out_path.exists() {
        let ignored_macros = IgnoreMacros(
            vec![
                "FP_INFINITE".into(),
                "FP_NAN".into(),
                "FP_NORMAL".into(),
                "FP_SUBNORMAL".into(),
                "FP_ZERO".into(),
                "IPPORT_RESERVED".into(),
            ]
            .into_iter()
            .collect(),
        );

        let bindings = bindgen::Builder::default()
            .header("/home/longshorej/downloads/duktape-2.3.0/duktape-src/duktape.h")
            .parse_callbacks(Box::new(ignored_macros))
            .rustfmt_bindings(true)
            .blacklist_type("max_align_t")
            .generate()
            .expect("Unable to generate bindings");
        bindings
            .write_to_file(out_path)
            .expect("Couldn't write bindings!");
    }
}
