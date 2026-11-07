//! Markdown to HTML. Parsing is done by comrak, but the HTML is produced by our
//! own renderer, which mimics the output of kramdown (the Markdown converter
//! used by Jekyll), including the kramdown-specific features used by the posts:
//! attribute lists (`{: #id}`, `{:style="..."}`), smart quotes, header ids and
//! footnote markup.

use std::collections::HashMap;

use anyhow::{Result, bail};
use comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};
use comrak::{Arena, Options, parse_document};
use regex::Regex;
use std::sync::LazyLock;

use crate::highlight::{code_block, escape_html};

/// Resolves a link to another Markdown file (e.g. `2022-10-16-bf_compiler-part1.md`)
/// into its URL.
pub type LinkResolver<'a> = &'a dyn Fn(&str) -> Option<String>;

pub struct Rendered {
    /// The HTML of the document.
    pub html: String,
    /// The HTML of the document with an anchor link in each heading.
    pub html_with_anchors: String,
    /// The plain text of the first block, used for descriptions and feed summaries.
    pub excerpt: String,
}

pub fn render(markdown: &str, resolve_link: LinkResolver) -> Result<Rendered> {
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.footnotes = true;
    options.extension.strikethrough = true;
    let root = parse_document(&arena, markdown, &options);

    merge_text_nodes(root);
    let attrs = extract_attribute_lists(root);

    let excerpt = match root.first_child() {
        Some(first) => {
            let mut r = Renderer::new(&attrs, resolve_link, false);
            r.render_node(first);
            normalize_whitespace(&strip_html(&r.out))
        }
        None => String::new(),
    };

    let mut html = String::new();
    for anchors in [false, true] {
        let mut r = Renderer::new(&attrs, resolve_link, anchors);
        r.render_document(root);
        if !r.errors.is_empty() {
            bail!("{}", r.errors.join("\n"));
        }
        if anchors {
            return Ok(Rendered {
                html,
                html_with_anchors: r.out,
                excerpt,
            });
        }
        html = r.out;
    }
    unreachable!()
}

/// Strip HTML tags, comments, scripts and styles, like Liquid's `strip_html`.
pub fn strip_html(html: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)<script.*?</script>|<!--.*?-->|<style.*?</style>|<.*?>").unwrap()
    });
    RE.replace_all(html, "").into_owned()
}

pub fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn merge_text_nodes<'a>(root: &'a AstNode<'a>) {
    for node in root.descendants() {
        while let Some(next) = node.next_sibling() {
            let mut data = node.data_mut();
            let NodeValue::Text(text) = &mut data.value else {
                break;
            };
            let NodeValue::Text(next_text) = &next.data().value else {
                break;
            };
            text.to_mut().push_str(next_text);
            drop(data);
            next.detach();
        }
    }
}

type Attrs = Vec<(String, String)>;
type NodeId = usize;

fn node_id(node: &AstNode) -> NodeId {
    node as *const AstNode as usize
}

/// Parse a kramdown inline attribute list, like `#id .class key="value"`.
fn parse_attribute_list(s: &str) -> Attrs {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"#([\w-]+)|\.([\w-]+)|([\w-]+)=(?:"([^"]*)"|'([^']*)')"#).unwrap()
    });
    let mut attrs: Attrs = Vec::new();
    for caps in RE.captures_iter(s) {
        if let Some(id) = caps.get(1) {
            attrs.push(("id".into(), id.as_str().into()));
        } else if let Some(class) = caps.get(2) {
            match attrs.iter_mut().find(|(k, _)| k == "class") {
                Some((_, v)) => {
                    v.push(' ');
                    v.push_str(class.as_str());
                }
                None => attrs.push(("class".into(), class.as_str().into())),
            }
        } else {
            let value = caps.get(4).or(caps.get(5)).unwrap();
            attrs.push((caps[3].into(), value.as_str().into()));
        }
    }
    attrs
}

/// Find kramdown attribute lists, remove them from the text, and return the
/// attributes of each element they apply to. Two forms are supported:
/// - `![](img.png){:style="..."}`: applies to the preceding inline element.
/// - a line with only `{: #id}` at the end of a paragraph: applies to the paragraph.
fn extract_attribute_lists<'a>(root: &'a AstNode<'a>) -> HashMap<NodeId, Attrs> {
    static IAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\{:\s*([^}]*)\}").unwrap());
    let mut result = HashMap::new();
    let mut to_detach = Vec::new();
    for node in root.descendants() {
        let mut data = node.data_mut();
        let NodeValue::Text(text) = &mut data.value else {
            continue;
        };
        let Some(caps) = IAL.captures(text) else {
            continue;
        };
        let attrs = parse_attribute_list(&caps[1]);
        let len = caps[0].len();
        let prev = node.previous_sibling();
        let parent = node.parent().unwrap();
        if let Some(prev) = prev.filter(|p| !matches!(p.data().value, NodeValue::SoftBreak)) {
            result.insert(node_id(prev), attrs);
            text.to_mut().replace_range(..len, "");
            if text.is_empty() {
                to_detach.push(node);
            }
        } else if len == text.len()
            && node.next_sibling().is_none()
            && matches!(parent.data().value, NodeValue::Paragraph)
        {
            result.insert(node_id(parent), attrs);
            to_detach.push(node);
            to_detach.extend(prev);
        }
    }
    for node in to_detach {
        node.detach();
    }
    result
}

/// Generate a header id the way kramdown's GFM parser does.
fn header_id(text: &str, used: &mut HashMap<String, usize>) -> String {
    let id: String = text
        .to_lowercase()
        .chars()
        .filter(|&c| c.is_alphanumeric() || c == '_' || c == '-' || c == ' ' || c == '\t')
        .map(|c| if c == ' ' || c == '\t' { '-' } else { c })
        .collect();
    let count = used.entry(id.clone()).or_insert(0);
    *count += 1;
    if *count > 1 {
        format!("{}-{}", id, *count - 1)
    } else {
        id
    }
}

fn plain_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut out = String::new();
    for n in node.descendants() {
        match &n.data().value {
            NodeValue::Text(t) => out.push_str(t),
            NodeValue::Code(c) => out.push_str(&c.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

struct Renderer<'r, 'a> {
    out: String,
    attrs: &'r HashMap<NodeId, Attrs>,
    resolve_link: LinkResolver<'r>,
    anchors: bool,
    errors: Vec<String>,
    header_ids: HashMap<String, usize>,
    /// The last character of text output, for deciding smart quotes direction.
    prev_char: Option<char>,
    /// Footnote names in order of their first reference.
    footnotes: Vec<String>,
    footnote_refs: HashMap<String, usize>,
    footnote_defs: HashMap<String, &'a AstNode<'a>>,
    /// Whether paragraphs are rendered without `<p>`, as in tight lists.
    tight: bool,
}

impl<'r, 'a> Renderer<'r, 'a> {
    fn new(
        attrs: &'r HashMap<NodeId, Attrs>,
        resolve_link: LinkResolver<'r>,
        anchors: bool,
    ) -> Self {
        Self {
            out: String::new(),
            attrs,
            resolve_link,
            anchors,
            errors: Vec::new(),
            header_ids: HashMap::new(),
            prev_char: None,
            footnotes: Vec::new(),
            footnote_refs: HashMap::new(),
            footnote_defs: HashMap::new(),
            tight: false,
        }
    }

    fn render_document(&mut self, root: &'a AstNode<'a>) {
        for node in root.children() {
            if let NodeValue::FootnoteDefinition(def) = &node.data().value {
                self.footnote_defs.insert(def.name.clone(), node);
            }
        }
        self.render_children(root);
        self.render_footnotes();
    }

    fn render_children(&mut self, node: &'a AstNode<'a>) {
        for child in node.children() {
            self.render_node(child);
        }
    }

    fn attrs_of(&self, node: &'a AstNode<'a>) -> String {
        let mut s = String::new();
        if let Some(attrs) = self.attrs.get(&node_id(node)) {
            for (k, v) in attrs {
                s.push_str(&format!(" {}=\"{}\"", k, escape_html(v)));
            }
        }
        s
    }

    fn render_node(&mut self, node: &'a AstNode<'a>) {
        let value = node.data().value.clone();
        match value {
            NodeValue::Document => self.render_children(node),
            NodeValue::FrontMatter(_) | NodeValue::FootnoteDefinition(_) => {}
            NodeValue::Paragraph => {
                self.prev_char = None;
                if self.tight {
                    self.render_children(node);
                } else {
                    let attrs = self.attrs_of(node);
                    self.out.push_str(&format!("<p{attrs}>"));
                    self.render_children(node);
                    self.out.push_str("</p>\n\n");
                }
            }
            NodeValue::Heading(h) => {
                self.prev_char = None;
                let id = header_id(&plain_text(node), &mut self.header_ids);
                self.out.push_str(&format!("<h{} id=\"{}\">", h.level, id));
                self.render_children(node);
                if self.anchors {
                    self.out
                        .push_str(&format!("&nbsp;<a href=\"#{id}\" class=\"anchor\">§</a>"));
                }
                self.out.push_str(&format!("</h{}>\n\n", h.level));
            }
            NodeValue::BlockQuote => {
                self.out.push_str("<blockquote>\n");
                self.render_children(node);
                self.out.push_str("</blockquote>\n\n");
            }
            NodeValue::List(list) => {
                let tag = match list.list_type {
                    ListType::Bullet => "ul",
                    ListType::Ordered => "ol",
                };
                if list.list_type == ListType::Ordered && list.start != 1 {
                    self.out
                        .push_str(&format!("<{tag} start=\"{}\">\n", list.start));
                } else {
                    self.out.push_str(&format!("<{tag}>\n"));
                }
                let tight = std::mem::replace(&mut self.tight, list.tight);
                self.render_children(node);
                self.tight = tight;
                self.out.push_str(&format!("</{tag}>\n\n"));
            }
            NodeValue::Item(_) => {
                self.out.push_str("  <li>");
                // In a tight list only the first paragraph is rendered without `<p>`.
                let tight = self.tight;
                for (i, child) in node.children().enumerate() {
                    self.tight = tight && i == 0;
                    self.render_node(child);
                }
                self.tight = tight;
                self.out.push_str("</li>\n");
            }
            NodeValue::CodeBlock(block) => {
                let lang = block.info.split_whitespace().next().unwrap_or("");
                self.out.push_str(&code_block(lang, &block.literal));
                self.out.push('\n');
            }
            NodeValue::HtmlBlock(block) => {
                self.out.push_str(&block.literal);
                self.out.push('\n');
            }
            NodeValue::ThematicBreak => self.out.push_str("<hr />\n\n"),
            NodeValue::Table(table) => {
                self.out.push_str("<table>\n");
                let mut in_body = false;
                for (i, row) in node.children().enumerate() {
                    let header = matches!(row.data().value, NodeValue::TableRow(true));
                    // Like kramdown, a separator line (`|---|---|`) in the body
                    // starts a new `<tbody>`.
                    let is_separator = row.children().all(|cell| {
                        let text = plain_text(cell);
                        text.contains('-') && text.chars().all(|c| matches!(c, '-' | ':' | ' '))
                    });
                    if in_body && is_separator {
                        self.out.push_str("  </tbody>\n  <tbody>\n");
                        continue;
                    }
                    if i == 0 && header {
                        self.out.push_str("  <thead>\n");
                    } else if !in_body {
                        if i > 0 {
                            self.out.push_str("  </thead>\n");
                        }
                        self.out.push_str("  <tbody>\n");
                        in_body = true;
                    }
                    self.out.push_str("    <tr>\n");
                    let cell_tag = if header { "th" } else { "td" };
                    for (j, cell) in row.children().enumerate() {
                        let style = match table.alignments.get(j) {
                            Some(TableAlignment::Left) => " style=\"text-align: left\"",
                            Some(TableAlignment::Center) => " style=\"text-align: center\"",
                            Some(TableAlignment::Right) => " style=\"text-align: right\"",
                            _ => "",
                        };
                        self.prev_char = None;
                        self.out.push_str(&format!("      <{cell_tag}{style}>"));
                        self.render_children(cell);
                        self.out.push_str(&format!("</{cell_tag}>\n"));
                    }
                    self.out.push_str("    </tr>\n");
                }
                if in_body {
                    self.out.push_str("  </tbody>\n");
                } else {
                    self.out.push_str("  </thead>\n");
                }
                self.out.push_str("</table>\n\n");
            }
            NodeValue::Text(text) => self.text(&text),
            NodeValue::SoftBreak => {
                self.out.push('\n');
                self.prev_char = Some('\n');
            }
            NodeValue::LineBreak => {
                self.out.push_str("<br />\n");
                self.prev_char = Some('\n');
            }
            NodeValue::Code(code) => {
                self.out
                    .push_str("<code class=\"language-plaintext highlighter-rouge\">");
                self.out.push_str(&escape_html(&code.literal));
                self.out.push_str("</code>");
                self.prev_char = code.literal.chars().last().or(self.prev_char);
            }
            NodeValue::HtmlInline(html) => self.out.push_str(&html),
            NodeValue::Emph => self.wrap("em", node),
            NodeValue::Strong => self.wrap("strong", node),
            NodeValue::Strikethrough => self.wrap("del", node),
            NodeValue::Link(link) => {
                let url = self.resolve_url(&link.url);
                self.out
                    .push_str(&format!("<a href=\"{}\"", escape_html(&url)));
                if !link.title.is_empty() {
                    self.out
                        .push_str(&format!(" title=\"{}\"", escape_html(&link.title)));
                }
                let attrs = self.attrs_of(node);
                self.out.push_str(&format!("{attrs}>"));
                self.render_children(node);
                self.out.push_str("</a>");
            }
            NodeValue::Image(link) => {
                let url = self.resolve_url(&link.url);
                let alt = plain_text(node);
                self.out.push_str(&format!(
                    "<img src=\"{}\" alt=\"{}\"",
                    escape_html(&url),
                    escape_html(&alt)
                ));
                if !link.title.is_empty() {
                    self.out
                        .push_str(&format!(" title=\"{}\"", escape_html(&link.title)));
                }
                let attrs = self.attrs_of(node);
                self.out.push_str(&format!("{attrs} />"));
            }
            NodeValue::FootnoteReference(r) => {
                let name = r.name.clone();
                let number = match self.footnotes.iter().position(|n| *n == name) {
                    Some(i) => i + 1,
                    None => {
                        if !self.footnote_defs.contains_key(&name) {
                            self.errors.push(format!("undefined footnote: {name}"));
                        }
                        self.footnotes.push(name.clone());
                        self.footnotes.len()
                    }
                };
                let count = self.footnote_refs.entry(name.clone()).or_insert(0);
                let ref_id = if *count == 0 {
                    format!("fnref:{name}")
                } else {
                    format!("fnref:{name}:{count}")
                };
                *count += 1;
                self.out.push_str(&format!(
                    "<sup id=\"{ref_id}\" role=\"doc-noteref\"><a href=\"#fn:{name}\" class=\"footnote\" rel=\"footnote\">{number}</a></sup>"
                ));
            }
            other => self
                .errors
                .push(format!("unsupported markdown element: {other:?}")),
        }
    }

    fn wrap(&mut self, tag: &str, node: &'a AstNode<'a>) {
        self.out.push_str(&format!("<{tag}>"));
        self.render_children(node);
        self.out.push_str(&format!("</{tag}>"));
    }

    fn render_footnotes(&mut self) {
        if self.footnotes.is_empty() {
            return;
        }
        self.out
            .push_str("<div class=\"footnotes\" role=\"doc-endnotes\">\n  <ol>\n");
        // Footnotes referenced inside other footnotes are appended to the list
        // while it is rendered.
        let mut i = 0;
        while i < self.footnotes.len() {
            let name = self.footnotes[i].clone();
            i += 1;
            let Some(&def) = self.footnote_defs.get(&name) else {
                continue;
            };
            self.out
                .push_str(&format!("    <li id=\"fn:{name}\" role=\"doc-endnote\">\n"));
            let backlink = format!(
                "<a href=\"#fnref:{name}\" class=\"reversefootnote\" role=\"doc-backlink\">&#8617;</a>"
            );
            let children: Vec<_> = def.children().collect();
            for (j, child) in children.iter().enumerate() {
                let last = j + 1 == children.len();
                self.render_node(child);
                if last {
                    if matches!(child.data().value, NodeValue::Paragraph)
                        && self.out.ends_with("</p>\n\n")
                    {
                        self.out.truncate(self.out.len() - "</p>\n\n".len());
                        self.out.push_str(&format!(" {backlink}</p>\n"));
                    } else {
                        self.out.push_str(&format!("<p>{backlink}</p>\n"));
                    }
                }
            }
            self.out.push_str("    </li>\n");
        }
        self.out.push_str("  </ol>\n</div>\n");
    }

    fn resolve_url(&mut self, url: &str) -> String {
        if url.contains("://") || url.starts_with('/') || url.starts_with('#') {
            return url.to_string();
        }
        let (path, fragment) = match url.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (url, None),
        };
        if !path.ends_with(".md") {
            return url.to_string();
        }
        match (self.resolve_link)(path) {
            Some(resolved) => match fragment {
                Some(f) => format!("{resolved}#{f}"),
                None => resolved,
            },
            None => {
                self.errors.push(format!("broken link to {url}"));
                url.to_string()
            }
        }
    }

    /// Output text, with kramdown's typographic substitutions (smart quotes,
    /// dashes and ellipsis).
    fn text(&mut self, text: &str) {
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let mut buf = String::new();
        while i < chars.len() {
            let c = chars[i];
            let next = chars.get(i + 1).copied();
            let rest = &chars[i..];
            let (sym, len) = if rest.starts_with(&['-', '-', '-']) {
                ("—".to_string(), 3)
            } else if rest.starts_with(&['-', '-']) {
                ("–".to_string(), 2)
            } else if rest.starts_with(&['.', '.', '.']) {
                ("…".to_string(), 3)
            } else if c == '\'' || c == '"' {
                let open = self.quote_is_opening(next, chars.get(i + 2).copied());
                let q = match (c, open) {
                    ('\'', true) => '‘',
                    ('\'', false) => '’',
                    ('"', true) => '“',
                    _ => '”',
                };
                (q.to_string(), 1)
            } else {
                (c.to_string(), 1)
            };
            buf.push_str(&sym);
            self.prev_char = Some(c);
            i += len;
        }
        self.out.push_str(&escape_text(&buf));
    }

    fn quote_is_opening(&self, next: Option<char>, after_next: Option<char>) -> bool {
        // kramdown never closes a quote after an opening bracket or a dash.
        if self
            .prev_char
            .is_some_and(|p| matches!(p, '(' | '[' | '{' | '-'))
        {
            return true;
        }
        let prev_space = self.prev_char.is_none_or(char::is_whitespace);
        let next_word = next.is_some_and(|c| c.is_alphanumeric() || c == '_');
        if prev_space && next_word {
            return true;
        }
        let closes = self
            .prev_char
            .is_some_and(|p| !matches!(p, ' ' | '\\' | '\t' | '\r' | '\n'));
        if closes {
            return false;
        }
        let next_is_s_word = next == Some('s') && !after_next.is_some_and(char::is_alphanumeric);
        if next.is_none_or(char::is_whitespace) || next_is_s_word {
            return false;
        }
        true
    }
}

/// Escape text content (quotes don't need escaping outside attributes).
fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
