# Command-Line Extensions for `smart-config`

This library provides a couple of command-line extensions for the [`smart-config`] library:

- Printing help for configuration params with optional filtering.
- Debugging param values and deserialization errors.

## Usage

Add this to your `Crate.toml`:

```toml
[dependencies]
smart-config-commands = "0.1.0"
```

### Printing help on config params

```rust
use std::io;
use smart_config::ConfigSchema;
use smart_config_commands::print_help;

let mut schema = ConfigSchema::default();
// Add configurations to the schema...

print_help(&schema, |param| {
    // Allows filtering output params.
    param.name.contains("test")
})?;
io::Result::Ok(())
```

Example output is as follows:

![Example output for print_help](examples/help.svg)

### Debugging param values

```rust
use std::io;
use smart_config::{ConfigSchema, ConfigRepository};
use smart_config_commands::print_debug;

let mut schema = ConfigSchema::default();
// Add configurations to the schema...
let mut repo = ConfigRepository::new(&schema);
// Add sources to the repo...

print_debug(&repo)?;
io::Result::Ok(())
```

Example output is as follows:

![Example output for print_debug](examples/debug.svg)

The output will contain deserialization errors for all available params:

![Example output for print_debug](examples/errors.svg)

## License

Distributed under the terms of either

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

[`smart-config`]: ../smart-config
