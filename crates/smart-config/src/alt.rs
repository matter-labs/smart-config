//! Alternative value sources.

#![allow(missing_docs)] // FIXME

use std::{cell::RefCell, collections::HashMap, env, fmt, sync::Arc};

use crate::{
    source::ConfigContents,
    value::{Map, Pointer, StrValue, Value, ValueOrigin, WithOrigin},
    ConfigSchema, ConfigSource,
};

pub trait ProvideValue: 'static + Send + Sync + fmt::Debug + fmt::Display {
    fn provide_value(&self) -> Option<WithOrigin>;
}

thread_local! {
    static MOCK_ENV_VARS: RefCell<HashMap<String, String>> = RefCell::default();
}

#[derive(Debug)]
pub struct MockEnvGuard(());

impl MockEnvGuard {
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

#[derive(Debug, Clone, Copy)]
pub struct Env(pub &'static str);

impl fmt::Display for Env {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "env var {:?}", self.0)
    }
}

impl ProvideValue for Env {
    fn provide_value(&self) -> Option<WithOrigin> {
        let value = MOCK_ENV_VARS
            .with(|cell| cell.borrow().get(self.0).cloned())
            .or_else(|| env::var(self.0).ok());
        if let Some(value) = value {
            let value = Value::String(StrValue::Plain(value));
            let origin = ValueOrigin::Path {
                source: Arc::new(ValueOrigin::EnvVars),
                path: self.0.into(),
            };
            Some(WithOrigin::new(value, Arc::new(origin)))
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct Custom {
    description: &'static str,
    getter: fn() -> Option<WithOrigin>,
}

impl Custom {
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

impl ProvideValue for Custom {
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
