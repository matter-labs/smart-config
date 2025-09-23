# Smart Schema-driven Layered Configuration System

[![Build status](https://github.com/matter-labs/smart-config/actions/workflows/ci.yml/badge.svg)](https://github.com/matter-labs/smart-config/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%2FApache--2.0-blue)](https://github.com/matter-labs/smart-config#license)
![rust 1.86+ required](https://img.shields.io/badge/rust-1.86+-blue.svg?label=Required%20Rust)

**Docs:**
[![crate docs (main)](https://img.shields.io/badge/main-yellow.svg?label=docs)](https://matter-labs.github.io/smart-config/smart_config/)

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

This information provides rich human-readable info about configs. It also assists when preprocessing and merging config inputs.
For example, env vars are a flat string -> string map; with the help of a schema, it's possible to:

- Correctly nest vars (e.g., transform the `API_HTTP_PORT` var into a `port` var inside `http` object inside `api` object)
- Transform value types from strings to expected types.

## Features

- Rich, self-documenting configuration schema.
- Utilizes the schema to enrich configuration sources and intelligently merge them.
- Doesn't require a god object uniting all configs in the app; they may be dynamically collected and deserialized
  inside relevant components.
- Supports lazy parsing for complex / multi-component apps (only the used configs are parsed; other configs are not required).
- Supports multiple configuration formats and programmable source priorities (e.g., `base.yml` + overrides from the
  `overrides/` dir in the alphabetic order + env vars).
- Rich and complete deserialization errors including locations and value origins.

## Usage

Add this to your `Crate.toml`:
<!--- x-release-please-start-version -->
```toml
[dependencies]
smart-config = "0.3.0-pre"
```
<!--- x-release-please-end -->

### Declaring configurations

```rust
use std::{collections::{HashMap, HashSet}, path::PathBuf, time::Duration};
use serde::{Deserialize, Serialize};
use smart_config::{
    de::{Optional, Serde}, metadata::*, ByteSize, DescribeConfig, DeserializeConfig,
};

#[derive(Debug, Serialize, Deserialize)]
enum CustomEnum {
    First,
    Second,
}

/// Configuration with type params of several types.
#[derive(Debug, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))] // derive according to default values for params
pub struct TestConfig {
    /// Port to bind to.
    #[config(default_t = 8080, alias = "http_port")]
    pub port: u16,
    #[config(default_t = "test".into(), deprecated = "app_name")]
    pub name: String,
    #[config(default_t = "./test".into())]
    pub path: PathBuf,

    // Basic collections are supported as well:
    #[config(default)]
    pub vec: Vec<u64>,
    #[config(default)]
    pub set: HashSet<String>,
    #[config(default)]
    pub map: HashMap<String, u64>,

    // For custom types, you can specify a custom deserializer. The deserializer below
    // expects a string and works for all types implementing `serde::Deserialize`.
    #[config(with = Serde![str])]
    #[config(default_t = CustomEnum::First)]
    pub custom: CustomEnum,

    // There is dedicated support for durations and byte sizes.
    #[config(default_t = Duration::from_millis(100))]
    pub short_dur: Duration,
    #[config(default_t = Some(128 * SizeUnit::MiB))]
    pub memory_size: Option<ByteSize>,

    // Configuration nesting and flattening are supported:
    #[config(nest)]
    pub nested: NestedConfig,
    #[config(flatten)]
    pub flattened: NestedConfig,
}

#[derive(Debug, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct NestedConfig {
    #[config(default)]
    pub other_int: u32,
}
```

### Testing config deserialization

```rust
use smart_config::{config, testing, DescribeConfig, DeserializeConfig};

#[derive(Debug, DescribeConfig, DeserializeConfig)]
pub struct TestConfig {
    #[config(default_t = 8080)]
    pub port: u16,
    #[config(default_t = "test".into())]
    pub name: String,
}

let input = config!("port": 3000, "name": "app");
// `test_complete` ensures that all params are mentioned in the input
let config = testing::test_complete::<TestConfig>(input).unwrap();
assert_eq!(config.port, 3000);
assert_eq!(config.name, "app");
```

### Deserializing config

```rust
use smart_config::{
    config, ConfigSchema, ConfigRepository, DescribeConfig, DeserializeConfig, Yaml, Environment,
};

#[derive(Debug, DescribeConfig, DeserializeConfig)]
pub struct TestConfig {
    pub port: u16,
    #[config(default_t = "test".into())]
    pub name: String,
    #[config(default_t = true)]
    pub tracing: bool,
}

let mut schema = ConfigSchema::default();
schema.insert(&TestConfig::DESCRIPTION, "test");
// Assume we use two config sources: a YAML file and env vars,
// the latter having higher priority.
let yaml = r"
test:
  port: 4000
  name: app
";
let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml)?)?;
let env = Environment::from_iter("APP_", [("APP_TEST_PORT", "8000")]);
// Add both sources to a repo.
let repo = ConfigRepository::new(&schema).with(yaml).with(env);
// Get the parser for the config.
let parser = repo.single::<TestConfig>()?;
let config = parser.parse()?;
assert_eq!(config.port, 8_000); // from the env var
assert_eq!(config.name, "app"); // from YAML
assert!(config.tracing); // from the default value

anyhow::Ok(())
```

See crate docs for more examples.

## Alternatives and similar tools

- [`config`](https://crates.io/crates/config) and [`figment`](https://crates.io/crates/figment) are multi-layered configuration libraries.
  They provide a similar scope of functionality, missing some features (e.g., auto-generated docs, smart handling of env vars,
  extended error reporting, smart coercions etc.).
- [`envy`](https://crates.io/crates/envy) provides `serde`-based parsing from env vars.

## License

Distributed under the terms of either

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
