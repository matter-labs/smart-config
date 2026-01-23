//! # Parsing configurations from multiple sources
//!
//! # Source kinds
//!
//! [Configuration types](trait@crate::DescribeConfig) can be parsed from multiple [sources](crate::ConfigSource).
//! Sources supported out of the box are:
//!
//! - [YAML](crate::Yaml) and [JSON](crate::Json) files
//! - [environment variables](crate::Environment).
//!
//! YAML and JSON sources are *structured*, i.e., support the full JSON object model. In other words,
//! config parameters can be serialized as objects or arrays.
//!
//! On the other hand, environment variables only support serialization of params as strings. There are a couple of workarounds
//! to bridge the gap:
//!
//! - [`Environment::coerce_json()`](crate::Environment::coerce_json()) allows to signal that the variable value is JSON,
//!   via the `__JSON` suffix appended to the variable name.
//! - Several deserializers like [`Delimited`](crate::de::Delimited) and [`DelimitedEntries`](crate::de::DelimitedEntries)
//!   allow to deserialize structured data from a string *in addition* to the structured object / array deserialization.
//!
//! # Combining sources
//!
//! Config sources can be combined in a [`ConfigSources`](crate::ConfigSources) object.
//! *How* sources are combined, depends on the app; the library doesn't force any particular choice.
//!
//! As an example, assume that we want to source configuration
//! from files supplied by the user via command-line args (like `--cfg-file base.yml:overrides.yml`),
//! and with higher-priority env variable overrides, where env variables are prefixed by `APP_`.
//! In this case, `ConfigSources` can be constructed as follows (using [`clap`] as the command-line parser):
//!
//! [`clap`]: https://docs.rs/clap/
//!
//! ```no_run
//! # use std::{fs, io, path::PathBuf};
//! # use anyhow::Context;
//! # use clap::Parser as _;
//! use smart_config::{
//!     ConfigRepository, ConfigSchema, ConfigSources, Environment, Json, Yaml,
//! };
//!
//! // Separator between paths
//! const PATH_SEP: char = if cfg!(windows) { ';' } else { ':' };
//!
//! #[derive(Debug, clap::Parser)]
//! struct Cli {
//!     /// Configuration files.
//!     #[arg(long, value_name = "FILE", value_delimiter = PATH_SEP)]
//!     cfg_file: Vec<PathBuf>,
//!     // Other command-line args...
//! }
//!
//! let cli: Cli = Cli::parse();
//! let mut sources = ConfigSources::default();
//!
//! // Add file configuration sources
//! for file in &cli.cfg_file {
//!     // Auto-detect file type by its extension
//!     let ext = file.extension().context("no file extension")?;
//!     let ext = ext.to_str().context("unsupported file extension")?;
//!     match ext {
//!         "json" => {
//!             let file_reader = io::BufReader::new(fs::File::open(file)?);
//!             let json = serde_json::from_reader(file_reader)?;
//!             sources.push(Json::new(&file.to_string_lossy(), json));
//!         }
//!         "yml" | "yaml" => {
//!             let file_reader = io::BufReader::new(fs::File::open(file)?);
//!             let yaml = serde_yaml::from_reader(file_reader)?;
//!             sources.push(Yaml::new(&file.to_string_lossy(), yaml)?);
//!         }
//!         _ => anyhow::bail!("unsupported extension: {ext}"),
//!     }
//! }
//!
//! // Add environment variables
//! let mut env = Environment::prefixed("APP_");
//! // Coerce JSON-suffixed env variables
//! env.coerce_json()?;
//! sources.push(env);
//!
//! // Parse configurations from the sources.
//! let schema: ConfigSchema = // ...
//! #    ConfigSchema::default();
//! let repo = ConfigRepository::new(&schema).with_all(sources);
//! // `repo` can be used to parse configurations, output canonical param values etc.
//! # anyhow::Ok(())
//! ```
//!
//! # See also
//!
//! - [Examples of YAML and env sources](super::derive_examples#advanced-features)
