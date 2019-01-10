use std::env;
use std::panic;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

extern crate reqwest;
extern crate serde_json;

#[macro_use]
extern crate serde_derive;

#[derive(Debug, Deserialize)]
struct ScriptCreated {
    id: String,
}

#[test]
#[ignore]
/// Runs a small test on the JSaaS server by starting it,
/// defining a script, and executing it.
fn test_jsaas() {
    let dir = cargo_dir();
    let jsaas = dir.join("jsaas");

    with_jsaas(&jsaas, move || {
        eventually(Duration::from_secs(300), Duration::from_secs(5), || {
            eprintln!("attempting ping...");

            reqwest::get("http://localhost:9412/ping")
                .map(|r| r.status().is_success())
                .unwrap_or_else(|_| false)
        });

        let client = reqwest::Client::new();

        let created_data = client
            .post("http://localhost:9412/scripts")
            .body("function(a, b) { return a * b; }")
            .send()
            .unwrap()
            .text()
            .unwrap();

        let created: ScriptCreated = serde_json::from_str(&created_data).unwrap();

        let value = client
            .post(&format!("http://localhost:9412/scripts/{}", created.id))
            .body("[4, 3]")
            .send()
            .unwrap()
            .text()
            .unwrap()
            .parse::<i64>()
            .unwrap();

        assert_eq!(value, 12);
    });
}

/// Repeatedly runs the provided function upto a timelimit of `limit`,
/// waiting `retry` interval between retries.
fn eventually<F: Fn() -> bool>(limit: Duration, retry: Duration, f: F) {
    let start = Instant::now();

    while !f() {
        thread::sleep(retry);

        assert!(start.elapsed() < limit);
    }
}

/// Runs the provided function after starting JSaaS, and stops the server
/// once the function returns.
///
/// If the function panics in an unwind safe manner, cleanup is still performed.
fn with_jsaas<F: FnOnce() -> () + panic::UnwindSafe>(jsaas: &PathBuf, f: F) {
    let mut jsaas_child = Command::new(jsaas)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();

    // Avoid some cosmetic errors in the logs by giving some
    // time to start up.
    thread::sleep(Duration::from_secs(2));

    let result = panic::catch_unwind(|| f());

    let stopped = jsaas_child.kill().is_ok();

    assert!(result.is_ok());

    if !stopped {
        panic!("failed to stop JSaaS");
    }
}

/// Returns the directory where the binaries are saved. This
/// was taken from the Cargo project, see
/// https://github.com/rust-lang/cargo/blob/7fa132c7272fb9faca365c1d350e8e3c4c0d45e9/tests/cargotest/support/mod.rs#L316-L333
fn cargo_dir() -> PathBuf {
    env::var_os("CARGO_BIN_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            env::current_exe().ok().map(|mut path| {
                path.pop();
                if path.ends_with("deps") {
                    path.pop();
                }
                path
            })
        })
        .unwrap_or_else(|| panic!("CARGO_BIN_PATH wasn't set. Cannot continue running test"))
}
