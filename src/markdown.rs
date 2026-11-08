//! Markdown to HTML, using comrak.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use anyhow::{Result, bail};
use comrak::adapters::SyntaxHighlighterAdapter;
use comrak::nodes::NodeValue;
use comrak::options::Plugins;
use comrak::{Arena, Options, format_html_with_plugins, parse_document};

use crate::highlight::{escape_html, highlight};

/// Resolves a link to another Markdown file (e.g. `2022-10-16-bf_compiler-part1.md`)
/// into its URL.
pub type LinkResolver<'a> = &'a dyn Fn(&str) -> Option<String>;

pub struct Rendered {
    pub html: String,
    /// The plain text of the first paragraph, used for descriptions and feed summaries.
    pub excerpt: String,
}

pub fn render(markdown: &str, resolve_link: LinkResolver) -> Result<Rendered> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.footnotes = true;
    options.extension.strikethrough = true;
    // Give headings an id and an anchor link (styled in sass/main.scss).
    options.extension.header_id_prefix = Some(String::new());
    options.parse.smart = true;
    // Allow raw HTML in posts.
    options.render.r#unsafe = true;

    let arena = Arena::new();
    let root = parse_document(&arena, markdown, &options);

    // Resolve links to other Markdown files.
    let mut broken = Vec::new();
    for node in root.descendants() {
        if let NodeValue::Link(link) | NodeValue::Image(link) = &mut node.data_mut().value
            && let Some(url) = resolve_md_link(&link.url, resolve_link)
        {
            match url {
                Some(url) => link.url = url,
                None => broken.push(link.url.clone()),
            }
        }
    }
    if !broken.is_empty() {
        bail!("broken links: {}", broken.join(", "));
    }

    let excerpt = root
        .children()
        .find(|n| matches!(n.data().value, NodeValue::Paragraph))
        .map(|n| {
            n.collect_text()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();

    let mut plugins = Plugins::default();
    plugins.render.codefence_syntax_highlighter = Some(&Highlighter);
    let mut html = String::new();
    format_html_with_plugins(root, &options, &mut html, &plugins)?;
    Ok(Rendered { html, excerpt })
}

/// Returns `None` if `url` is not a link to a Markdown file, and `Some(None)`
/// if it is a link to a file that doesn't exist.
fn resolve_md_link(url: &str, resolve_link: LinkResolver) -> Option<Option<String>> {
    if url.contains("://") || url.starts_with('/') || url.starts_with('#') {
        return None;
    }
    let (path, fragment) = match url.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (url, None),
    };
    if !path.ends_with(".md") {
        return None;
    }
    Some(resolve_link(path).map(|resolved| match fragment {
        Some(f) => format!("{resolved}#{f}"),
        None => resolved,
    }))
}

struct Highlighter;

impl SyntaxHighlighterAdapter for Highlighter {
    fn write_highlighted(
        &self,
        output: &mut dyn fmt::Write,
        lang: Option<&str>,
        code: &str,
    ) -> fmt::Result {
        output.write_str(&highlight(lang.unwrap_or(""), code))
    }

    fn write_pre_tag(
        &self,
        output: &mut dyn fmt::Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        write_tag(output, "pre", attributes)
    }

    fn write_code_tag(
        &self,
        output: &mut dyn fmt::Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        write_tag(output, "code", attributes)
    }
}

fn write_tag(
    output: &mut dyn fmt::Write,
    tag: &str,
    attributes: HashMap<&'static str, Cow<'_, str>>,
) -> fmt::Result {
    write!(output, "<{tag}")?;
    let mut attributes: Vec<_> = attributes.into_iter().collect();
    attributes.sort();
    for (name, value) in attributes {
        write!(output, " {name}=\"{}\"", escape_html(&value))?;
    }
    output.write_str(">")
}
