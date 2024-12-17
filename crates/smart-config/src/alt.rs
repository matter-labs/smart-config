//! Alternative [`Value`] sources.
//!
//! # Motivation and use cases
//!
//! Some configuration params may be sourced from places that do not fit well into the hierarchical config schema.
//! For example, a config param with logging directives may want to read from a `RUST_LOG` env var, regardless of where
//! the param is placed in the hierarchy. It is possible to manually move raw config values around, it may get unmaintainable
//! for large configs.
//!
//! *Alternatives* provide a more sound approach: declare the alternative config sources as a part of the [`DescribeConfig`](macro@crate::DescribeConfig)
//! derive macro. In this way, alternatives are documented (being a part of the config metadata)
//! and do not require splitting logic between config declaration and preparing config sources.
//!
//! Alternatives should be used sparingly, since they make it more difficult to reason about configs due to their non-local nature.
//!
//! # Features and limitations
//!
//! - By design, alternatives are location-independent. E.g., an [`Env`] alternative will always read from the same env var,
//!   regardless of where the param containing it is placed (including the case when it has multiple copies!).
//! - Alternatives always have lower priority than all other config sources.

use std::{cell::RefCell, collections::HashMap, env, fmt, sync::Arc};

use crate::{
    source::ConfigContents,
    value::{Map, Pointer, Value, ValueOrigin, WithOrigin},
    ConfigSchema, ConfigSource,
};

/// Alternative source of a configuration param.
pub trait AltSource: 'static + Send + Sync + fmt::Debug + fmt::Display {
    /// Potentially provides a value for the param.
    ///
    /// Implementations should return `None` (vs `Some(Value::Null)` etc.) if the source doesn't have a value.
    fn provide_value(&self) -> Option<WithOrigin>;
}

thread_local! {
    static MOCK_ENV_VARS: RefCell<HashMap<String, String>> = RefCell::default();
}

/// Thread-local guard for mock env variables read by the [`Env`] value provider.
///
/// While a guard is active, all vars defined [when creating it](Self::new()) will be used in place of
/// the corresponding env vars.
///
/// # Examples
///
/// See [`Env`] for the examples of usage.
#[derive(Debug)]
pub struct MockEnvGuard(());

impl MockEnvGuard {
    /// Creates a guard that defines the specified env vars.
    ///
    /// # Panics
    ///
    /// Panics if another guard is active for the same thread.
    pub fn new<S: Into<String>>(vars: impl IntoIterator<Item = (S, S)>) -> Self {
        MOCK_ENV_VARS.with(|cell| {
            let mut map = cell.borrow_mut();
            assert!(
                map.is_empty(),
                "Cannot define mock env vars while another `MockEnvGuard` is active"
            );
            *map = vars
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect();
        });
        Self(())
    }
}

impl Drop for MockEnvGuard {
    fn drop(&mut self) {
        MOCK_ENV_VARS.take(); // Remove all mocked env vars
    }
}

/// Gets a string value from the specified env variable.
///
/// This source is aware of mock env vars provided via [`MockEnvGuard`].
///
/// # Examples
///
/// ```
/// use smart_config::{alt, testing, DescribeConfig, DeserializeConfig};
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     /// Log directives. Always read from `RUST_LOG` env var in addition to
///     /// the conventional sources.
///     #[config(default_t = "info".into(), alt = &alt::Env("RUST_LOG"))]
///     log_directives: String,
/// }
///
/// let config: TestConfig = testing::test(smart_config::config!())?;
/// // Without env var set or other sources, the param will assume the default value.
/// assert_eq!(config.log_directives, "info");
///
/// let _guard = alt::MockEnvGuard::new([("RUST_LOG", "warn")]);
/// let config: TestConfig = testing::test(smart_config::config!())?;
/// assert_eq!(config.log_directives, "warn");
///
/// // Mock env vars are still set here, but alternatives have lower priority
/// // than other sources.
/// let input = smart_config::config!("log_directives": "info,my_crate=debug");
/// let config: TestConfig = testing::test(input)?;
/// assert_eq!(config.log_directives, "info,my_crate=debug");
/// # anyhow::Ok(())
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Env(pub &'static str);

impl fmt::Display for Env {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "env var {:?}", self.0)
    }
}

impl Env {
    /// Gets the raw string value of the env var, taking [mock vars](MockEnvGuard) into account.
    pub fn get_raw(&self) -> Option<String> {
        MOCK_ENV_VARS
            .with(|cell| cell.borrow().get(self.0).cloned())
            .or_else(|| env::var(self.0).ok())
    }
}

impl AltSource for Env {
    fn provide_value(&self) -> Option<WithOrigin> {
        if let Some(value) = self.get_raw() {
            let origin = ValueOrigin::Path {
                source: Arc::new(ValueOrigin::EnvVars),
                path: self.0.into(),
            };
            Some(WithOrigin::new(value.into(), Arc::new(origin)))
        } else {
            None
        }
    }
}

/// Custom [value provider](AltSource).
///
/// # Examples
///
/// ```
/// # use std::sync::Arc;
/// use smart_config::{
///     alt, testing, value::{ValueOrigin, WithOrigin},
///     DescribeConfig, DeserializeConfig,
/// };
///
/// // Value source combining two env variables. It usually makes sense to split off
/// // the definition like this so that it's more readable.
/// const COMBINED_VARS: &'static dyn alt::AltSource =
///     &alt::Custom::new("$TEST_ENV - $TEST_NETWORK", || {
///         let env = alt::Env("TEST_ENV").get_raw()?;
///         let network = alt::Env("TEST_NETWORK").get_raw()?;
///         let origin = Arc::new(ValueOrigin::EnvVars);
///         Some(WithOrigin::new(format!("{env} - {network}").into(), origin))
///     });
///
/// #[derive(DescribeConfig, DeserializeConfig)]
/// struct TestConfig {
///     #[config(default_t = "app".into(), alt = COMBINED_VARS)]
///     app: String,
/// }
///
/// let _guard = alt::MockEnvGuard::new([
///     ("TEST_ENV", "stage"),
///     ("TEST_NETWORK", "goerli"),
/// ]);
/// let config: TestConfig = testing::test(smart_config::config!())?;
/// assert_eq!(config.app, "stage - goerli");
/// # anyhow::Ok(())
/// ```
#[derive(Debug)]
pub struct Custom {
    description: &'static str,
    getter: fn() -> Option<WithOrigin>,
}

impl Custom {
    /// Creates a provider with the specified human-readable description and a getter function.
    pub const fn new(description: &'static str, getter: fn() -> Option<WithOrigin>) -> Self {
        Self {
            description,
            getter,
        }
    }
}

impl fmt::Display for Custom {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.description)
    }
}

impl AltSource for Custom {
    fn provide_value(&self) -> Option<WithOrigin> {
        (self.getter)()
    }
}

#[derive(Debug)]
pub(crate) struct Alternatives {
    inner: HashMap<(String, &'static str), WithOrigin>,
    origin: Arc<ValueOrigin>,
}

impl Alternatives {
    pub(crate) fn new(schema: &ConfigSchema) -> Option<Self> {
        let mut inner = HashMap::new();
        for (prefix, config) in schema.iter_ll() {
            for param in config.metadata.params {
                let Some(alt) = param.alt else {
                    continue;
                };
                if let Some(mut val) = alt.provide_value() {
                    let origin = ValueOrigin::Synthetic {
                        source: val.origin.clone(),
                        transform: format!(
                            "alternative for `{}.{}`",
                            config.metadata.ty.name_in_code(),
                            param.rust_field_name,
                        ),
                    };
                    val.origin = Arc::new(origin);
                    inner.insert((prefix.0.to_owned(), param.name), val);
                }
            }
        }

        if inner.is_empty() {
            None
        } else {
            Some(Self {
                inner,
                origin: Arc::new(ValueOrigin::Alternatives),
            })
        }
    }
}

impl ConfigSource for Alternatives {
    fn origin(&self) -> Arc<ValueOrigin> {
        self.origin.clone()
    }

    fn into_contents(self) -> ConfigContents {
        let origin = self.origin;
        let mut map = WithOrigin::new(Value::Object(Map::new()), origin.clone());
        for ((prefix, name), value) in self.inner {
            map.ensure_object(Pointer(&prefix), |_| origin.clone())
                .insert(name.to_owned(), value);
        }
        ConfigContents::Hierarchical(match map.inner {
            Value::Object(map) => map,
            _ => unreachable!(),
        })
    }
}