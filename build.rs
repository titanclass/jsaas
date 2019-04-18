extern crate bindgen;
extern crate cc;

use std::collections::HashSet;
use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const DUKTAPE_VERSION: &'static str = "2.3.0";

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

/// This build script downloads Duktape (if necessary), extracts it
/// to the relevant target directory, and uses its make script to
/// create the .c/.h files.
///
/// The `cc` crate then compiles the source code, and the `bindgen`
/// crate uses it to generate bindings for use by the Rust source code.
fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let duktape_bindings = PathBuf::from(env::var("OUT_DIR").unwrap()).join("duktape-bindings.rs");
    let src = out_dir.join("src");

    // Download/prepare the duktape code if we haven't already
    if !src.join("duktape-src").join("duktape.c").exists() {
        let result = Command::new(
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
                .join("scripts")
                .join("build-setup-duktape"),
        )
        .arg(DUKTAPE_VERSION)
        .arg(&src)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .unwrap();

        assert!(result.success());
    }

    // Note that this is always necessary -- the C compiler
    // must be invoked on every step. Luckily it is very fast.
    cc::Build::new()
        .file(format!(
            "{}/duktape-src/duktape.c",
            &src.as_path().to_str().unwrap()
        ))
        .include(format!("{}/duktape-src", &src.as_path().to_str().unwrap()))
        .compile("libduktape.a");

    // This is expensive, as it needs to gen/compile a giant Rust file
    // that is generated from the Duktape source code, so only invoke
    // if it isn't there. If there are any changes to Duktape source,
    // this implies a cargo clean must be run. Perhaps name it according
    // to sha or something.
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let src = out_dir.join("src");

    if !duktape_bindings.exists() {
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

        let target = env::var("TARGET").expect("missing TARGET");

        let bindings = bindgen::Builder::default()
            .header(format!(
                "{}/duktape-src/duktape.h",
                src.into_os_string().to_str().unwrap()
            ))
            .parse_callbacks(Box::new(ignored_macros))
            .rustfmt_bindings(true)
            .clang_args(&["-target", &target])
            .blacklist_type("max_align_t")
            .generate()
            .expect("Unable to generate bindings");
        bindings
            .write_to_file(duktape_bindings)
            .expect("Couldn't write bindings!");
    }
}
