use std::{
    any,
    collections::{HashMap, HashSet},
    io::{self, Write as _},
};

use anstream::stream::{AsLockedWrite, RawStream};
use anstyle::{AnsiColor, Color, Style};
use smart_config::{
    metadata::ConfigMetadata,
    value::{FileFormat, ValueOrigin, WithOrigin},
    visit::{ConfigVisitor, VisitConfig},
    ConfigRepository, ParseError, ParseErrors,
};

use crate::{
    utils::{write_json_value, write_value, STRING},
    ParamRef, Printer, CONFIG_PATH,
};

const SECTION: Style = Style::new().bold();
const ARROW: Style = Style::new().bold();
const INACTIVE: Style = Style::new().italic();
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

#[derive(Debug)]
struct ParamValuesVisitor {
    config: &'static ConfigMetadata,
    variant: Option<usize>,
    param_values: HashMap<usize, serde_json::Value>,
}

impl ParamValuesVisitor {
    fn new(config: &'static ConfigMetadata) -> Self {
        Self {
            config,
            variant: None,
            param_values: HashMap::new(),
        }
    }
}

impl ConfigVisitor for ParamValuesVisitor {
    fn visit_tag(&mut self, variant_index: usize) {
        self.variant = Some(variant_index);
    }

    fn visit_param(&mut self, param_index: usize, value: &dyn any::Any) {
        let param = self.config.params[param_index];
        let json = if param.type_description().contains_secrets() {
            "[REDACTED]".into()
        } else {
            param.deserializer.serialize_param(value)
        };
        self.param_values.insert(param_index, json);
    }

    fn visit_nested_config(&mut self, _config_index: usize, _config: &dyn VisitConfig) {
        // Don't recurse into nested configs, we debug them separately
    }
}

/// Type ID of the config + path to the config / param.
type ErrorKey = (any::TypeId, String);

#[derive(Debug)]
struct ConfigErrors {
    by_param: HashMap<ErrorKey, Vec<ParseError>>,
    by_config: HashMap<ErrorKey, Vec<ParseError>>,
}

impl ConfigErrors {
    fn new(repo: &ConfigRepository<'_>) -> Self {
        let mut by_param = HashMap::<_, Vec<_>>::new();
        let mut by_config = HashMap::<_, Vec<_>>::new();
        for config_parser in repo.iter() {
            if !config_parser.config().is_top_level() {
                // The config should be parsed as a part of the parent config. Filtering out these configs
                // might not be sufficient to prevent error duplication because a parent config may be inserted after
                // the config itself, so we perform additional deduplication below.
                continue;
            }

            if let Err(errors) = config_parser.parse_opt() {
                // Only insert errors for a certain param / config if errors for it were not encountered before.
                let mut new_params = HashSet::new();
                let mut new_configs = HashSet::new();

                for err in errors {
                    let key = (err.config().ty.id(), err.path().to_owned());
                    if err.param().is_some() {
                        if !by_param.contains_key(&key) || new_params.contains(&key) {
                            by_param.entry(key.clone()).or_default().push(err);
                            new_params.insert(key);
                        }
                    } else if !by_config.contains_key(&key) || new_configs.contains(&key) {
                        by_config.entry(key.clone()).or_default().push(err);
                        new_configs.insert(key);
                    }
                }
            }
        }
        Self {
            by_param,
            by_config,
        }
    }
}

impl From<ConfigErrors> for Result<(), ParseErrors> {
    fn from(errors: ConfigErrors) -> Self {
        let errors = errors
            .by_config
            .into_values()
            .chain(errors.by_param.into_values())
            .flatten();
        errors.collect()
    }
}

impl<W: RawStream + AsLockedWrite> Printer<W> {
    /// Prints debug info for all param values in the provided `repo`. If params fail to deserialize,
    /// corresponding error(s) are output as well.
    ///
    /// # Errors
    ///
    /// - Propagates I/O errors.
    /// - Returns the exhaustive parsing result. Depending on the application, some parsing errors (e.g., missing params for optional configs)
    ///   may not be fatal.
    #[allow(clippy::missing_panics_doc)] // false positive
    pub fn print_debug(
        self,
        repo: &ConfigRepository<'_>,
        mut filter: impl FnMut(ParamRef<'_>) -> bool,
    ) -> io::Result<Result<(), ParseErrors>> {
        let mut writer = self.writer;
        if repo.sources().is_empty() {
            writeln!(&mut writer, "configuration is empty")?;
            return Ok(Ok(()));
        }

        writeln!(&mut writer, "{SECTION}Configuration sources:{SECTION:#}")?;
        for source in repo.sources() {
            write!(&mut writer, "- ")?;
            write_origin(&mut writer, &source.origin)?;
            writeln!(&mut writer, ", {} param(s)", source.param_count)?;
        }

        writeln!(&mut writer)?;
        writeln!(&mut writer, "{SECTION}Values:{SECTION:#}")?;

        let errors = ConfigErrors::new(repo);
        let merged = repo.merged();
        for config_parser in repo.iter() {
            let config = config_parser.config();
            let config_name = config.metadata().ty.name_in_code();
            let config_id = (config.metadata().ty.id(), config.prefix().to_owned());

            if let Some(errors) = errors.by_config.get(&config_id) {
                writeln!(
                    writer,
                    "{CONFIG_PATH}{}{CONFIG_PATH:#} {RUST}[Rust: {config_name}]{RUST:#}, config",
                    config.prefix()
                )?;
                write_de_errors(&mut writer, errors)?;
            }

            let (variant, mut param_values) =
                if let Ok(Some(boxed_config)) = config_parser.parse_opt() {
                    let visitor_fn = config.metadata().visitor;
                    let mut visitor = ParamValuesVisitor::new(config.metadata());
                    visitor_fn(boxed_config.as_ref(), &mut visitor);
                    (visitor.variant, visitor.param_values)
                } else {
                    (None, HashMap::new())
                };

            let variant = variant.map(|idx| {
                // `unwrap()` is safe by construction: if there's an active variant, the config must have a tag
                let tag = config.metadata().tag.unwrap();
                let name = tag.variants[idx].name;
                // Add the tag value, using the fact that the tag is always the last param in the config.
                let tag_param_idx = config.metadata().params.len() - 1;
                param_values.insert(tag_param_idx, name.into());

                ActiveTagVariant {
                    canonical_path: ParamRef {
                        config,
                        param: tag.param,
                    }
                    .canonical_path(),
                    name,
                }
            });

            for (param_idx, param) in config.metadata().params.iter().enumerate() {
                let param_ref = ParamRef { config, param };
                if !filter(param_ref) {
                    continue;
                }
                let canonical_path = param_ref.canonical_path();

                let raw_value = merged.pointer(&canonical_path);
                let param_value = param_values.get(&param_idx);
                let mut param_written = false;
                if param_value.is_some() || raw_value.is_some() {
                    write_param(
                        &mut writer,
                        param_ref,
                        &canonical_path,
                        param_value,
                        raw_value,
                        variant.as_ref(),
                    )?;
                    param_written = true;
                }

                let param_id = (config_id.0, canonical_path.clone());
                if let Some(errors) = errors.by_param.get(&param_id) {
                    if !param_written {
                        let field_name = param.rust_field_name;
                        writeln!(
                            writer,
                            "{canonical_path} {RUST}[Rust: {config_name}.{field_name}]{RUST:#}"
                        )?;
                    }
                    write_de_errors(&mut writer, errors)?;
                }
            }
        }
        Ok(errors.into())
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

#[derive(Debug)]
struct ActiveTagVariant {
    canonical_path: String,
    name: &'static str,
}

fn write_param(
    writer: &mut impl io::Write,
    param_ref: ParamRef<'_>,
    path: &str,
    visited_value: Option<&serde_json::Value>,
    raw_value: Option<&WithOrigin>,
    active_variant: Option<&ActiveTagVariant>,
) -> io::Result<()> {
    let activity_style = if visited_value.is_some() {
        Style::new()
    } else {
        INACTIVE
    };
    write!(
        writer,
        "{activity_style}{path}{activity_style:#} {RUST}[Rust: {}.{}]{RUST:#}",
        param_ref.config.metadata().ty.name_in_code(),
        param_ref.param.rust_field_name
    )?;

    if let Some(value) = visited_value {
        write!(writer, " = ")?;
        write_json_value(writer, value, 0)?;
        writeln!(writer)?;
    } else {
        writeln!(writer)?;
    }

    if let (Some(param_variant), Some(active_variant)) =
        (param_ref.param.tag_variant, active_variant)
    {
        let tag_path = &active_variant.canonical_path;
        let param_variant_name = param_variant.name;
        let (label, eq) = if param_variant_name == active_variant.name {
            ("Active", "==")
        } else {
            ("Inactive", "!=")
        };
        writeln!(
            writer,
            "  {label}: {tag_path} {eq} {STRING}'{param_variant_name}'{STRING:#}"
        )?;
    }

    if let Some(value) = raw_value {
        write!(writer, "  Raw: ")?;
        write_value(writer, value, 2)?;
        writeln!(writer)?;
        write!(writer, "  Origin: ")?;
        write_origin(writer, &value.origin)?;
        writeln!(writer)?;
    }
    Ok(())
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
    if let Some(validation) = err.validation() {
        writeln!(writer, "    {SECTION}validation:{SECTION:#} {validation}")?;
    }
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
