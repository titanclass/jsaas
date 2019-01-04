# JSaaS

*Currently at the POC stage, i.e. only proving that invoking Duktape from Rust is easy enough.*

## Overview

An HTTP service that uses the [Duktape](https://duktape.org/) JavaScript engine
to safely execute JavaScript in a sandboxed environment.

## HTTP design

The HTTP service provides two routes:

1) Define a JavaScript function (`/define`)
2) Execute that JavaScript function (`/eval`)

Functions are immutable once defined, and they expire when unused for some period of time.

### Function Definition

To define a function, issue a `POST` with its definition:

Example exchange:

```http
POST /define HTTP/1.1
Content-Type: application/javascript

function(one, two) {
  return {
    product: one * two,
    sum: one + two
  };
}
```

At this point, the service will respond with an id that points to the function.

*V4 UUIDs are used to ensure that the correct function is executed, given JSaaS could be restarted. Any other ID of sufficient random entropy would suffice.*

```http
HTTP/1.1 200 OK
Content-Type: application/json

{ "id": "72083b74-cffd-4c85-a195-c6c26b0729a5" }
```

### Function Execution

To execute a function, issue a `POST` with its id and an array of arguments (JSON encoded):

Example exchange:

```http
POST /eval/72083b74-cffd-4c85-a195-c6c26b0729a5 HTTP/1.1
Content-Type: application/json

[2, 3]
```

At this point, if the function exists, the service will respond with the result (JSON encoded):

```http
HTTP/1.1 OK
Content-Type: application/json

{ "product": 6, "sum": 5 }
```

If the function does not exist, the service will respond with a 404. The client can then decide to redefine it and try again, or abort.

*Note: Unused functions are periodically removed from the server, and are not persisted to disk, so this case must be considered.*

## Development

(TBD)

## Releasing

(TBD)
