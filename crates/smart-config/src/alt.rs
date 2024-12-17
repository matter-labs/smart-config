//! Alternative value sources.

#![allow(missing_docs)] // FIXME

use std::{env, fmt, sync::Arc};

use crate::value::{StrValue, Value, ValueOrigin, WithOrigin};

pub trait ProvideValue: 'static + Send + Sync + fmt::Debug + fmt::Display {
    fn provide_value(&self) -> Option<WithOrigin>;
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
        if let Ok(value) = env::var(self.0) {
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
