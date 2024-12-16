use std::{io, io::Write as _};

use anstream::stream::{AsLockedWrite, RawStream};
use anstyle::{AnsiColor, Color, Style};
use smart_config::{
    metadata::{BasicTypes, TypeDescription},
    ConfigSchema,
};

use crate::{ParamRef, Printer};

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
            let filtered_params = config
                .metadata()
                .params
                .iter()
                .map(|param| ParamRef { config, param })
                .filter(|&param_ref| filter(param_ref));

            for param_ref in filtered_params {
                param_ref.write_help(&mut writer)?;
                writeln!(&mut writer)?;
            }
        }
        Ok(())
    }
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
