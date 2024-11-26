# Smart Schema-driven Layered Configuration System

`smart-config` is a schema-driven layered configuration system with support of multiple configuration formats.

The task solved by the library is merging configuration input from a variety of prioritized sources
(JSON and YAML files, env variables, command-line args etc.) and converting this input to strongly typed
representation (i.e., config structs or enums). As with other config systems, config input follows the JSON object model,
with each value enriched with its origin (e.g., a path in a specific JSON file,
or a specific env var). This allows attributing errors during deserialization.

The defining feature of `smart-config` is its schema-driven design. Each config type has associated metadata
defined with the help of the derive macros.
Metadata includes a variety of info extracted from the config type:

- Parameter info: name (including aliases and renaming), help (extracted from doc comments),
  type, deserializer for the param etc.
- Nested / flattened configurations.

Multiple configurations are collected into a global schema. Each configuration is *mounted* at a specific path.
E.g., if a large app has an HTTP server component, it may be mounted at `api.http`. Multiple config types may be mounted
at the same path (e.g., flattened configs); conversely, a single config type may be mounted at multiple places.
As a result, there doesn't need to be a god object uniting all configs in the app; they may be dynamically collected and deserialized
inside relevant components.

This information provides rich human-readable info about configs. It also assists when preprocessing and merging config inputs.
For example, env vars are a flat string -> string map; with the help of a schema, it's possible to:

- Correctly nest vars (e.g., transform the `API_HTTP_PORT` var into a `port` var inside `http` object inside `api` object)
- Transform value types from strings to expected types.
