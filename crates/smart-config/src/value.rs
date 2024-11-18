//! Enriched JSON object model that allows to associate values with origins.

use std::{collections::HashMap, fmt, sync::Arc};

use crate::metadata::NestedConfigMetadata;

#[derive(Debug, Clone)]
pub(crate) struct ValueOrigin(pub Arc<str>);

impl fmt::Display for ValueOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl ValueOrigin {
    pub fn env_var(name: &str) -> Self {
        Self(format!("env variable '{name}'").into())
    }

    // FIXME: more details
    pub fn group(nested_meta: &NestedConfigMetadata) -> Self {
        Self(format!("group '{}'", nested_meta.name).into())
    }
}

#[allow(dead_code)] // FIXME: allow deserializing from JSON etc.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<ValueWithOrigin>),
    Object(Map),
}

pub(crate) type Map = HashMap<String, ValueWithOrigin>;

#[derive(Debug, Clone)]
pub(crate) struct ValueWithOrigin {
    pub inner: Value,
    pub origin: ValueOrigin,
}

impl PartialEq for ValueWithOrigin {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl ValueWithOrigin {
    // FIXME: use `a.b.c` pointers
    pub fn pointer(&self, mut pointer: &str, final_part: &str) -> Option<&Self> {
        if !pointer.starts_with('/') {
            return None;
        }
        if let Some(trimmed_pointer) = pointer.strip_suffix('/') {
            pointer = trimmed_pointer;
        }

        let mut pointer_parts = pointer
            .split('/')
            .skip(1)
            .map(|part| part.replace("~1", "/").replace("~0", "~"))
            .chain([final_part.to_owned()]);
        pointer_parts.try_fold(self, |ptr, part| match &ptr.inner {
            Value::Object(map) => map.get(&part),
            Value::Array(array) => array.get(part.parse::<usize>().ok()?),
            _ => None,
        })
    }
}
