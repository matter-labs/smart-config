//! Functionality shared by multiple CLI commands.

use std::io::{self, Write as _};

use anstream::stream::{AsLockedWrite, RawStream};
use anstyle::{AnsiColor, Color, Style};
use smart_config::value::{StrValue, Value, WithOrigin};

use crate::Printer;

pub(crate) const STRING: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
const NULL: Style = Style::new().bold();
const BOOL: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
const NUMBER: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
const SECRET: Style = Style::new()
    .bg_color(Some(Color::Ansi(AnsiColor::Cyan)))
    .fg_color(None);
const OBJECT_KEY: Style = Style::new().bold();

impl<W: RawStream + AsLockedWrite> Printer<W> {
    /// Outputs JSON with syntax highlighting.
    ///
    /// # Errors
    ///
    /// Proxies I/O errors.
    pub fn print_json(&mut self, json: &serde_json::Value) -> io::Result<()> {
        write_json_value(&mut self.writer, json, 0)?;
        writeln!(&mut self.writer)
    }

    /// Outputs YAML adhering to the JSON model with syntax highlighting.
    ///
    /// # Errors
    ///
    /// Proxies I/O errors.
    pub fn print_yaml(&mut self, json: &serde_json::Value) -> io::Result<()> {
        write_yaml_value(&mut self.writer, json, 0, false)?;
        writeln!(&mut self.writer)
    }
}

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
                for (i, item) in val.iter().enumerate() {
                    write!(writer, "{:ident$}  ", "")?;
                    write_json_value(writer, item, ident + 2)?;
                    if i + 1 < val.len() {
                        writeln!(writer, ",")?;
                    } else {
                        writeln!(writer)?;
                    }
                }
                write!(writer, "{:ident$}]", "")
            }
        }
        serde_json::Value::Object(val) => {
            if val.is_empty() {
                write!(writer, "{{}}")
            } else {
                writeln!(writer, "{{")?;
                for (i, (key, value)) in val.iter().enumerate() {
                    write!(writer, "{:ident$}  {OBJECT_KEY}{key:?}{OBJECT_KEY:#}: ", "")?;
                    write_json_value(writer, value, ident + 2)?;
                    if i + 1 < val.len() {
                        writeln!(writer, ",")?;
                    } else {
                        writeln!(writer)?;
                    }
                }
                write!(writer, "{:ident$}}}", "")
            }
        }
    }
}

fn yaml_string(val: &str) -> String {
    // YAML has arcane rules escaping strings, so we just use the library.
    let mut yaml = serde_yaml::to_string(val).unwrap();
    if yaml.ends_with('\n') {
        yaml.pop();
    }
    yaml
}

fn write_yaml_value(
    writer: &mut impl io::Write,
    value: &serde_json::Value,
    ident: usize,
    is_array_item: bool,
) -> io::Result<()> {
    match value {
        serde_json::Value::Null => write!(writer, "{NULL}null{NULL:#}"),
        serde_json::Value::Bool(val) => write!(writer, "{BOOL}{val:?}{BOOL:#}"),
        serde_json::Value::Number(val) => write!(writer, "{NUMBER}{val}{NUMBER:#}"),
        serde_json::Value::String(val) => {
            let yaml_val = yaml_string(val);
            write!(writer, "{STRING}{yaml_val}{STRING:#}")
        }
        serde_json::Value::Array(val) => {
            if val.is_empty() {
                if ident > 0 {
                    write!(writer, " ")?; // We haven't output a space before the array in the parent array / object
                }
                write!(writer, "[]")
            } else {
                if ident > 0 {
                    writeln!(writer)?;
                }
                for (i, item) in val.iter().enumerate() {
                    write!(writer, "{:ident$}-", "")?;
                    if !item.is_array() {
                        write!(writer, " ")?; // If the item is another array, we'll output a newline instead
                    }

                    write_yaml_value(writer, item, ident + 2, true)?;
                    if i + 1 < val.len() {
                        writeln!(writer)?;
                    }
                }
                Ok(())
            }
        }
        serde_json::Value::Object(val) => {
            if val.is_empty() {
                if ident > 0 {
                    write!(writer, " ")?; // We haven't output a space before the array in the parent array / object
                }
                write!(writer, "{{}}")
            } else {
                if ident > 0 && !is_array_item {
                    writeln!(writer)?;
                }
                for (i, (key, value)) in val.iter().enumerate() {
                    let yaml_key = yaml_string(key);
                    if is_array_item && i == 0 {
                        // Skip padding for the first item in an array
                    } else {
                        write!(writer, "{:ident$}", "")?;
                    }
                    write!(writer, "{OBJECT_KEY}{yaml_key}{OBJECT_KEY:#}:")?;
                    if !value.is_object() && !value.is_array() {
                        write!(writer, " ")?; // If the child value is an object or array, we'll output a newline
                    }

                    write_yaml_value(writer, value, ident + 2, false)?;
                    if i + 1 < val.len() {
                        writeln!(writer)?;
                    }
                }
                Ok(())
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

#[cfg(test)]
mod tests {
    use anstream::AutoStream;

    use crate::utils::write_yaml_value;

    #[test]
    fn writing_yaml() {
        let original_yaml = "\
test:
  app_name: test
  cache_size: 128 MiB
  dir_paths:
    - /usr/local/bin
    - /usr/bin
  funding:
    address: '0x0000000000000000000000000000000000001234'
    api_key: correct horse battery staple
    balance: '0x123456'
    secret_key: 0x000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f
  nested:
    complex:
      array:
        - 1
        - 2
      map:
        value: 25
    exit_on_error: false
    method_limits:
      - method: eth_blockNumber
        rps: 3
      - method: eth_getLogs
        rps: 100
    more_timeouts: []
  object_store:
    bucket_name: test-bucket
    type: gcs
  poll_latency: 300ms
  port: 3000
  required: 123
  scaling_factor: 4.199999809265137
  temp_dir: /var/folders/mw/lhb7m9dj3jbdm3w994t0_c8h0000gn/T/
  timeout_sec: 60";
        let json: serde_json::Value = serde_yaml::from_str(original_yaml).unwrap();
        assert!(json["test"].as_object().unwrap().len() > 10, "{json:?}");

        let mut buffer = vec![];
        write_yaml_value(&mut AutoStream::never(&mut buffer), &json, 0, false).unwrap();
        let produced_yaml = String::from_utf8(buffer).unwrap();
        assert_eq!(produced_yaml, original_yaml);
    }
}
