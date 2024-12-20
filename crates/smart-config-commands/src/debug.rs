use std::{
    collections::{HashMap, HashSet},
    io::{self, Write as _},
};

use anstream::stream::{AsLockedWrite, RawStream};
use anstyle::{AnsiColor, Color, Style};
use smart_config::{
    value::{FileFormat, StrValue, Value, ValueOrigin, WithOrigin},
    ConfigRepository, ParseError,
};

use crate::{ParamRef, Printer, CONFIG_PATH};

const SECTION: Style = Style::new().bold();
const ARROW: Style = Style::new().bold();
const RUST: Style = Style::new().dimmed();
const JSON_FILE: Style = Style::new()
    .bg_color(Some(Color::Ansi(AnsiColor::Cyan)))
    .fg_color(None);
const YAML_FILE: Style = Style::new()
    .bg_color(Some(Color::Ansi(AnsiColor::Green)))
    .fg_color(None);
const DOTENV_FILE: Style = Style::new()
    .bg_color(Some(Color::Ansi(AnsiColor::Magenta)))
    .fg_color(None);
const ERROR_LABEL: Style = Style::new()
    .bold()
    .bg_color(Some(Color::Ansi(AnsiColor::Red)))
    .fg_color(None);

impl<W: RawStream + AsLockedWrite> Printer<W> {
    /// Prints debug info for all param values in the provided `repo`. If params fail to deserialize,
    /// corresponding error(s) are output as well.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors.
    pub fn print_debug(
        self,
        repo: &ConfigRepository,
        mut filter: impl FnMut(ParamRef<'_>) -> bool,
    ) -> io::Result<()> {
        let mut writer = self.writer;
        if repo.sources().is_empty() {
            writeln!(&mut writer, "configuration is empty")?;
            return Ok(());
        }

        writeln!(&mut writer, "{SECTION}Configuration sources:{SECTION:#}")?;
        for source in repo.sources() {
            write!(&mut writer, "- ")?;
            write_origin(&mut writer, &source.origin)?;
            writeln!(&mut writer, ", {} param(s)", source.param_count)?;
        }

        let mut errors_by_param = HashMap::<_, Vec<_>>::new();
        let mut errors_by_config = HashMap::<_, Vec<_>>::new();
        for config_parser in repo.iter() {
            if let Err(errors) = config_parser.parse() {
                // Only insert errors for a certain param / config if errors for it were not encountered before.
                let mut new_params = HashSet::new();
                let mut new_configs = HashSet::new();

                for err in errors {
                    let config_id = err.config().ty.id();
                    if let Some(param) = err.param() {
                        let key = (config_id, param.rust_field_name);
                        if !errors_by_param.contains_key(&key) || new_params.contains(&key) {
                            errors_by_param.entry(key).or_default().push(err);
                            new_params.insert(key);
                        }
                    } else if !errors_by_config.contains_key(&config_id)
                        || new_configs.contains(&config_id)
                    {
                        errors_by_config.entry(config_id).or_default().push(err);
                        new_configs.insert(config_id);
                    }
                }
            }
        }

        writeln!(&mut writer)?;
        writeln!(&mut writer, "{SECTION}Values:{SECTION:#}")?;

        let merged = repo.merged();
        for config_parser in repo.iter() {
            let config = config_parser.config();
            let config_name = config.metadata().ty.name_in_code();
            let config_id = config.metadata().ty.id();

            if let Some(errors) = errors_by_config.get(&config_id) {
                writeln!(
                    writer,
                    "{CONFIG_PATH}{}{CONFIG_PATH:#} {RUST}[Rust: {config_name}]{RUST:#}, config",
                    config.prefix()
                )?;
                write_de_errors(&mut writer, errors)?;
            }

            for param in config.metadata().params {
                let param_ref = ParamRef { config, param };
                if !filter(param_ref) {
                    continue;
                }
                let canonical_path = param_ref.canonical_path();

                let mut param_written = false;
                if let Some(value) = merged.pointer(&canonical_path) {
                    write_param(&mut writer, param_ref, &canonical_path, value)?;
                    param_written = true;
                }
                let field_name = param.rust_field_name;
                if let Some(errors) = errors_by_param.get(&(config_id, field_name)) {
                    if !param_written {
                        writeln!(
                            writer,
                            "{canonical_path} {RUST}[Rust: {config_name}.{field_name}]{RUST:#}"
                        )?;
                    }
                    write_de_errors(&mut writer, errors)?;
                }
            }
        }
        Ok(())
    }
}

fn write_origin(writer: &mut impl io::Write, origin: &ValueOrigin) -> io::Result<()> {
    match origin {
        ValueOrigin::EnvVars => {
            write!(writer, "{DOTENV_FILE}env{DOTENV_FILE:#}")
        }
        ValueOrigin::File { name, format } => {
            let style = match format {
                FileFormat::Json => JSON_FILE,
                FileFormat::Yaml => YAML_FILE,
                FileFormat::Dotenv => DOTENV_FILE,
                _ => Style::new(),
            };
            write!(writer, "{style}{format}:{style:#}{name}")
        }
        ValueOrigin::Path { source, path } => {
            if matches!(source.as_ref(), ValueOrigin::EnvVars) {
                write!(writer, "{DOTENV_FILE}env:{DOTENV_FILE:#}{path:?}")
            } else {
                write_origin(writer, source)?;
                if !path.is_empty() {
                    write!(writer, " {ARROW}->{ARROW:#} .{path}")?;
                }
                Ok(())
            }
        }
        ValueOrigin::Synthetic { source, transform } => {
            write_origin(writer, source)?;
            write!(writer, " {ARROW}->{ARROW:#} {transform}")
        }
        _ => write!(writer, "{origin}"),
    }
}

fn write_param(
    writer: &mut impl io::Write,
    param_ref: ParamRef<'_>,
    path: &str,
    value: &WithOrigin,
) -> io::Result<()> {
    write!(
        writer,
        "{path} {RUST}[Rust: {}.{}]{RUST:#} = ",
        param_ref.config.metadata().ty.name_in_code(),
        param_ref.param.rust_field_name
    )?;
    write_value(writer, value, 0)?;

    writeln!(writer)?;
    write!(writer, "  Origin: ")?;
    write_origin(writer, &value.origin)?;
    writeln!(writer)
}

fn write_value(writer: &mut impl io::Write, value: &WithOrigin, ident: usize) -> io::Result<()> {
    const NULL: Style = Style::new().bold();
    const BOOL: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
    const NUMBER: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
    const STRING: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
    const SECRET: Style = Style::new()
        .bg_color(Some(Color::Ansi(AnsiColor::Cyan)))
        .fg_color(None);
    const OBJECT_KEY: Style = Style::new().bold();

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

fn write_de_errors(writer: &mut impl io::Write, errors: &[ParseError]) -> io::Result<()> {
    if errors.len() == 1 {
        write!(writer, "  {ERROR_LABEL}Error:{ERROR_LABEL:#} ")?;
        write_de_error(writer, &errors[0])
    } else {
        writeln!(writer, "  {ERROR_LABEL}Errors:{ERROR_LABEL:#}")?;
        for err in errors {
            write!(writer, "  - ")?;
            write_de_error(writer, err)?;
        }
        Ok(())
    }
}

fn write_de_error(writer: &mut impl io::Write, err: &ParseError) -> io::Result<()> {
    writeln!(writer, "{}", err.inner())?;
    writeln!(
        writer,
        "    at {SECTION}{path}{SECTION:#}",
        path = err.path()
    )?;
    if !matches!(err.origin(), ValueOrigin::Unknown) {
        write!(writer, "    ")?;
        write_origin(writer, err.origin())?;
        writeln!(writer)?;
    }
    Ok(())
}
