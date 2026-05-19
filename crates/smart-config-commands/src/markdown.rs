use std::{collections::BTreeMap, io, io::Write as _};

use anstream::stream::{AsLockedWrite, RawStream};
use cmark_writer::{CommonMarkWriter, ListItem, Node};
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

#[derive(Debug)]
struct DetailDoc {
    label: String,
    summary: Vec<Node>,
    children: Vec<DetailDoc>,
}

impl DetailDoc {
    fn new(label: impl Into<String>, summary: Vec<Node>) -> Self {
        Self {
            label: label.into(),
            summary,
            children: vec![],
        }
    }
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
        let document = render_document(options, &docs)?;

        let mut markdown = CommonMarkWriter::new();
        markdown.write(&document).map_err(io::Error::other)?;
        let markdown = trim_trailing_spaces(markdown.into_string().as_ref());

        let mut writer = self.writer;
        writer.write_all(markdown.as_bytes())
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

fn render_document(options: &MarkdownOptions, docs: &[MarkdownDoc<'_>]) -> io::Result<Node> {
    let mut nodes = vec![];
    if let Some(title) = &options.title {
        nodes.push(heading(options.heading_level, vec![text(title)]));
    }
    if options.include_table_of_contents {
        render_table_of_contents(&mut nodes, options, docs);
    }

    for doc in docs {
        render_config_reference(&mut nodes, options, doc)?;
    }
    Ok(Node::Document(nodes))
}

fn render_table_of_contents(
    nodes: &mut Vec<Node>,
    options: &MarkdownOptions,
    docs: &[MarkdownDoc<'_>],
) {
    nodes.push(heading(options.heading_level + 1, vec![text("Contents")]));

    let mut anchors = BTreeMap::new();
    let mut items = vec![];
    for doc in docs {
        let config_name = config_heading_text(doc.config);
        let anchor = unique_anchor(slugify_heading(&config_name), &mut anchors);
        let mut children = vec![];

        if let Some(tag) = doc.config.metadata().tag {
            let tag_ref = ParamRef {
                config: doc.config,
                param: tag.param,
            };
            let tag_path = tag_ref.canonical_path();
            let anchor = unique_anchor(slugify_heading(&tag_path), &mut anchors);
            children.push(list_item(vec![paragraph(vec![link(
                format!("#{anchor}"),
                vec![code(&tag_path)],
            )])]));
        }

        for param_ref in filtered_params_without_tag(doc) {
            let path = param_ref.canonical_path();
            let anchor = unique_anchor(slugify_heading(&path), &mut anchors);
            children.push(list_item(vec![paragraph(vec![link(
                format!("#{anchor}"),
                vec![code(&path)],
            )])]));
        }

        let mut content = vec![paragraph(vec![link(
            format!("#{anchor}"),
            vec![text(&config_name)],
        )])];
        if !children.is_empty() {
            content.push(Node::UnorderedList(children));
        }
        items.push(list_item(content));
    }
    nodes.push(Node::UnorderedList(items));
}

fn render_config_reference(
    nodes: &mut Vec<Node>,
    options: &MarkdownOptions,
    doc: &MarkdownDoc<'_>,
) -> io::Result<()> {
    let config = doc.config;
    nodes.push(heading(
        options.heading_level + 1,
        vec![code(&config_heading_text(config))],
    ));

    render_help(nodes, config.metadata().help);
    if options.include_rust_types {
        nodes.push(labeled_paragraph(
            "Rust config",
            vec![code(config.metadata().ty.name_in_code())],
        ));
    }
    if options.include_aliases {
        let aliases = format_aliases(config.aliases());
        if !aliases.is_empty() {
            nodes.push(labeled_paragraph("Aliases", aliases));
        }
    }
    if !config.metadata().validations.is_empty() {
        nodes.push(labeled_paragraph("Validations", vec![]));
        nodes.push(Node::UnorderedList(
            config
                .metadata()
                .validations
                .iter()
                .map(|validation| list_item(vec![paragraph(vec![text(&validation.to_string())])]))
                .collect(),
        ));
    }

    if let Some(tag) = config.metadata().tag {
        render_tag_reference(nodes, options, config, &tag, &doc.conditions);
    }
    for param_ref in filtered_params_without_tag(doc) {
        render_param_reference(nodes, options, param_ref, &doc.conditions)?;
    }
    Ok(())
}

fn render_tag_reference(
    nodes: &mut Vec<Node>,
    options: &MarkdownOptions,
    config: ConfigRef<'_>,
    tag: &ConfigTag,
    conditions: &[(ParamRef<'_>, &ConfigVariant)],
) {
    let tag_ref = ParamRef {
        config,
        param: tag.param,
    };
    nodes.push(heading(
        options.heading_level + 2,
        vec![code(&tag_ref.canonical_path())],
    ));

    if options.include_aliases {
        let aliases = format_aliases(tag_ref.all_paths().skip(1));
        if !aliases.is_empty() {
            nodes.push(labeled_paragraph("Aliases", aliases));
        }
    }
    if let Some(env_options) = &options.include_env_vars {
        nodes.push(labeled_paragraph(
            "Environment variable",
            vec![code(&env_var_name(
                &env_options.prefix,
                &tag_ref.canonical_path(),
            ))],
        ));
    }

    nodes.push(labeled_paragraph(
        "Type",
        vec![text("string tag with variants:")],
    ));
    nodes.push(Node::UnorderedList(
        tag.variants
            .iter()
            .map(|variant| render_variant(config, tag, variant))
            .collect(),
    ));
    render_conditions(nodes, conditions.iter().rev().copied());
}

fn render_variant(config: ConfigRef<'_>, tag: &ConfigTag, variant: &ConfigVariant) -> ListItem {
    let mut content = vec![
        code(variant.name),
        text(" (Rust: "),
        code(&format!(
            "{}::{}",
            config.metadata().ty.name_in_code(),
            variant.rust_name
        )),
        text(")"),
    ];
    if tag
        .default_variant
        .is_some_and(|default| default.rust_name == variant.rust_name)
    {
        content.push(text(" "));
        content.push(strong(vec![text("(default)")]));
    }
    if !variant.help.is_empty() {
        content.push(text(" - "));
        content.push(text(variant.help));
    }

    let mut blocks = vec![paragraph(content)];
    if !variant.aliases.is_empty() {
        let aliases = format_strs_as_code(variant.aliases.iter().copied());
        blocks.push(Node::UnorderedList(vec![list_item(vec![
            labeled_paragraph("Aliases", aliases),
        ])]));
    }
    list_item(blocks)
}

fn render_param_reference(
    nodes: &mut Vec<Node>,
    options: &MarkdownOptions,
    param_ref: ParamRef<'_>,
    conditions: &[(ParamRef<'_>, &ConfigVariant)],
) -> io::Result<()> {
    nodes.push(heading(
        options.heading_level + 2,
        vec![code(&param_ref.canonical_path())],
    ));

    if options.include_aliases {
        let aliases = format_aliases(param_ref.all_paths().skip(1));
        if !aliases.is_empty() {
            nodes.push(labeled_paragraph("Aliases", aliases));
        }
    }
    if let Some(env_options) = &options.include_env_vars {
        nodes.push(labeled_paragraph(
            "Environment variable",
            vec![code(&env_var_name(
                &env_options.prefix,
                &param_ref.canonical_path(),
            ))],
        ));
    }

    let description = param_ref.param.type_description();
    let type_doc = render_type_doc(
        "Type",
        param_ref.param.expecting,
        &description,
        options,
        true,
    );
    nodes.push(labeled_paragraph("Type", type_doc.summary));
    if !type_doc.children.is_empty() {
        nodes.push(Node::UnorderedList(
            type_doc.children.iter().map(render_detail).collect(),
        ));
    }

    let full_conditions = conditions.iter().rev().copied().chain(
        param_ref
            .param
            .tag_variant
            .map(|variant| (ParamRef::for_tag(param_ref.config), variant)),
    );
    render_conditions(nodes, full_conditions);

    let default = param_ref.param.default_value_json();
    if let Some(default) = &default {
        render_json_value(nodes, "Default", default)?;
    }
    let example = param_ref
        .param
        .example_value_json()
        .filter(|val| Some(val) != default.as_ref());
    if options.include_examples
        && let Some(example) = example
    {
        render_json_value(nodes, "Example", &example)?;
    }
    if let Some(fallback) = param_ref.param.fallback {
        render_multiline_value(nodes, "Fallbacks", &fallback.to_string());
    }
    render_help(nodes, param_ref.param.help);
    Ok(())
}

fn render_type_doc(
    field: &str,
    expecting: BasicTypes,
    description: &TypeDescription,
    options: &MarkdownOptions,
    is_top_level: bool,
) -> DetailDoc {
    let mut summary = vec![];
    if options.include_secret_marker && description.contains_secrets() {
        summary.push(text("secret "));
    }
    summary.push(text(&format_basic_types(expecting)));
    if options.include_rust_types && !description.rust_type().is_empty() {
        summary.push(text(" (Rust: "));
        summary.push(code(description.rust_type()));
        summary.push(text(")"));
    }
    if let Some(details) = description.details() {
        summary.push(text("; "));
        summary.push(text(details));
    }
    if let Some(unit) = description.unit() {
        summary.push(text("; unit: "));
        summary.push(code(&unit.to_string()));
    }

    let mut doc = DetailDoc::new(field, summary);
    if is_top_level && let Some(suffixes) = description.suffixes().and_then(format_suffixes) {
        doc.children.push(DetailDoc::new("Name suffixes", suffixes));
    }
    if !description.validations().is_empty() {
        doc.children.push(DetailDoc {
            label: format!("{field} validations"),
            summary: vec![],
            children: description
                .validations()
                .iter()
                .map(|validation| DetailDoc::new("", vec![text(validation)]))
                .collect(),
        });
    }
    if let Some(condition) = description.deserialize_if() {
        doc.children.push(DetailDoc::new(
            format!("{field} filtering"),
            vec![text(condition), text(", otherwise set to "), code("null")],
        ));
    }

    if let Some((expecting, item)) = description.items() {
        doc.children.push(render_type_doc(
            "Array items",
            expecting,
            item,
            options,
            false,
        ));
    }
    if let Some(separator) = description.item_separator() {
        doc.children.push(DetailDoc::new(
            "Item separator",
            format_separator(separator),
        ));
    }

    if let Some((expecting, key)) = description.keys() {
        doc.children
            .push(render_type_doc("Map keys", expecting, key, options, false));
    }
    if let Some((expecting, value)) = description.values() {
        doc.children.push(render_type_doc(
            "Map values",
            expecting,
            value,
            options,
            false,
        ));
    }
    if let Some((entry_sep, kv_sep)) = description.entry_separators() {
        doc.children.push(DetailDoc::new(
            "Entries separator",
            format_separator(entry_sep),
        ));
        doc.children.push(DetailDoc::new(
            "Key-value separator",
            format_separator(kv_sep),
        ));
    }

    if let Some((expecting, fallback)) = description.fallback() {
        doc.children.push(render_type_doc(
            "Fallback", expecting, fallback, options, false,
        ));
    }
    doc
}

fn render_detail(detail: &DetailDoc) -> ListItem {
    let mut content = if detail.label.is_empty() {
        detail.summary.clone()
    } else if detail.summary.is_empty() {
        vec![strong(vec![text(&format!("{}:", detail.label))])]
    } else {
        let mut content = vec![strong(vec![text(&format!("{}:", detail.label))]), text(" ")];
        content.extend(detail.summary.clone());
        content
    };

    let mut blocks = vec![paragraph(std::mem::take(&mut content))];
    if !detail.children.is_empty() {
        blocks.push(Node::UnorderedList(
            detail.children.iter().map(render_detail).collect(),
        ));
    }
    list_item(blocks)
}

fn render_conditions<'a>(
    nodes: &mut Vec<Node>,
    conditions: impl Iterator<Item = (ParamRef<'a>, &'a ConfigVariant)>,
) {
    let conditions: Vec<_> = conditions
        .map(|(tag_ref, variant)| format!("{} == '{}'", tag_ref.canonical_path(), variant.name))
        .collect();
    if !conditions.is_empty() {
        let field = if conditions.len() == 1 { "Tag" } else { "Tags" };
        nodes.push(labeled_paragraph(
            field,
            vec![code(&conditions.join(" && "))],
        ));
    }
}

fn render_json_value(
    nodes: &mut Vec<Node>,
    label: &str,
    value: &serde_json::Value,
) -> io::Result<()> {
    let pretty = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
    if pretty.lines().count() <= 1 {
        nodes.push(labeled_paragraph(label, vec![code(&pretty)]));
    } else {
        nodes.push(labeled_paragraph(label, vec![]));
        nodes.push(Node::code_block(Some("json".into()), pretty.into()));
    }
    Ok(())
}

fn render_multiline_value(nodes: &mut Vec<Node>, label: &str, value: &str) {
    let mut lines = value.lines();
    let Some(first_line) = lines.next() else {
        return;
    };
    nodes.push(labeled_paragraph(label, vec![text(first_line)]));
    let extra_lines: Vec<_> = lines.collect();
    if !extra_lines.is_empty() {
        nodes.push(Node::UnorderedList(
            extra_lines
                .into_iter()
                .map(|line| list_item(vec![paragraph(vec![text(line)])]))
                .collect(),
        ));
    }
}

fn render_help(nodes: &mut Vec<Node>, help: &str) {
    nodes.extend(
        help.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| paragraph(vec![text(line)])),
    );
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

fn format_aliases<A: AsRef<str>>(
    aliases: impl Iterator<Item = (A, smart_config::metadata::AliasOptions)>,
) -> Vec<Node> {
    let mut rendered = vec![];
    for (i, (alias, options)) in aliases.enumerate() {
        if i > 0 {
            rendered.push(text(", "));
        }
        rendered.push(code(alias.as_ref()));
        if options.is_deprecated {
            rendered.push(text(" (deprecated)"));
        }
    }
    rendered
}

fn format_strs_as_code<'a>(values: impl Iterator<Item = &'a str>) -> Vec<Node> {
    let mut rendered = vec![];
    for (i, value) in values.enumerate() {
        if i > 0 {
            rendered.push(text(", "));
        }
        rendered.push(code(value));
    }
    rendered
}

fn format_suffixes(suffixes: TypeSuffixes) -> Option<Vec<Node>> {
    match suffixes {
        TypeSuffixes::DurationUnits => Some(vec![
            text("duration units from millis to weeks, for example "),
            code("_ms"),
            text(" or "),
            code("_in_sec"),
        ]),
        TypeSuffixes::SizeUnits => Some(vec![
            text("byte size units up to gigabytes, for example "),
            code("_mb"),
            text(" or "),
            code("_in_kib"),
        ]),
        TypeSuffixes::EtherUnits => Some(vec![
            text("ether value units, for example "),
            code("_gwei"),
            text(" or "),
            code("_in_ether"),
        ]),
        _ => None,
    }
}

fn format_separator(separator: &PatternDisplay) -> Vec<Node> {
    match separator {
        PatternDisplay::Exact(s) => vec![text("exact match: "), code(&format!("{s:?}"))],
        PatternDisplay::Regex(regex) => vec![text("regex: "), code(regex)],
        PatternDisplay::Generic(display) => vec![text(display)],
        _ => vec![text(&separator.to_string())],
    }
}

fn format_basic_types(expecting: BasicTypes) -> String {
    expecting.to_string().replace(" | ", " or ").to_lowercase()
}

fn labeled_paragraph(label: &str, value: Vec<Node>) -> Node {
    let mut content = vec![strong(vec![text(&format!("{label}:"))])];
    if !value.is_empty() {
        content.push(text(" "));
        content.extend(value);
    }
    paragraph(content)
}

fn heading(level: u8, content: Vec<Node>) -> Node {
    Node::heading(level.clamp(1, 6), content)
}

fn paragraph(content: Vec<Node>) -> Node {
    Node::Paragraph(content)
}

fn list_item(content: Vec<Node>) -> ListItem {
    ListItem::Unordered { content }
}

fn text(value: &str) -> Node {
    Node::Text(value.into())
}

fn code(value: &str) -> Node {
    Node::InlineCode(value.into())
}

fn strong(content: Vec<Node>) -> Node {
    Node::Strong(content)
}

fn link(url: String, content: Vec<Node>) -> Node {
    Node::Link {
        url: url.into(),
        title: None,
        content,
    }
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

fn trim_trailing_spaces(markdown: &str) -> String {
    let mut trimmed = markdown
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    if markdown.ends_with('\n') {
        trimmed.push('\n');
    }
    trimmed
}
