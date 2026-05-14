use std::{collections::BTreeMap, io, io::Write as _};

use anstream::stream::{AsLockedWrite, RawStream};
use smart_config::{
    ConfigRef, ConfigSchema,
    metadata::{BasicTypes, ConfigTag, ConfigVariant, TypeDescription, TypeSuffixes},
    pat::PatternDisplay,
};

use crate::{ParamRef, Printer, schema_ref::collect_conditions};

/// Options controlling Markdown reference generation.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
#[non_exhaustive]
pub struct MarkdownOptions {
    /// Optional top-level title to emit before the generated reference.
    pub title: Option<String>,
    /// Markdown heading level for [`Self::title`]. Nested headings are derived from this level.
    pub heading_level: u8,
    /// Whether to include a generated table of contents.
    pub include_table_of_contents: bool,
    /// Whether to include Rust type names in the generated reference.
    pub include_rust_types: bool,
    /// Whether to include config and param aliases.
    pub include_aliases: bool,
    /// Whether to include environment variable names for canonical param paths.
    pub include_env_vars: Option<EnvVarOptions>,
    /// Whether to include example values.
    pub include_examples: bool,
    /// Whether to mark secret params and child values.
    pub include_secret_marker: bool,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            title: Some("Configuration Reference".to_owned()),
            heading_level: 1,
            include_table_of_contents: false,
            include_rust_types: true,
            include_aliases: true,
            include_env_vars: None,
            include_examples: true,
            include_secret_marker: true,
        }
    }
}

/// Options for rendering environment variable names.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct EnvVarOptions {
    /// Prefix prepended to generated env var names, e.g. `APP_`.
    pub prefix: String,
}

#[derive(Debug)]
struct MarkdownDoc<'a> {
    config: ConfigRef<'a>,
    conditions: Vec<(ParamRef<'a>, &'a ConfigVariant)>,
    params: Vec<ParamRef<'a>>,
}

impl<W: RawStream + AsLockedWrite> Printer<W> {
    /// Prints a Markdown reference for config params in the provided `schema`.
    ///
    /// Params can be filtered by the supplied predicate. Enum tag params are rendered together with
    /// other matching params in the same config so that conditional fields have useful context.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors.
    pub fn print_markdown_reference(
        self,
        schema: &ConfigSchema,
        options: &MarkdownOptions,
        mut filter: impl FnMut(ParamRef<'_>) -> bool,
    ) -> io::Result<()> {
        let docs = collect_markdown_docs(schema, &mut filter);
        let mut writer = self.writer;
        write_title(&mut writer, options)?;
        if options.include_table_of_contents {
            write_table_of_contents(&mut writer, options, &docs)?;
        }

        for (i, doc) in docs.iter().enumerate() {
            if i > 0 || options.title.is_some() || options.include_table_of_contents {
                writeln!(writer)?;
            }
            write_config_reference(&mut writer, options, doc)?;
        }
        Ok(())
    }
}

fn collect_markdown_docs<'a>(
    schema: &'a ConfigSchema,
    filter: &mut impl FnMut(ParamRef<'_>) -> bool,
) -> Vec<MarkdownDoc<'a>> {
    schema
        .iter()
        .filter_map(|config| {
            let conditions = collect_conditions(config);
            let params: Vec<_> = config
                .metadata()
                .params
                .iter()
                .map(|param| ParamRef { config, param })
                .filter(|&param_ref| filter(param_ref))
                .collect();
            (!params.is_empty()).then_some(MarkdownDoc {
                config,
                conditions,
                params,
            })
        })
        .collect()
}

fn write_title(writer: &mut impl io::Write, options: &MarkdownOptions) -> io::Result<()> {
    if let Some(title) = &options.title {
        writeln!(
            writer,
            "{} {}",
            heading_marker(options.heading_level),
            escape_heading(title)
        )?;
    }
    Ok(())
}

fn write_table_of_contents(
    writer: &mut impl io::Write,
    options: &MarkdownOptions,
    docs: &[MarkdownDoc<'_>],
) -> io::Result<()> {
    if options.title.is_some() {
        writeln!(writer)?;
    }
    writeln!(
        writer,
        "{} Contents",
        heading_marker(options.heading_level + 1)
    )?;
    let mut anchors = BTreeMap::new();
    for doc in docs {
        let config_name = config_heading_text(doc.config);
        let anchor = unique_anchor(slugify_heading(&config_name), &mut anchors);
        writeln!(writer, "- [{config_name}](#{anchor})")?;
        if let Some(tag) = doc.config.metadata().tag {
            let tag_ref = ParamRef {
                config: doc.config,
                param: tag.param,
            };
            let tag_path = tag_ref.canonical_path();
            let anchor = unique_anchor(slugify_heading(&tag_path), &mut anchors);
            writeln!(writer, "  - [`{tag_path}`](#{anchor})")?;
        }
        for param_ref in filtered_params_without_tag(doc) {
            let path = param_ref.canonical_path();
            let anchor = unique_anchor(slugify_heading(&path), &mut anchors);
            writeln!(writer, "  - [`{path}`](#{anchor})")?;
        }
    }
    Ok(())
}

fn write_config_reference(
    writer: &mut impl io::Write,
    options: &MarkdownOptions,
    doc: &MarkdownDoc<'_>,
) -> io::Result<()> {
    let config = doc.config;
    writeln!(
        writer,
        "{} {}",
        heading_marker(options.heading_level + 1),
        code_span(&config_heading_text(config))
    )?;

    if !config.metadata().help.is_empty() {
        write_paragraph(writer, config.metadata().help)?;
    }

    let mut rows = vec![];
    if options.include_rust_types {
        rows.push((
            "Rust config".to_owned(),
            code_span(config.metadata().ty.name_in_code()),
        ));
    }
    if options.include_aliases {
        let aliases = format_aliases(config.aliases());
        if !aliases.is_empty() {
            rows.push(("Aliases".to_owned(), aliases));
        }
    }
    if !config.metadata().validations.is_empty() {
        let validations = config
            .metadata()
            .validations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("<br />");
        rows.push(("Validations".to_owned(), validations));
    }
    write_table(writer, &rows)?;

    if let Some(tag) = config.metadata().tag {
        writeln!(writer)?;
        write_tag_reference(writer, options, config, &tag, &doc.conditions)?;
    }

    for param_ref in filtered_params_without_tag(doc) {
        writeln!(writer)?;
        write_param_reference(writer, options, param_ref, &doc.conditions)?;
    }
    Ok(())
}

fn write_tag_reference(
    writer: &mut impl io::Write,
    options: &MarkdownOptions,
    config: ConfigRef<'_>,
    tag: &ConfigTag,
    conditions: &[(ParamRef<'_>, &ConfigVariant)],
) -> io::Result<()> {
    let tag_ref = ParamRef {
        config,
        param: tag.param,
    };
    writeln!(
        writer,
        "{} {}",
        heading_marker(options.heading_level + 2),
        code_span(&tag_ref.canonical_path())
    )?;

    let mut rows = vec![("Type".to_owned(), "string tag with variants".to_owned())];
    extend_location_rows(&mut rows, options, tag_ref);
    extend_condition_rows(&mut rows, conditions.iter().rev().copied());
    write_table(writer, &rows)?;

    writeln!(writer)?;
    writeln!(
        writer,
        "| Value | Rust variant | Default | Aliases | Description |"
    )?;
    writeln!(writer, "| --- | --- | --- | --- | --- |")?;
    for variant in tag.variants {
        let is_default = tag
            .default_variant
            .is_some_and(|default| default.rust_name == variant.rust_name);
        let rust_variant = format!(
            "{}::{}",
            config.metadata().ty.name_in_code(),
            variant.rust_name
        );
        let aliases = variant
            .aliases
            .iter()
            .map(|alias| code_span(alias))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            writer,
            "| {} | {} | {} | {} | {} |",
            table_cell(&code_span(variant.name)),
            table_cell(&code_span(&rust_variant)),
            if is_default { "yes" } else { "" },
            table_cell(&aliases),
            table_cell(variant.help)
        )?;
    }
    Ok(())
}

fn write_param_reference(
    writer: &mut impl io::Write,
    options: &MarkdownOptions,
    param_ref: ParamRef<'_>,
    conditions: &[(ParamRef<'_>, &ConfigVariant)],
) -> io::Result<()> {
    writeln!(
        writer,
        "{} {}",
        heading_marker(options.heading_level + 2),
        code_span(&param_ref.canonical_path())
    )?;

    let description = param_ref.param.type_description();
    let mut rows = vec![];
    extend_type_rows(
        &mut rows,
        "Type",
        param_ref.param.expecting,
        &description,
        options,
        true,
    );
    extend_location_rows(&mut rows, options, param_ref);

    let full_conditions = conditions.iter().rev().copied().chain(
        param_ref
            .param
            .tag_variant
            .map(|variant| (ParamRef::for_tag(param_ref.config), variant)),
    );
    extend_condition_rows(&mut rows, full_conditions);

    let mut value_blocks = vec![];
    let default = param_ref.param.default_value_json();
    if let Some(default) = &default {
        let value = markdown_json_value(default)?;
        rows.push(("Default".to_owned(), value.table_value));
        if let Some(block) = value.block {
            value_blocks.push(("Default", block));
        }
    }

    let example = param_ref
        .param
        .example_value_json()
        .filter(|val| Some(val) != default.as_ref());
    if options.include_examples
        && let Some(example) = example
    {
        let value = markdown_json_value(&example)?;
        rows.push(("Example".to_owned(), value.table_value));
        if let Some(block) = value.block {
            value_blocks.push(("Example", block));
        }
    }

    if let Some(fallback) = param_ref.param.fallback {
        rows.push((
            "Fallbacks".to_owned(),
            fallback.to_string().replace('\n', "<br />"),
        ));
    }
    write_table(writer, &rows)?;

    for (title, block) in value_blocks {
        writeln!(writer)?;
        writeln!(writer, "**{title}:**")?;
        writeln!(writer)?;
        writeln!(writer, "```json")?;
        writeln!(writer, "{block}")?;
        writeln!(writer, "```")?;
    }

    if !param_ref.param.help.is_empty() {
        write_paragraph(writer, param_ref.param.help)?;
    }
    Ok(())
}

fn filtered_params_without_tag<'a>(doc: &'a MarkdownDoc<'a>) -> impl Iterator<Item = ParamRef<'a>> {
    let tag_field = doc
        .config
        .metadata()
        .tag
        .map(|tag| tag.param.rust_field_name);
    doc.params
        .iter()
        .copied()
        .filter(move |param| Some(param.param.rust_field_name) != tag_field)
}

fn extend_type_rows(
    rows: &mut Vec<(String, String)>,
    field: &str,
    expecting: BasicTypes,
    description: &TypeDescription,
    options: &MarkdownOptions,
    is_top_level: bool,
) {
    let mut value = String::new();
    if options.include_secret_marker && description.contains_secrets() {
        value.push_str("secret ");
    }
    value.push_str(&code_span(&expecting.to_string()));
    if !is_top_level && options.include_rust_types && !description.rust_type().is_empty() {
        value.push_str("; Rust: ");
        value.push_str(&code_span(description.rust_type()));
    }
    if let Some(details) = description.details() {
        value.push_str("; ");
        value.push_str(details);
    }
    if let Some(unit) = description.unit() {
        value.push_str("; unit: ");
        value.push_str(&code_span(&unit.to_string()));
    }
    rows.push((field.to_owned(), value));

    if is_top_level && options.include_rust_types && !description.rust_type().is_empty() {
        rows.push(("Rust type".to_owned(), code_span(description.rust_type())));
    }
    if is_top_level && let Some(suffixes) = description.suffixes().and_then(format_suffixes) {
        rows.push(("Name suffixes".to_owned(), suffixes));
    }
    if !description.validations().is_empty() {
        rows.push((
            format!("{field} validations"),
            description.validations().join("<br />"),
        ));
    }
    if let Some(condition) = description.deserialize_if() {
        rows.push((
            format!("{field} filtering"),
            format!("{condition}, otherwise set to {}", code_span("null")),
        ));
    }

    if let Some((expecting, item)) = description.items() {
        extend_type_rows(rows, "Array items", expecting, item, options, false);
    }
    if let Some(separator) = description.item_separator() {
        rows.push(("Item separator".to_owned(), format_separator(separator)));
    }

    if let Some((expecting, key)) = description.keys() {
        extend_type_rows(rows, "Map keys", expecting, key, options, false);
    }
    if let Some((expecting, value)) = description.values() {
        extend_type_rows(rows, "Map values", expecting, value, options, false);
    }
    if let Some((entry_sep, kv_sep)) = description.entry_separators() {
        rows.push(("Entries separator".to_owned(), format_separator(entry_sep)));
        rows.push(("Key-value separator".to_owned(), format_separator(kv_sep)));
    }

    if let Some((expecting, fallback)) = description.fallback() {
        extend_type_rows(rows, "Fallback", expecting, fallback, options, false);
    }
}

fn extend_location_rows(
    rows: &mut Vec<(String, String)>,
    options: &MarkdownOptions,
    param_ref: ParamRef<'_>,
) {
    if options.include_aliases {
        let aliases = format_aliases(param_ref.all_paths().skip(1));
        if !aliases.is_empty() {
            rows.push(("Aliases".to_owned(), aliases));
        }
    }
    if let Some(env_options) = &options.include_env_vars {
        rows.push((
            "Environment variable".to_owned(),
            code_span(&env_var_name(
                &env_options.prefix,
                &param_ref.canonical_path(),
            )),
        ));
    }
}

fn extend_condition_rows<'a>(
    rows: &mut Vec<(String, String)>,
    conditions: impl Iterator<Item = (ParamRef<'a>, &'a ConfigVariant)>,
) {
    let conditions: Vec<_> = conditions
        .map(|(tag_ref, variant)| {
            format!(
                "{} == {}",
                code_span(&tag_ref.canonical_path()),
                code_span(variant.name)
            )
        })
        .collect();
    if !conditions.is_empty() {
        let field = if conditions.len() == 1 { "Tag" } else { "Tags" };
        rows.push((field.to_owned(), conditions.join(" && ")));
    }
}

fn format_aliases<A: AsRef<str>>(
    aliases: impl Iterator<Item = (A, smart_config::metadata::AliasOptions)>,
) -> String {
    aliases
        .map(|(alias, options)| {
            let mut rendered = code_span(alias.as_ref());
            if options.is_deprecated {
                rendered.push_str(" (deprecated)");
            }
            rendered
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_suffixes(suffixes: TypeSuffixes) -> Option<String> {
    match suffixes {
        TypeSuffixes::DurationUnits => Some(format!(
            "duration units from millis to weeks, for example {} or {}",
            code_span("_ms"),
            code_span("_in_sec")
        )),
        TypeSuffixes::SizeUnits => Some(format!(
            "byte size units up to gigabytes, for example {} or {}",
            code_span("_mb"),
            code_span("_in_kib")
        )),
        TypeSuffixes::EtherUnits => Some(format!(
            "ether value units, for example {} or {}",
            code_span("_gwei"),
            code_span("_in_ether")
        )),
        _ => None,
    }
}

fn format_separator(separator: &PatternDisplay) -> String {
    match separator {
        PatternDisplay::Exact(s) => format!("exact match: {}", code_span(&format!("{s:?}"))),
        PatternDisplay::Regex(regex) => format!("regex: {}", code_span(regex)),
        PatternDisplay::Generic(display) => display.clone(),
        _ => separator.to_string(),
    }
}

#[derive(Debug)]
struct MarkdownJsonValue {
    table_value: String,
    block: Option<String>,
}

fn markdown_json_value(value: &serde_json::Value) -> io::Result<MarkdownJsonValue> {
    if value.is_array() || value.is_object() {
        Ok(MarkdownJsonValue {
            table_value: "See below.".to_owned(),
            block: Some(serde_json::to_string_pretty(value).map_err(io::Error::other)?),
        })
    } else {
        Ok(MarkdownJsonValue {
            table_value: code_span(&serde_json::to_string(value).map_err(io::Error::other)?),
            block: None,
        })
    }
}

fn write_table(writer: &mut impl io::Write, rows: &[(String, String)]) -> io::Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    writeln!(writer)?;
    writeln!(writer, "| Field | Value |")?;
    writeln!(writer, "| --- | --- |")?;
    for (field, value) in rows {
        writeln!(writer, "| {} | {} |", table_cell(field), table_cell(value))?;
    }
    Ok(())
}

fn write_paragraph(writer: &mut impl io::Write, paragraph: &str) -> io::Result<()> {
    writeln!(writer)?;
    for line in paragraph.lines() {
        writeln!(writer, "{line}")?;
    }
    Ok(())
}

fn heading_marker(level: u8) -> String {
    "#".repeat(usize::from(level.clamp(1, 6)))
}

fn config_heading_text(config: ConfigRef<'_>) -> String {
    if config.prefix().is_empty() {
        "root".to_owned()
    } else {
        config.prefix().to_owned()
    }
}

fn env_var_name(prefix: &str, path: &str) -> String {
    let mut var_name = path.replace('.', "_").to_uppercase();
    var_name.insert_str(0, prefix);
    var_name
}

fn table_cell(value: &str) -> String {
    value
        .replace('\\', r"\\")
        .replace('|', r"\|")
        .replace('\n', "<br />")
}

fn escape_heading(value: &str) -> String {
    value.replace('\n', " ")
}

fn code_span(value: &str) -> String {
    if value.is_empty() {
        return "``".to_owned();
    }

    let max_backticks = value.split(|ch| ch != '`').map(str::len).max().unwrap_or(0);
    let ticks = "`".repeat(max_backticks + 1);
    if max_backticks == 0 {
        format!("{ticks}{value}{ticks}")
    } else {
        format!("{ticks} {value} {ticks}")
    }
}

fn slugify_heading(value: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            prev_dash = false;
        } else if !prev_dash && !slug.is_empty() {
            slug.push('-');
            prev_dash = true;
        }
    }
    if slug.ends_with('-') {
        slug.pop();
    }
    slug
}

fn unique_anchor(slug: String, anchors: &mut BTreeMap<String, usize>) -> String {
    let count = anchors.entry(slug.clone()).or_default();
    let anchor = if *count == 0 {
        slug
    } else {
        format!("{slug}-{count}")
    };
    *count += 1;
    anchor
}
