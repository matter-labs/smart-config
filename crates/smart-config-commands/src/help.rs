use std::{io, io::Write as _};

use anstream::stream::{AsLockedWrite, RawStream};
use anstyle::{AnsiColor, Color, Style};
use smart_config::{
    metadata::{BasicTypes, ConfigTag, ConfigVariant, TypeDescription, TypeSuffixes},
    ConfigRef, ConfigSchema,
};

use crate::{
    utils::{write_json_value, NULL, STRING},
    ParamRef, Printer, CONFIG_PATH,
};

const INDENT: &str = "  ";
const DIMMED: Style = Style::new().dimmed();
const MAIN_NAME: Style = Style::new().bold();
const DEPRECATED: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));
const DEFAULT_VARIANT: Style = Style::new().bold();
const FIELD: Style = Style::new().underline();
const UNIT: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
const SECRET: Style = Style::new()
    .bg_color(Some(Color::Ansi(AnsiColor::Cyan)))
    .fg_color(None);

fn collect_conditions(mut config: ConfigRef<'_>) -> Vec<(ParamRef<'_>, &ConfigVariant)> {
    let mut conditions = vec![];
    while let Some((parent_ref, this_ref)) = config.parent_link() {
        if let Some(variant) = this_ref.tag_variant {
            conditions.push((ParamRef::for_tag(parent_ref), variant));
        }
        config = parent_ref;
    }
    conditions
}

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
            let conditions = collect_conditions(config);

            let mut filtered_params: Vec<_> = config
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
                writeln!(&mut writer)?;
            }

            if let Some(tag) = &config.metadata().tag {
                write_tag_help(&mut writer, config, tag, &conditions)?;
                // Do not output the tag param twice.
                filtered_params
                    .retain(|param| param.param.rust_field_name != tag.param.rust_field_name);
                writeln!(&mut writer)?;
            }

            for param_ref in filtered_params {
                param_ref.write_help(&mut writer, &conditions)?;
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
    for (alias, options) in config.aliases() {
        let config_style = if options.is_deprecated {
            CONFIG_PATH.strikethrough()
        } else {
            CONFIG_PATH
        };
        write!(writer, "{config_style}{alias}{config_style:#}")?;
        if options.is_deprecated {
            writeln!(writer, " {DEPRECATED}[deprecated alias]{DEPRECATED:#}")?;
        } else {
            writeln!(writer)?;
        }
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

fn write_tag_help(
    writer: &mut impl io::Write,
    config: ConfigRef<'_>,
    tag: &ConfigTag,
    conditions: &[(ParamRef<'_>, &ConfigVariant)],
) -> io::Result<()> {
    ParamRef {
        config,
        param: tag.param,
    }
    .write_locations(writer)?;
    writeln!(
        writer,
        "{INDENT}{FIELD}Type{FIELD:#}: string tag with variants:"
    )?;

    let default_variant_name = tag.default_variant.map(|variant| variant.rust_name);

    for variant in tag.variants {
        let default_marker = if default_variant_name == Some(variant.rust_name) {
            format!(" {DEFAULT_VARIANT}(default){DEFAULT_VARIANT:#}")
        } else {
            String::new()
        };

        writeln!(
            writer, "{INDENT}- {STRING}'{name}'{STRING:#} {DIMMED}[Rust: {config_name}::{rust_name}]{DIMMED:#}{default_marker}",
            name = variant.name,
            config_name = config.metadata().ty.name_in_code(),
            rust_name = variant.rust_name
        )?;
        if !variant.aliases.is_empty() {
            write!(writer, "{INDENT}  {FIELD}Aliases{FIELD:#}: ")?;
            for (i, &alias) in variant.aliases.iter().enumerate() {
                write!(writer, "{STRING}'{alias}'{STRING:#}")?;
                if i + 1 < variant.aliases.len() {
                    write!(writer, ", ")?;
                }
            }
            writeln!(writer)?;
        }

        if !variant.help.is_empty() {
            for line in variant.help.lines() {
                writeln!(writer, "{INDENT}  {line}")?;
            }
        }
    }

    let condition_count = conditions.len();
    ParamRef::write_tag_conditions(writer, condition_count, conditions.iter().copied())
}

impl ParamRef<'_> {
    fn write_locations(&self, writer: &mut impl io::Write) -> io::Result<()> {
        let all_paths = self.all_paths();
        let mut main_name = true;
        for (path, options) in all_paths {
            let (prefix, name) = path.rsplit_once('.').unwrap_or(("", &path));
            let prefix_sep = if prefix.is_empty() || prefix.ends_with('.') {
                ""
            } else {
                "."
            };
            let name_style = if main_name {
                MAIN_NAME
            } else if options.is_deprecated {
                Style::new().strikethrough()
            } else {
                Style::new()
            };
            main_name = false;
            write!(
                writer,
                "{DIMMED}{prefix}{prefix_sep}{DIMMED:#}{name_style}{name}{name_style:#}"
            )?;

            if options.is_deprecated {
                writeln!(writer, " {DEPRECATED}[deprecated alias]{DEPRECATED:#}")?;
            } else {
                writeln!(writer)?;
            }
        }
        Ok(())
    }

    fn write_help(
        &self,
        writer: &mut impl io::Write,
        conditions: &[(ParamRef<'_>, &ConfigVariant)],
    ) -> io::Result<()> {
        self.write_locations(writer)?;
        let description = self.param.type_description();
        write_type_description(writer, None, 2, self.param.expecting, &description)?;

        // `conditions` are ordered from most specific to least specific; we want the reverse ordering.
        let full_conditions = conditions.iter().rev().copied().chain(
            self.param
                .tag_variant
                .map(|variant| (ParamRef::for_tag(self.config), variant)),
        );
        let condition_count = conditions.len() + usize::from(self.param.tag_variant.is_some());
        Self::write_tag_conditions(writer, condition_count, full_conditions)?;

        let default = self.param.default_value_json();
        if let Some(default) = &default {
            write!(writer, "{INDENT}{FIELD}Default{FIELD:#}: ")?;
            write_json_value(writer, default, 2)?;
            writeln!(writer)?;
        }

        let example = self
            .param
            .example_value_json()
            .filter(|val| Some(val) != default.as_ref());
        if let Some(example) = example {
            write!(writer, "{INDENT}{FIELD}Example{FIELD:#}: ")?;
            write_json_value(writer, &example, 2)?;
            writeln!(writer)?;
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

    fn write_tag_conditions<'a>(
        writer: &mut impl io::Write,
        condition_count: usize,
        conditions: impl Iterator<Item = (ParamRef<'a>, &'a ConfigVariant)>,
    ) -> io::Result<()> {
        if condition_count == 0 {
            return Ok(());
        }

        let tag_field = if condition_count == 1 { "Tag" } else { "Tags" };
        write!(writer, "{INDENT}{FIELD}{tag_field}{FIELD:#}: ")?;
        for (i, (tag_ref, variant)) in conditions.enumerate() {
            let tag_name = tag_ref.canonical_path();
            let variant = variant.name;
            write!(writer, "{tag_name} == {STRING}'{variant}'{STRING:#}")?;
            if i + 1 < condition_count {
                write!(writer, " && ")?;
            }
        }
        writeln!(writer)
    }
}

fn write_type_description(
    writer: &mut impl io::Write,
    relation_to_parent: Option<&str>,
    indent: usize,
    expecting: BasicTypes,
    description: &TypeDescription,
) -> io::Result<()> {
    let maybe_secret = if description.contains_secrets() {
        format!("{SECRET}secret{SECRET:#} ")
    } else {
        String::new()
    };
    let rust_type = description.rust_type();
    let rust_type = if rust_type.is_empty() {
        String::new()
    } else {
        format!(" {DIMMED}[Rust: {rust_type}]{DIMMED:#}")
    };
    let ty = format!("{maybe_secret}{expecting}{rust_type}");

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

    let field_name = relation_to_parent.unwrap_or("Type");
    writeln!(
        writer,
        "{:>indent$}{FIELD}{field_name}{FIELD:#}: {ty}{details}{unit}",
        ""
    )?;

    // Suffixes are only active for top-level types, not for array items etc.
    if let (None, Some(suffixes)) = (relation_to_parent, description.suffixes()) {
        let suffixes = match suffixes {
            TypeSuffixes::DurationUnits => {
                Some(format!("duration units from millis to weeks, e.g. {STRING}_ms{STRING:#} or {STRING}_in_sec{STRING:#}"))
            }
            TypeSuffixes::SizeUnits => {
                Some(format!("byte suze units up to gigabytes, e.g. {STRING}_mb{STRING:#} or {STRING}_in_kib{STRING:#}"))
            }
            _ => None,
        };
        if let Some(suffixes) = &suffixes {
            writeln!(
                writer,
                "{:>indent$}{FIELD}Name suffixes{FIELD:#}: {suffixes}",
                ""
            )?;
        }
    }

    let validations = description.validations();
    if !validations.is_empty() {
        writeln!(writer, "{:>indent$}{FIELD}Validations{FIELD:#}:", "")?;
        for validation in validations {
            writeln!(writer, "{:>indent$}- {validation}", "")?;
        }
    }

    if let Some(condition) = description.deserialize_if() {
        writeln!(
            writer,
            "{:>indent$}{FIELD}Filtering{FIELD:#}: {condition}, otherwise set to {NULL}null{NULL:#}",
            ""
        )?;
    }

    if let Some((expecting, item)) = description.items() {
        write_type_description(writer, Some("Array items"), indent + 2, expecting, item)?;
    }
    if let Some((expecting, key)) = description.keys() {
        write_type_description(writer, Some("Map keys"), indent + 2, expecting, key)?;
    }
    if let Some((expecting, value)) = description.values() {
        write_type_description(writer, Some("Map values"), indent + 2, expecting, value)?;
    }
    if let Some((expecting, fallback)) = description.fallback() {
        write_type_description(writer, Some("Fallback"), indent + 2, expecting, fallback)?;
    }

    Ok(())
}
