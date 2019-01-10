# JSaaS

*Currently at the POC stage, i.e. only proving that invoking Duktape from Rust is easy enough.*

## Overview

An HTTP service that uses the [Duktape](https://duktape.org/) JavaScript engine
to safely execute JavaScript in a sandboxed environment.

## Configuration

JSaaS is configured through environment variables. See the following table for a listing of variables:

| Name                                    | Description                                                                                                    |
| --------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| JSAAS_BIND_ADDR                         | Declare the address to bind to. Default: "127.0.0.1:9412"                                                      |
| JSAAS_SCRIPT_DEFINITION_EXPIRATION_TIME | If a script isn't executed in this duration (milliseconds), it is removed from the server. Default: "86400000" |
| JSAAS_SCRIPT_EXECUTION_THREAD_POOL_SIZE | Number of workers to use for executing JavaScript. 0 signifies number of CPUs availablet. Default: "0"         |
| JSAAS_SCRIPT_EXECUTION_COMPLETION_TIME  | Duration of time to wait for a script to finish executing before timing out. Default: "10000"                  |
| JSAAS_TLS_BIND_ADDR                     | If specified, and TLS is configured, a separate port will be bound for TLS instead of using the default one.   |
| JSAAS_TLS_PUBLIC_CERTIFICATE_PATH       | TLS public key path, PEM format. Note that TLS is currently only supported on Linux.                           |
| JSAAS_TLS_PRIVATE_KEY_PATH              | TLS private key path, PEM format. Note that TLS is currently only supported on Linux.                          |

## Usage

```bash
cargo run --release

~/work/jsaas#finish-program $ curl -i -XPOST --data 'function(a, b) { return a * b * 2; }' http://127.0.0.1:3000/scripts
HTTP/1.1 201 Created
content-type: application/json
location: /scripts/b3bca5f8-d6e8-4a11-8170-8fb238e3a216
content-length: 45
date: Sat, 05 Jan 2019 02:11:00 GMT

{"id":"b3bca5f8-d6e8-4a11-8170-8fb238e3a216"}-> 0

~/work/jsaas#finish-program $ curl -i -XPOST --data '[1, 2]' http://127.0.0.1:3000/scripts/b3bca5f8-d6e8-4a11-8170-8fb238e3a216
HTTP/1.1 200 OK
content-type: application/json
content-length: 1
date: Sat, 05 Jan 2019 02:11:36 GMT

4-> 0

~/work/jsaas#finish-program $ curl -i -XPOST --data 'function(a, b) { return { sum:  a + b } }' http://127.0.0.1:3000/scripts
HTTP/1.1 201 Created
content-type: application/json
location: /scripts/c6608297-fdd7-4116-828c-a23c255f7995
content-length: 45
date: Sat, 05 Jan 2019 02:12:34 GMT

{"id":"c6608297-fdd7-4116-828c-a23c255f7995"}-> 0

~/work/jsaas#finish-program $ curl -i -XPOST --data '[8, 32]' http://127.0.0.1:3000/scripts/c6608297-fdd7-4116-828c-a23c255f7995
HTTP/1.1 200 OK
content-type: application/json
content-length: 10
date: Sat, 05 Jan 2019 02:12:48 GMT

{"sum":40}-> 0
```

## Development

This project currently requires a POSIX-compliant operating system and bash, mostly due to its build setup. The first time that the project is compiled may take some time as the build downloads Duktape and configures it.

You'll need the following software:

* cargo
* curl
* gcc
* python2
* python2-yaml
* rustc

Once the environment is prepared, execute the following:

```bash
cargo build
```

A static binary can be produced:

```bash
cargo build --release --target=x86_64-unknown-linux-musl
```

A webserver can be started for development:

```bash
cargo run
```

(TBD)

## Releasing

(TBD)
