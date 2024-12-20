use std::{io, io::Write as _};

use anstream::stream::{AsLockedWrite, RawStream};
use anstyle::{AnsiColor, Color, Style};
use smart_config::{
    metadata::{BasicTypes, TypeDescription},
    ConfigRef, ConfigSchema,
};

use crate::{ParamRef, Printer, CONFIG_PATH};

const INDENT: &str = "  ";
const DIMMED: Style = Style::new().dimmed();
const MAIN_NAME: Style = Style::new().bold();
const FIELD: Style = Style::new().underline();
const DEFAULT_VAL: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
const UNIT: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
const SECRET: Style = Style::new()
    .bg_color(Some(Color::Ansi(AnsiColor::Cyan)))
    .fg_color(None);

impl<W: RawStream + AsLockedWrite> Printer<W> {
    /// Prints help on config params in the provided `schema`. Params can be filtered by the supplied predicate.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors.
    pub fn print_help(
        self,
        schema: &ConfigSchema,
        mut filter: impl FnMut(ParamRef<'_>) -> bool,
    ) -> io::Result<()> {
        let mut writer = self.writer;
        for config in schema.iter() {
            let filtered_params: Vec<_> = config
                .metadata()
                .params
                .iter()
                .map(|param| ParamRef { config, param })
                .filter(|&param_ref| filter(param_ref))
                .collect();
            if filtered_params.is_empty() {
                continue;
            }

            let validations = config.metadata().validations;
            if !validations.is_empty() {
                write_config_help(&mut writer, config)?;
            }

            for param_ref in filtered_params {
                param_ref.write_help(&mut writer)?;
                writeln!(&mut writer)?;
            }
        }
        Ok(())
    }
}

fn write_config_help(writer: &mut impl io::Write, config: ConfigRef<'_>) -> io::Result<()> {
    writeln!(
        writer,
        "{MAIN_NAME}{CONFIG_PATH}{}{CONFIG_PATH:#}{MAIN_NAME:#}",
        config.prefix()
    )?;
    for alias in config.aliases() {
        writeln!(writer, "{CONFIG_PATH}{alias}{CONFIG_PATH:#}")?;
    }
    writeln!(
        writer,
        "{INDENT}{FIELD}Config{FIELD:#}: {}",
        config.metadata().ty.name_in_code()
    )?;

    writeln!(writer, "{INDENT}{FIELD}Validations{FIELD:#}:")?;
    for &validation in config.metadata().validations {
        let description = validation.to_string();
        writeln!(writer, "{INDENT}- {description}")?;
    }
    Ok(())
}

impl ParamRef<'_> {
    fn write_help(&self, writer: &mut impl io::Write) -> io::Result<()> {
        let all_paths = self.all_paths_inner();
        let mut main_name = true;
        for (prefix, name) in all_paths {
            let prefix_sep = if prefix.is_empty() || prefix.ends_with('.') {
                ""
            } else {
                "."
            };
            let name_style = if main_name { MAIN_NAME } else { Style::new() };
            main_name = false;
            writeln!(
                writer,
                "{DIMMED}{prefix}{prefix_sep}{DIMMED:#}{name_style}{name}{name_style:#}"
            )?;
        }

        let description = self.param.type_description();
        write_type_description(writer, "Type", 2, self.param.expecting, &description)?;

        if let Some(default) = self.param.default_value() {
            writeln!(
                writer,
                "{INDENT}{FIELD}Default{FIELD:#}: {DEFAULT_VAL}{default:?}{DEFAULT_VAL:#}"
            )?;
        }
        if let Some(fallback) = self.param.fallback {
            write!(writer, "{INDENT}{FIELD}Fallbacks{FIELD:#}: ")?;
            let fallback = fallback.to_string();
            let mut lines = fallback.lines();
            if let Some(first_line) = lines.next() {
                writeln!(writer, "{first_line}")?;
                for line in lines {
                    writeln!(writer, "{INDENT}  {line}")?;
                }
            }
        }

        if !self.param.help.is_empty() {
            for line in self.param.help.lines() {
                writeln!(writer, "{INDENT}{line}")?;
            }
        }
        Ok(())
    }
}

fn write_type_description(
    writer: &mut impl io::Write,
    relation_to_parent: &str,
    indent: usize,
    expecting: BasicTypes,
    description: &TypeDescription,
) -> io::Result<()> {
    let maybe_secret = if description.contains_secrets() {
        format!("{SECRET}secret{SECRET:#} ")
    } else {
        String::new()
    };
    let ty = format!(
        "{maybe_secret}{expecting} {DIMMED}[Rust: {}]{DIMMED:#}",
        description.rust_type()
    );

    let details = if let Some(details) = description.details() {
        format!("; {details}")
    } else {
        String::new()
    };
    let unit = if let Some(unit) = description.unit() {
        format!("; unit: {UNIT}{unit}{UNIT:#}")
    } else {
        String::new()
    };
    writeln!(
        writer,
        "{:>indent$}{FIELD}{relation_to_parent}{FIELD:#}: {ty}{details}{unit}",
        ""
    )?;

    if let Some((expecting, item)) = description.items() {
        write_type_description(writer, "Array items", indent + 2, expecting, item)?;
    }
    if let Some((expecting, key)) = description.keys() {
        write_type_description(writer, "Map keys", indent + 2, expecting, key)?;
    }
    if let Some((expecting, value)) = description.values() {
        write_type_description(writer, "Map values", indent + 2, expecting, value)?;
    }

    Ok(())
}
