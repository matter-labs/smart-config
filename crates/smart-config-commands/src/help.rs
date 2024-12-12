use std::{io, io::Write as _};

use anstream::stream::{AsLockedWrite, RawStream};
use anstyle::{AnsiColor, Color, Style};
use smart_config::ConfigSchema;

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

        let qualifiers = self.param.deserializer.type_qualifiers();
        let maybe_secret = if qualifiers.is_secret() {
            format!("{SECRET}secret{SECRET:#} ")
        } else {
            String::new()
        };
        let kind = self.param.expecting;
        let ty = format!(
            "{maybe_secret}{kind} {DIMMED}[Rust: {}]{DIMMED:#}",
            self.param.rust_type.name_in_code()
        );

        let description = if let Some(description) = qualifiers.description() {
            format!("; {description}")
        } else {
            String::new()
        };
        let unit = if let Some(unit) = qualifiers.unit() {
            format!("; unit: {UNIT}{unit}{UNIT:#}")
        } else {
            String::new()
        };
        writeln!(
            writer,
            "{INDENT}{FIELD}Type{FIELD:#}: {ty}{description}{unit}"
        )?;

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
