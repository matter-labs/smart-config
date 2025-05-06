//! Functionality shared by multiple CLI commands.

use std::io;

use anstyle::{AnsiColor, Color, Style};
use smart_config::value::{StrValue, Value, WithOrigin};

pub(crate) const STRING: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
const NULL: Style = Style::new().bold();
const BOOL: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
const NUMBER: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
const SECRET: Style = Style::new()
    .bg_color(Some(Color::Ansi(AnsiColor::Cyan)))
    .fg_color(None);
const OBJECT_KEY: Style = Style::new().bold();

pub(crate) fn write_json_value(
    writer: &mut impl io::Write,
    value: &serde_json::Value,
    ident: usize,
) -> io::Result<()> {
    match value {
        serde_json::Value::Null => write!(writer, "{NULL}null{NULL:#}"),
        serde_json::Value::Bool(val) => write!(writer, "{BOOL}{val:?}{BOOL:#}"),
        serde_json::Value::Number(val) => write!(writer, "{NUMBER}{val}{NUMBER:#}"),
        serde_json::Value::String(val) => write!(writer, "{STRING}{val:?}{STRING:#}"),
        serde_json::Value::Array(val) => {
            if val.is_empty() {
                write!(writer, "[]")
            } else {
                writeln!(writer, "[")?;
                for item in val {
                    write!(writer, "{:ident$}  ", "")?;
                    write_json_value(writer, item, ident + 2)?;
                    writeln!(writer, ",")?;
                }
                write!(writer, "{:ident$}]", "")
            }
        }
        serde_json::Value::Object(val) => {
            if val.is_empty() {
                write!(writer, "{{}}")
            } else {
                writeln!(writer, "{{")?;
                for (key, value) in val {
                    write!(writer, "{:ident$}  {OBJECT_KEY}{key:?}{OBJECT_KEY:#}: ", "")?;
                    write_json_value(writer, value, ident + 2)?;
                    writeln!(writer, ",")?;
                }
                write!(writer, "{:ident$}}}", "")
            }
        }
    }
}

pub(crate) fn write_value(
    writer: &mut impl io::Write,
    value: &WithOrigin,
    ident: usize,
) -> io::Result<()> {
    match &value.inner {
        Value::Null => write!(writer, "{NULL}null{NULL:#}"),
        Value::Bool(val) => write!(writer, "{BOOL}{val:?}{BOOL:#}"),
        Value::Number(val) => write!(writer, "{NUMBER}{val}{NUMBER:#}"),
        Value::String(StrValue::Plain(val)) => write!(writer, "{STRING}{val:?}{STRING:#}"),
        Value::String(StrValue::Secret(_)) => write!(writer, "{SECRET}[REDACTED]{SECRET:#}"),
        Value::Array(val) => {
            writeln!(writer, "[")?;
            for item in val {
                write!(writer, "{:ident$}  ", "")?;
                write_value(writer, item, ident + 2)?;
                writeln!(writer, ",")?;
            }
            write!(writer, "{:ident$}]", "")
        }
        Value::Object(val) => {
            writeln!(writer, "{{")?;
            for (key, value) in val {
                write!(writer, "{:ident$}  {OBJECT_KEY}{key:?}{OBJECT_KEY:#}: ", "")?;
                write_value(writer, value, ident + 2)?;
                writeln!(writer, ",")?;
            }
            write!(writer, "{:ident$}}}", "")
        }
    }
}
