# Command-Line Extensions for `smart-config`

[![Build status](https://github.com/matter-labs/smart-config/actions/workflows/ci.yml/badge.svg)](https://github.com/matter-labs/smart-config/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%2FApache--2.0-blue)](https://github.com/matter-labs/smart-config#license)
![rust 1.91+ required](https://img.shields.io/badge/rust-1.91+-blue.svg?label=Required%20Rust)

**Docs:** [![Docs.rs](https://docs.rs/smart-config-commands/badge.svg)](https://docs.rs/smart-config-commands/)
[![crate docs (main)](https://img.shields.io/badge/main-yellow.svg?label=docs)](https://matter-labs.github.io/smart-config/smart_config_commands/)

This library provides a couple of command-line extensions for the [`smart-config`] library:

- Printing help for configuration params with optional filtering.
- Generating a Markdown reference for configuration params.
- Debugging param values and deserialization errors.

## Usage

Add this to your `Crate.toml`:
<!--- x-release-please-start-version -->
```toml
[dependencies]
smart-config-commands = "0.4.0-pre.4"
```
<!--- x-release-please-end -->

### Printing help on config params

```rust
use std::io;
use smart_config::ConfigSchema;
use smart_config_commands::Printer;

let mut schema = ConfigSchema::default();
// Add configurations to the schema...

Printer::stdout().print_help(&schema, |param_ref| {
    // Allows filtering output params.
    param_ref.param.name.contains("test")
})?;
io::Result::Ok(())
```

Example output is as follows:

![Example output for print_help](examples/help.svg)

### Generating Markdown reference docs

```rust
use std::io;
use smart_config::ConfigSchema;
use smart_config_commands::{MarkdownOptions, Printer};

let mut schema = ConfigSchema::default();
// Add configurations to the schema...

Printer::stdout().print_markdown_reference(&schema, &MarkdownOptions::default(), |_| true)?;
io::Result::Ok(())
```

For a binary exposing configuration utilities, prefer a command name based on the task, for example:

```shell
config docs > docs/src/setup/configuration.md
```

### Debugging param values

```rust
use std::io;
use smart_config::{ConfigSchema, ConfigRepository};
use smart_config_commands::Printer;

let mut schema = ConfigSchema::default();
// Add configurations to the schema...
let mut repo = ConfigRepository::new(&schema);
// Add sources to the repo...

Printer::stdout().print_debug(&repo, |_| true)?;
io::Result::Ok(())
```

Example output is as follows:

![Example output for print_debug](examples/debug.svg)

The output will contain deserialization errors for all available params:

![Example output for print_debug](examples/errors.svg)

### Outputting JSON / YAML

The library can fancy-print JSON and YAML. This be used together with `smart-config` tooling to produce default / example configs,
diffs with default param values etc. See [the example](examples/cli.rs) for a couple of use cases.

![Example of fancy-printed YAML](examples/ser-yaml.svg)

## License

Distributed under the terms of either

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

[`smart-config`]: ../smart-config
