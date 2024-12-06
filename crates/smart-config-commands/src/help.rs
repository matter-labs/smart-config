use std::{io, io::Write as _, iter};

use anstream::stream::{AsLockedWrite, RawStream};
use anstyle::{AnsiColor, Color, Style};
use smart_config::{metadata::ParamMetadata, ConfigRef, ConfigSchema};

use crate::Printer;

const INDENT: &str = "  ";
const DIMMED: Style = Style::new().dimmed();
const MAIN_NAME: Style = Style::new().bold();
const FIELD: Style = Style::new().underline();
const DEFAULT_VAL: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
const UNIT: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));

impl<W: RawStream + AsLockedWrite> Printer<W> {
    /// Prints help on config params in the provided `schema`. Params can be filtered by the supplied predicate.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors.
    pub fn print_help(
        self,
        schema: &ConfigSchema,
        param_filter: impl Fn(&ParamMetadata) -> bool,
    ) -> io::Result<()> {
        let mut writer = self.writer;
        for config_ref in schema.iter() {
            let filtered_params: Vec<_> = config_ref
                .metadata()
                .params
                .iter()
                .filter(|&param| param_filter(param))
                .collect();
            if filtered_params.is_empty() {
                continue;
            }

            writeln!(&mut writer, "{}\n", config_ref.metadata().help)?;
            for param in filtered_params {
                write_parameter(&mut writer, config_ref, param)?;
                writeln!(&mut writer)?;
            }
        }
        Ok(())
    }
}

fn write_parameter(
    writer: &mut impl io::Write,
    config_ref: ConfigRef<'_>,
    param: &ParamMetadata,
) -> io::Result<()> {
    let all_names = all_names(&config_ref, param);
    let mut main_name = true;
    for (prefix, name) in all_names {
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

    let kind = param.expecting;
    let ty = format!(
        "{kind} {DIMMED}[Rust: {}]{DIMMED:#}",
        param.rust_type.name_in_code()
    );
    let qualifiers = param.deserializer.type_qualifiers();
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

    if let Some(default) = param.default_value() {
        writeln!(
            writer,
            "{INDENT}{FIELD}Default{FIELD:#}: {DEFAULT_VAL}{default:?}{DEFAULT_VAL:#}"
        )?;
    }

    if !param.help.is_empty() {
        for line in param.help.lines() {
            writeln!(writer, "{INDENT}{line}")?;
        }
    }
    Ok(())
}

fn all_names<'a>(
    config_ref: &'a ConfigRef<'_>,
    param: &'a ParamMetadata,
) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
    let local_names = iter::once(param.name).chain(param.aliases.iter().copied());
    let local_names_ = local_names.clone();
    let global_aliases = config_ref
        .aliases()
        .flat_map(move |alias| local_names_.clone().map(move |name| (alias, name)));
    let local_aliases = local_names
        .clone()
        .map(move |name| (config_ref.prefix(), name));
    local_aliases.chain(global_aliases)
}
