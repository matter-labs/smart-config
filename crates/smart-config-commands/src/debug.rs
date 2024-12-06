use std::io::{self, Write as _};

use anstream::stream::{AsLockedWrite, RawStream};
use anstyle::{AnsiColor, Color, Style};
use smart_config::{
    value::{FileFormat, Value, ValueOrigin, WithOrigin},
    ConfigRepository, ParseError, ParseErrors,
};

use crate::Printer;

const SECTION: Style = Style::new().bold();
const ARROW: Style = Style::new().bold();
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
    pub fn print_debug(self, repo: &ConfigRepository) -> io::Result<()> {
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

        writeln!(&mut writer)?;
        writeln!(&mut writer, "{SECTION}Values:{SECTION:#}")?;

        let merged = repo.merged();
        for config_parser in repo.iter() {
            let config_ref = config_parser.config();
            for (i, param) in config_ref.metadata().params.iter().enumerate() {
                let param_path = format!("{}.{}", config_ref.prefix(), param.name);
                if let Some(value) = merged.pointer(&param_path) {
                    write_param(&mut writer, &param_path, value)?;
                    if let Err(err) = config_parser.parse_param(i) {
                        write_de_errors(&mut writer, &err)?;
                    }
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

fn write_param(writer: &mut impl io::Write, path: &str, value: &WithOrigin) -> io::Result<()> {
    write!(writer, "{path} = ")?;
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
    const OBJECT_KEY: Style = Style::new().bold();

    match &value.inner {
        Value::Null => write!(writer, "{NULL}null{NULL:#}"),
        Value::Bool(val) => write!(writer, "{BOOL}{val:?}{BOOL:#}"),
        Value::Number(val) => write!(writer, "{NUMBER}{val}{NUMBER:#}"),
        Value::String(val) => write!(writer, "{STRING}{val:?}{STRING:#}"),
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

fn write_de_errors(writer: &mut impl io::Write, errors: &ParseErrors) -> io::Result<()> {
    if errors.len() == 1 {
        write!(writer, "  {ERROR_LABEL}Error:{ERROR_LABEL:#} ")?;
        write_de_error(writer, errors.first())
    } else {
        writeln!(writer, "  {ERROR_LABEL}Errors:{ERROR_LABEL:#}")?;
        for err in errors.iter() {
            write!(writer, "  - ")?;
            write_de_error(writer, err)?;
        }
        Ok(())
    }
}

fn write_de_error(writer: &mut impl io::Write, err: &ParseError) -> io::Result<()> {
    writeln!(writer, "{}", err.inner())?;

    let maybe_param = if let Some(param) = err.param() {
        format!(".{}", param.rust_field_name)
    } else {
        String::new()
    };
    writeln!(
        writer,
        "    at {SECTION}{path}{SECTION:#}, {config}{maybe_param}",
        path = err.path(),
        config = err.config().ty.name_in_code()
    )?;
    write!(writer, "    ")?;

    write_origin(writer, err.origin())?;
    writeln!(writer)
}
