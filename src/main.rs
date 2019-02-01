extern crate bytes;
extern crate futures;
extern crate futures_cpupool;
extern crate hyper;
extern crate native_tls;
extern crate num_cpus;
extern crate serde_json;
extern crate threadpool;
extern crate tokio;
extern crate tokio_signal;
extern crate tokio_tls;
extern crate uuid;

#[macro_use]
extern crate serde_derive;

#[cfg(target_os = "linux")]
extern crate openssl;

pub(crate) mod duktape;
pub(crate) mod script_registry;
pub(crate) mod settings;

use bytes::*;
use futures::sync::oneshot;
use hyper::http::request::Parts;
use hyper::rt::{Future, Stream};
use hyper::server::conn::Http;
use hyper::service::service_fn;
use hyper::{Body, Method, Request, Response};
use native_tls::TlsAcceptor;
use std::cell::RefCell;
use std::io::Read;
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::thread_local;
use std::time::Duration;
use std::{fs, io, net, path, process};
use threadpool::ThreadPool;
use tokio::net::TcpListener;
use uuid::Uuid;

#[derive(Serialize)]
struct ResponseCreated {
    id: String,
}

/// Represents a request with its header and body information,
/// as well as a oneshot channel to provide a response.
struct RequestWithSender {
    req_parts: Parts,
    req_body: Bytes,
    sender: oneshot::Sender<Response<Body>>,
}

/// Evaluates the provided JavaScript code with the
/// provided arguments, and returns its value after
/// encoding it via JSON. A thread-local Duktape
/// context is used to achieve this.
///
/// `code` is a string that defines a function,
///
/// Example:
///
///   "function(a, b) { return a * b; }"
///
/// `args` is a string with a JSON encoded array
/// of arbitrary arguments.
///
/// Example:
///
///   "[1, 2, \"hello world\"]"
fn json_eval(code: &str, args: &str, limit: Duration) -> io::Result<String> {
    thread_local! {
        static CONTEXT: RefCell<io::Result<duktape::Context>> = {
            RefCell::new(duktape::Context::new())
        };
    }

    CONTEXT.with(|ctx| {
        // If we failed to initialize on this thread, try to once
        // again. Then, continue with execution.

        {
            if ctx.borrow().is_err() {
                *ctx.borrow_mut() = duktape::Context::new()
            }
        }

        match *ctx.borrow_mut() {
            Err(ref e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),

            Ok(ref mut c) => c.evaluate(code, args, limit),
        }
    })
}

/// Handle the request, which means parsing it to determine
/// what to do.
///
/// If it's a request to execute some JavaScript, it's passed
/// off to a thread pool to parallelize execution.
///
/// If it's a request to define a function, we simply store it
/// locally in a synchronous fashion and send the reply.
fn request_handler(
    rx: Receiver<RequestWithSender>,
    js_thread_pool_size: usize,
    registry_script_ttl: Duration,
    script_execution_completion_time: Duration,
) {
    // @FIXME unwraps below should be cleaned up
    let mut registry = script_registry::ScriptRegistry::new(registry_script_ttl);
    let pool = ThreadPool::new(js_thread_pool_size);

    loop {
        let RequestWithSender {
            req_parts,
            req_body,
            sender,
        } = rx.recv().unwrap();

        match (req_parts.method, req_parts.uri.path()) {
            (Method::POST, "/execute") => match String::from_utf8(req_body.into_buf().collect()) {
                Ok(script) => {
                    pool.execute(move || {
                        let result = json_eval(&script, "[]", script_execution_completion_time);

                        let response = match result {
                            Ok(json_body) => Response::builder()
                                .header("Content-Type", "application/json")
                                .body(Body::from(json_body))
                                .unwrap(),

                            Err(e) => Response::builder()
                                .status(403)
                                .body(Body::from(e.to_string()))
                                .unwrap(),
                        };

                        sender.send(response).unwrap();
                    });
                }

                Err(_) => {
                    let response = Response::builder()
                        .status(403)
                        .body(Body::from("cannot extract script from request body"))
                        .unwrap();

                    sender.send(response).unwrap();
                }
            },

            (ref method, path)
                if path.starts_with("/scripts/")
                    && path.len() > 9
                    && (method == Method::POST
                        || method == Method::DELETE
                        || method == Method::GET) =>
            {
                let maybe_script = Uuid::parse_str(&path[9..])
                    .ok()
                    .and_then(|id| registry.get(&id).map(|s| (id, s)));

                match maybe_script {
                    Some((id, script)) => {
                        match method {
                            &Method::POST => {
                                match String::from_utf8(req_body.into_buf().collect()) {
                                    Ok(args) => {
                                        pool.execute(move || {
                                            let result = json_eval(
                                                &script,
                                                &args,
                                                script_execution_completion_time,
                                            );

                                            let response = match result {
                                                Ok(json_body) => Response::builder()
                                                    .header("Content-Type", "application/json")
                                                    .body(Body::from(json_body))
                                                    .unwrap(),

                                                Err(e) => Response::builder()
                                                    .status(403)
                                                    .body(Body::from(e.to_string()))
                                                    .unwrap(),
                                            };

                                            sender.send(response).unwrap();
                                        });
                                    }

                                    Err(_) => {
                                        let response = Response::builder()
                                            .status(403)
                                            .body(Body::from(
                                                "cannot extract arguments from request body",
                                            ))
                                            .unwrap();

                                        sender.send(response).unwrap();
                                    }
                                }
                            }

                            &Method::GET => {
                                let response = Response::builder()
                                    .header("Content-Type", "application/json")
                                    .body(Body::from(script))
                                    .unwrap();

                                sender.send(response).unwrap();
                            }

                            &Method::DELETE => {
                                registry.remove(&id);

                                let response =
                                    Response::builder().status(204).body(Body::empty()).unwrap();

                                sender.send(response).unwrap();
                            }

                            _ => {
                                // shouldn't happen given guard at top level
                            }
                        }
                    }

                    None => {
                        let response = Response::builder()
                            .status(404)
                            .body(Body::from("cannot find script"))
                            .unwrap();

                        sender.send(response).unwrap();
                    }
                }
            }

            (Method::POST, "/scripts") | (Method::POST, "/scripts/") => {
                match String::from_utf8(req_body.into_buf().collect()) {
                    Ok(script) => {
                        let id = registry.store(script);

                        let response_body =
                            serde_json::to_string(&ResponseCreated { id: id.to_string() }).unwrap();

                        let response = Response::builder()
                            .status(201)
                            .header("Content-Type", "application/json")
                            .header("Location", format!("/scripts/{}", id))
                            .body(Body::from(response_body))
                            .unwrap();

                        sender.send(response).unwrap();
                    }

                    Err(_) => {
                        let response = Response::builder()
                            .status(403)
                            .body(Body::from("cannot extract script from request body"))
                            .unwrap();

                        sender.send(response).unwrap();
                    }
                }
            }

            (Method::GET, "/ping") => {
                let response = Response::new(Body::from("pong!"));
                sender.send(response).unwrap();
            }

            _ => {
                let response = Response::new(Body::from("cannot find route"));
                sender.send(response).unwrap();
            }
        }
    }
}

/// Creates a TLS certificate (`Identity`) given PEM formatted public certificate and private key.
///
/// This uses OpenSSL to convert the PEM keys/certs into PK12 format so that they can be used
/// by Tokio TLS.
///
/// Thus, it's currently only supported on Linux. However, support for macOS could be enabled
/// by allowing JSaaS to take a PK12 file directly, instead of creating one at runtime.
#[cfg(target_os = "linux")]
fn create_tls_cert(
    private: path::PathBuf,
    public: path::PathBuf,
) -> io::Result<native_tls::Identity> {
    let name = "jsaas";
    let password = "";

    let private_key_data = {
        let mut file = fs::File::open(private)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        data
    };

    let public_key_data = {
        let mut file = fs::File::open(public)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        data
    };

    let private_key = openssl::pkey::PKey::private_key_from_pem(&private_key_data)?;
    let public_key = openssl::x509::X509::from_pem(&public_key_data)?;
    let pkcs12 = openssl::pkcs12::Pkcs12::builder()
        .build(password, name, &private_key, &public_key)?
        .to_der()?;

    Ok(native_tls::Identity::from_pkcs12(&pkcs12, password)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?)
}

#[cfg(not(target_os = "linux"))]
fn create_tls_cert(
    private: path::PathBuf,
    public: path::PathBuf,
) -> io::Result<native_tls::Identity> {
    Err(io::Error::new(
        io::ErrorKind::Other,
        "TLS is currently only supported on Linux",
    ))
}

/// Main entry point for the program that binds to the TCP socket
/// and handles requests by passing them into request_handler.
///
/// This sets up a channel that handles requests (similar to an actor)
/// and then this thread is taken over by the Tokio event loop via Hyper.
fn main() -> io::Result<()> {
    let settings = settings::Settings::new(
        "JSAAS_BIND_ADDR",
        "JSAAS_SCRIPT_DEFINITION_EXPIRATION_TIME",
        "JSAAS_SCRIPT_EXECUTION_THREAD_POOL_SIZE",
        "JSAAS_SCRIPT_EXECUTION_COMPLETION_TIME",
        "JSAAS_TLS_BIND_ADDR",
        "JSAAS_TLS_PUBLIC_CERTIFICATE_PATH",
        "JSAAS_TLS_PRIVATE_KEY_PATH",
    )?;

    // Note that JSaaS currently only targets POSIX operating systems, so
    // we explicitly handle SIGINT/SIGTERM, instead of tokio-signal's more
    // generic CTRL-C handler.
    //
    // Our current signal handling is very crude -- simply exit. This could
    // be extended to e.g. unbind the port and do some graceful shutdown, but
    // since this is not a user-facing application simply exiting should be
    // sufficient.

    #[allow(dead_code)]
    let signal_handler = tokio_signal::unix::Signal::new(tokio_signal::unix::SIGINT)
        .flatten_stream()
        .select(tokio_signal::unix::Signal::new(tokio_signal::unix::SIGTERM).flatten_stream())
        .for_each(|s| {
            process::exit(128 + s);

            #[allow(unreachable_code)]
            Ok(())
        })
        .map_err(|e| eprintln!("server error: {}", e));

    // Setup a channel that is used to send messages from the
    // Hyper webserver into our request handler thread.
    let (tx, rx) = channel();

    let http_proto = Http::new();

    let setup_http_server = |bind_addr: &net::SocketAddr,
                             tls_identity: Option<native_tls::Identity>|
     -> io::Result<Box<Future<Item = (), Error = _> + Send>> {
        let tx = tx.clone();

        let tls_cx = match tls_identity {
            Some(tls_identity) => {
                let c = TlsAcceptor::builder(tls_identity)
                    .build()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

                Some(tokio_tls::TlsAcceptor::from(c))
            }

            None => None,
        };

        let srv = TcpListener::bind(bind_addr)?;

        let http_handler = move || {
            let tx = tx.clone();
            service_fn(move |req: Request<Body>| {
                let (req_parts, req_raw_body) = req.into_parts();
                let tx = tx.clone();
                Box::new(
                    req_raw_body
                        .concat2()
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                        .and_then(move |chunks| {
                            let req_body = chunks.into_bytes();
                            let tx = tx.clone();
                            let (sender, c) = oneshot::channel::<Response<Body>>();

                            tx.send(RequestWithSender {
                                req_parts,
                                req_body,
                                sender,
                            })
                            .unwrap(); // @FIXME
                            c.map_err(|e| {
                                std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
                            })
                        }),
                )
            })
        };

        // Setup the Hyper webserver (optionally with TLS)
        Ok(match tls_cx {
            Some(tls_cx) => {
                eprintln!("Server will listen on {} (HTTPS)", bind_addr);
                Box::new(
                    http_proto
                        .serve_incoming(
                            srv.incoming().and_then(move |socket| {
                                tls_cx
                                    .accept(socket)
                                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
                            }),
                            http_handler,
                        )
                        .then(|res| match res {
                            Ok(conn) => Ok(Some(conn)),
                            Err(_e) => Ok(None),
                        })
                        .for_each(|conn_opt| {
                            if let Some(conn) = conn_opt {
                                tokio::spawn(
                                    conn.and_then(|c| c.map_err(|e| panic!("Hyper error {}", e)))
                                        .map_err(|_e| ()),
                                );
                            }

                            Ok(())
                        }),
                )
            }

            None => {
                eprintln!("Server will listen on {} (HTTP)", bind_addr);
                Box::new(
                    http_proto
                        .serve_incoming(srv.incoming(), http_handler)
                        .then(|res| match res {
                            Ok(conn) => Ok(Some(conn)),
                            Err(_e) => Ok(None),
                        })
                        .for_each(|conn_opt| {
                            if let Some(conn) = conn_opt {
                                tokio::spawn(
                                    conn.and_then(|c| c.map_err(|e| panic!("Hyper error {}", e)))
                                        .map_err(|_e| ()),
                                );
                            }

                            Ok(())
                        }),
                )
            }
        })
    };

    let bind_addr = settings.bind_addr;
    let tls_bind_addr = settings.tls_bind_addr;
    let tls_private_key_path = settings.tls_private_key_path.clone();
    let tls_public_certificate_path = settings.tls_public_certificate_path.clone();

    // Spawn our request handler
    // @TODO use tokio mpsc to save a thread
    thread::spawn(move || {
        request_handler(
            rx,
            settings.script_execution_thread_pool_size,
            settings.script_definition_expiration_time,
            settings.script_execution_completion_time,
        )
    });

    let tls_cert = match (tls_private_key_path, tls_public_certificate_path) {
        (Some(private), Some(public)) => Some(create_tls_cert(private, public)?),
        _ => None,
    };

    match (tls_bind_addr, tls_cert) {
        (Some(tls_bind_addr), Some(tls_cert)) => {
            let http_server = setup_http_server(&bind_addr, None)?;
            let https_server = setup_http_server(&tls_bind_addr, Some(tls_cert))?;
            Ok(tokio::run(
                signal_handler
                    .join(http_server)
                    .join(https_server)
                    .map(|_| ()),
            ))
        }

        (_, tls_cert) => {
            let http_server = setup_http_server(&bind_addr, tls_cert)?;
            Ok(tokio::run(signal_handler.join(http_server).map(|_| ())))
        }
    }
}
