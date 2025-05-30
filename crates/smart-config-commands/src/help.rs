use std::{io, io::Write as _};

use anstream::stream::{AsLockedWrite, RawStream};
use anstyle::{AnsiColor, Color, Style};
use smart_config::{
    metadata::{BasicTypes, ConfigTag, ConfigVariant, TypeDescription},
    ConfigRef, ConfigSchema,
};

use crate::{
    utils::{write_json_value, STRING},
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
                write_tag_help(&mut writer, config, tag)?;
                // Do not output the tag param twice.
                filtered_params
                    .retain(|param| param.param.rust_field_name != tag.param.rust_field_name);
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
    Ok(())
}

impl ParamRef<'_> {
    fn write_locations(&self, writer: &mut impl io::Write) -> io::Result<()> {
        let all_paths = self.all_paths_inner();
        let mut main_name = true;
        for (prefix, name, options) in all_paths {
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

    fn write_help(&self, writer: &mut impl io::Write) -> io::Result<()> {
        self.write_locations(writer)?;
        let description = self.param.type_description();
        write_type_description(writer, "Type", 2, self.param.expecting, &description)?;

        if let Some(tag_variant) = self.param.tag_variant {
            self.write_tag_variant(tag_variant, writer)?;
        }

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

    fn write_tag_variant(
        &self,
        variant: &ConfigVariant,
        writer: &mut impl io::Write,
    ) -> io::Result<()> {
        let tag_ref = ParamRef {
            config: self.config,
            param: self.config.metadata().tag.unwrap().param,
        };
        let tag_name = tag_ref.canonical_path();
        let variant = variant.name;
        writeln!(
            writer,
            "{INDENT}{FIELD}Tag{FIELD:#}: {tag_name} == {STRING}'{variant}'{STRING:#}"
        )
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
    writeln!(
        writer,
        "{:>indent$}{FIELD}{relation_to_parent}{FIELD:#}: {ty}{details}{unit}",
        ""
    )?;

    if !description.validations().is_empty() {
        writeln!(writer, "{:>indent$}{FIELD}Validations{FIELD:#}:", "")?;
        for validation in description.validations() {
            writeln!(writer, "{:>indent$}- {validation}", "")?;
        }
    }

    if let Some((expecting, item)) = description.items() {
        write_type_description(writer, "Array items", indent + 2, expecting, item)?;
    }
    if let Some((expecting, key)) = description.keys() {
        write_type_description(writer, "Map keys", indent + 2, expecting, key)?;
    }
    if let Some((expecting, value)) = description.values() {
        write_type_description(writer, "Map values", indent + 2, expecting, value)?;
    }
    if let Some((expecting, fallback)) = description.fallback() {
        write_type_description(writer, "Fallback", indent + 2, expecting, fallback)?;
    }

    Ok(())
}
