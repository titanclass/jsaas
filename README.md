# JSaaS

## Overview

An HTTP service that uses the [Duktape](https://duktape.org/) JavaScript engine to safely execute JavaScript in a sandboxed environment.

## Getting Started

Using [Docker](https://www.docker.com/), start the service (be sure to replace <version> below):

> You can find the latest version on [DockerHub](https://cloud.docker.com/u/titanclass/repository/docker/titanclass/jsaas/tags)

```bash
docker run -e JSAAS_BIND_ADDR=0.0.0.0:9412 -p 9412:9412 --rm -ti titanclass/jsaas:<version>
```

Define a program that adds two numbers:

```bash
curl -XPOST --data 'function(a, b) { return a + b; }' http://localhost:9412/scripts
```

which yields:

```
{"id":"af15791e-e9c1-4750-8a44-60222ef88c7c"}
```

Next, execute the program by supplying the numbers:

```bash
curl -XPOST --data '[4, 5]' http://localhost:9412/scripts/af15791e-e9c1-4750-8a44-60222ef88c7c
```

which yields:

```
9
```

In a real-world scenario, you can also return a JS object or any other JSON-serializable value.

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

## Releasing

To release, push a tag that starts with "v" -- e.g. "v0.2.0" -- and CircleCI will build the project and push an image to DockerHub.

(c)opyright 2019, Titan Class P/L
