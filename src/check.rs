//! Compares the generated site with a snapshot of the old Jekyll site, to
//! validate the port. Hard failures (the return value is `false`):
//! - a file of the old site is missing;
//! - the id or dates of a feed entry changed (this would notify subscribers again);
//! - an internal link is broken (unless it was already broken in the old site).
//!
//! Differences in the HTML, the syntax highlighting and the CSS are reported
//! for manual review.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context, Result};
use ego_tree::NodeRef;
use regex::Regex;
use scraper::{Html, Node, Selector};
use similar::TextDiff;
use walkdir::WalkDir;

const MAX_DIFF_LINES: usize = 60;

pub fn check(out: &Path, baseline: &Path) -> Result<bool> {
    let mut ok = true;

    println!("== files");
    let mut html_files = Vec::new();
    for entry in WalkDir::new(baseline) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(baseline)?.to_path_buf();
        if !out.join(&rel).exists() {
            println!("ERROR: missing {}", rel.display());
            ok = false;
        } else if rel.extension().is_some_and(|e| e == "html") {
            html_files.push(rel);
        }
    }
    html_files.sort();

    println!("== feed");
    ok &= check_feed(out, baseline)?;

    println!("== html");
    let mut highlight = HighlightStats::default();
    for rel in &html_files {
        let old = std::fs::read_to_string(baseline.join(rel))?;
        let new = std::fs::read_to_string(out.join(rel))?;
        compare_html(&rel.display().to_string(), &old, &new, &mut highlight);
    }
    highlight.report();

    println!("== links");
    ok &= check_links(out, baseline)?;

    println!("== css");
    compare_css(
        &std::fs::read_to_string(baseline.join("assets/css/style.css"))?,
        &std::fs::read_to_string(out.join("assets/css/style.css"))?,
    );

    println!();
    if ok {
        println!("check passed (review the differences reported above)");
    } else {
        println!("check FAILED");
    }
    Ok(ok)
}

#[derive(Debug, PartialEq)]
struct FeedEntry {
    title: String,
    link: String,
    published: String,
    updated: String,
    content: String,
}

fn parse_feed(xml: &str) -> (BTreeMap<String, String>, BTreeMap<String, FeedEntry>) {
    let tag = |s: &str, name: &str| -> String {
        let re = Regex::new(&format!(r"(?s)<{name}(?:\s[^>]*)?>(.*?)</{name}>")).unwrap();
        re.captures(s).map(|c| c[1].to_string()).unwrap_or_default()
    };
    let attr = |s: &str, re: &str| -> String {
        Regex::new(re)
            .unwrap()
            .captures(s)
            .map(|c| c[1].to_string())
            .unwrap_or_default()
    };
    let entry_re = Regex::new(r"(?s)<entry>.*?</entry>").unwrap();
    let header = entry_re.replace_all(xml, "");
    let mut meta = BTreeMap::new();
    meta.insert("id".to_string(), tag(&header, "id"));
    meta.insert("title".to_string(), tag(&header, "title"));
    meta.insert(
        "self link".to_string(),
        attr(&header, r#"<link href="([^"]*)" rel="self""#),
    );
    let mut entries = BTreeMap::new();
    for m in entry_re.find_iter(xml) {
        let e = m.as_str();
        entries.insert(
            tag(e, "id"),
            FeedEntry {
                title: tag(e, "title"),
                link: attr(e, r#"<link href="([^"]*)" rel="alternate""#),
                published: tag(e, "published"),
                updated: tag(e, "updated"),
                content: unescape_xml(&tag(e, "content")),
            },
        );
    }
    (meta, entries)
}

fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn check_feed(out: &Path, baseline: &Path) -> Result<bool> {
    let mut ok = true;
    let (old_meta, old) = parse_feed(&std::fs::read_to_string(baseline.join("feed.xml"))?);
    let (new_meta, new) = parse_feed(&std::fs::read_to_string(out.join("feed.xml"))?);
    if old_meta != new_meta {
        println!("ERROR: feed metadata changed:\n  old: {old_meta:?}\n  new: {new_meta:?}");
        ok = false;
    }
    // Entries are matched by link: a post whose entry has a different id
    // would be shown as a new post by feed readers.
    let mut checked = 0;
    for (id, o) in &old {
        let Some((new_id, n)) = new.iter().find(|(_, n)| n.link == o.link) else {
            // Old posts eventually drop out of the feed because of its size limit.
            println!("note: {} is no longer in the feed", o.link);
            continue;
        };
        checked += 1;
        for (field, a, b) in [
            ("id", id, new_id),
            ("published", &o.published, &n.published),
            ("updated", &o.updated, &n.updated),
        ] {
            if a != b {
                println!(
                    "ERROR: feed entry {}: {field} changed from {a:?} to {b:?}",
                    o.link
                );
                ok = false;
            }
        }
        if o.title != n.title {
            println!(
                "warning: feed entry {}: title changed from {:?} to {:?}",
                o.link, o.title, n.title
            );
        }
        let a = normalized_lines(&Html::parse_fragment(&o.content), &mut Vec::new());
        let b = normalized_lines(&Html::parse_fragment(&n.content), &mut Vec::new());
        if a != b {
            println!(
                "note: feed entry {}: content differs (see the HTML diff of the page)",
                o.link
            );
        }
    }
    for n in new
        .values()
        .filter(|n| !old.values().any(|o| o.link == n.link))
    {
        println!("note: new feed entry {}", n.link);
    }
    if ok {
        println!("ok: {checked} entries with the same id, published and updated dates");
    }
    Ok(ok)
}

/// Elements whose leading and trailing whitespace is not rendered.
const BLOCK_TAGS: &[&str] = &[
    "html",
    "head",
    "body",
    "header",
    "footer",
    "main",
    "nav",
    "article",
    "section",
    "div",
    "p",
    "ul",
    "ol",
    "li",
    "table",
    "thead",
    "tbody",
    "tr",
    "td",
    "th",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "hr",
    "pre",
    "meta",
    "link",
    "title",
    "script",
    "style",
    "svg",
    "path",
    "use",
    "symbol",
    "input",
    "label",
    "br",
    "img",
];

/// Serialize the DOM into lines that are easy to diff: one tag or text per
/// line, with sorted attributes and whitespace normalized the way a browser
/// renders it. The text of each `<pre>` is kept verbatim, and its highlighting
/// is pushed into `highlights` for a separate comparison.
fn normalized_lines(doc: &Html, highlights: &mut Vec<Vec<(char, String)>>) -> Vec<String> {
    let mut lines = Vec::new();
    for child in doc.tree.root().children() {
        walk(child, &mut lines, highlights);
    }
    lines.retain(|l| !l.is_empty());
    lines
}

fn trim_last_text(lines: &mut [String]) {
    if let Some(last) = lines.last_mut()
        && last.starts_with('"')
    {
        let text = last.trim_matches('"').trim_end().to_string();
        *last = if text.is_empty() {
            String::new()
        } else {
            format!("\"{text}\"")
        };
    }
}

fn walk(node: NodeRef<Node>, lines: &mut Vec<String>, highlights: &mut Vec<Vec<(char, String)>>) {
    match node.value() {
        Node::Element(e) => {
            let name = e.name();
            // The generator is expected to change.
            if name == "meta" && e.attr("name") == Some("generator") {
                return;
            }
            let block = BLOCK_TAGS.contains(&name);
            let mut attrs: Vec<_> = e
                .attrs()
                .map(|(k, v)| {
                    format!(
                        " {k}=\"{}\"",
                        v.split_whitespace().collect::<Vec<_>>().join(" ")
                    )
                })
                .collect();
            attrs.sort();
            if block {
                trim_last_text(lines);
            }
            lines.push(format!("<{name}{}>", attrs.concat()));
            if name == "pre" {
                let mut chars = Vec::new();
                collect_highlight(node, None, &mut chars);
                let text: String = chars.iter().map(|(c, _)| *c).collect();
                lines.push(format!("pre-text: {}", text.replace('\n', "⏎")));
                if e.attr("class") == Some("highlight") {
                    highlights.push(chars);
                }
            } else if name == "script" {
                let text: String = node
                    .children()
                    .filter_map(|c| c.value().as_text().map(|t| t.to_string()))
                    .collect();
                lines.push(format!("script: {}", text.trim()));
            } else {
                for child in node.children() {
                    walk(child, lines, highlights);
                }
            }
            if block {
                trim_last_text(lines);
            }
            lines.push(format!("</{name}>"));
        }
        Node::Text(t) => {
            let collapsed = t.split_whitespace().collect::<Vec<_>>().join(" ");
            let mut text = String::new();
            if t.starts_with(char::is_whitespace) {
                text.push(' ');
            }
            text.push_str(&collapsed);
            if t.ends_with(char::is_whitespace) && !collapsed.is_empty() {
                text.push(' ');
            }
            // Leading whitespace after a block tag is not rendered.
            let after_block = lines.last().is_none_or(|l| {
                l.starts_with('<') && {
                    let name = l.trim_start_matches(['<', '/']);
                    let name = name.split([' ', '>']).next().unwrap_or("");
                    BLOCK_TAGS.contains(&name)
                }
            });
            if after_block {
                text = text.trim_start().to_string();
            }
            if text.is_empty() {
                return;
            }
            // Merge with a previous text line (e.g. split by a comment).
            if let Some(last) = lines.last_mut()
                && last.starts_with('"')
            {
                let prev = last.trim_matches('"').to_string();
                let merged = format!("{prev}{text}").replace("  ", " ");
                *last = format!("\"{merged}\"");
            } else {
                lines.push(format!("\"{text}\""));
            }
        }
        _ => {}
    }
}

fn collect_highlight(node: NodeRef<Node>, class: Option<&str>, out: &mut Vec<(char, String)>) {
    for child in node.children() {
        match child.value() {
            Node::Text(t) => {
                for c in t.chars() {
                    out.push((c, class.unwrap_or("").to_string()));
                }
            }
            Node::Element(e) => {
                let class = if e.name() == "span" {
                    e.attr("class")
                } else {
                    class
                };
                collect_highlight(child, class, out);
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct HighlightStats {
    total: usize,
    matching: usize,
    /// (expected class, actual class) -> (count, example token)
    mismatches: HashMap<(String, String), (usize, String)>,
}

impl HighlightStats {
    fn add(&mut self, old: &[(char, String)], new: &[(char, String)]) {
        let mut i = 0;
        while i < old.len().min(new.len()) {
            let (c, a) = &old[i];
            let b = &new[i].1;
            if c.is_whitespace() {
                i += 1;
                continue;
            }
            self.total += 1;
            if a == b {
                self.matching += 1;
            } else {
                // Record the token, as the run of chars with the same classes.
                let mut token = String::new();
                let mut j = i;
                while j < old.len().min(new.len()) && old[j].1 == *a && new[j].1 == *b {
                    token.push(old[j].0);
                    j += 1;
                }
                let e = self
                    .mismatches
                    .entry((a.clone(), b.clone()))
                    .or_insert((0, token.clone()));
                e.0 += token.chars().filter(|c| !c.is_whitespace()).count();
                self.total += token.chars().filter(|c| !c.is_whitespace()).count() - 1;
                i = j;
                continue;
            }
            i += 1;
        }
    }

    fn report(&self) {
        if self.total == 0 {
            return;
        }
        println!(
            "highlighting: {:.2}% of {} code characters have the same class as Rouge",
            100.0 * self.matching as f64 / self.total as f64,
            self.total
        );
        let mut m: Vec<_> = self.mismatches.iter().collect();
        m.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
        for ((a, b), (count, example)) in m.iter().take(15) {
            println!("  {count:5} chars: {a:>3} -> {b:<3} e.g. {example:?}");
        }
    }
}

fn compare_html(name: &str, old: &str, new: &str, stats: &mut HighlightStats) {
    let old_doc = Html::parse_document(old);
    let new_doc = Html::parse_document(new);
    let mut old_hl = Vec::new();
    let mut new_hl = Vec::new();
    let old_lines = normalized_lines(&old_doc, &mut old_hl);
    let new_lines = normalized_lines(&new_doc, &mut new_hl);

    for (a, b) in old_hl.iter().zip(&new_hl) {
        stats.add(a, b);
    }

    // Compare the JSON-LD as data, as the key order doesn't matter.
    let json_ld = |doc: &Html| -> Option<serde_json::Value> {
        let sel = Selector::parse(r#"script[type="application/ld+json"]"#).unwrap();
        let text: String = doc.select(&sel).next()?.text().collect();
        serde_json::from_str(&text).ok()
    };
    let skip = |l: &&String| !l.starts_with("script: {");
    let old_lines: Vec<String> = old_lines.iter().filter(skip).cloned().collect();
    let new_lines: Vec<String> = new_lines.iter().filter(skip).cloned().collect();

    let mut problems = Vec::new();
    if json_ld(&old_doc) != json_ld(&new_doc) {
        problems.push(format!(
            "JSON-LD differs:\n  old: {:?}\n  new: {:?}",
            json_ld(&old_doc),
            json_ld(&new_doc)
        ));
    }
    let old_text = old_lines.join("\n") + "\n";
    let new_text = new_lines.join("\n") + "\n";
    if old_text != new_text {
        let diff = TextDiff::from_lines(&old_text, &new_text);
        let unified = diff.unified_diff().context_radius(1).to_string();
        let changed = unified
            .lines()
            .filter(|l| l.starts_with(['+', '-']))
            .count();
        let mut text: Vec<&str> = unified.lines().take(MAX_DIFF_LINES).collect();
        if unified.lines().count() > MAX_DIFF_LINES {
            text.push("  ...");
        }
        problems.push(format!("{changed} changed lines:\n{}", text.join("\n")));
    }
    if problems.is_empty() {
        println!("ok: {name}");
    } else {
        println!("DIFF: {name}");
        for p in problems {
            println!("{p}");
        }
    }
}

/// Find internal links that point to missing files or missing ids.
fn broken_links(site: &Path) -> Result<BTreeSet<String>> {
    let mut ids: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut links = Vec::new();
    let id_re = Regex::new(r#"\sid="([^"]*)""#).unwrap();
    let link_re = Regex::new(r#"\s(?:href|src|xlink:href)="([^"]*)""#).unwrap();
    for entry in WalkDir::new(site) {
        let entry = entry?;
        let path = entry.path();
        if !path.extension().is_some_and(|e| e == "html" || e == "svg") {
            continue;
        }
        let rel = format!(
            "/{}",
            path.strip_prefix(site)?
                .to_string_lossy()
                .replace('\\', "/")
        );
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {rel}"))?;
        ids.insert(
            rel.clone(),
            id_re
                .captures_iter(&text)
                .map(|c| c[1].to_string())
                .collect(),
        );
        for c in link_re.captures_iter(&text) {
            links.push((rel.clone(), c[1].to_string()));
        }
    }
    let mut broken = BTreeSet::new();
    for (page, link) in links {
        if link.contains("://") || link.starts_with("mailto:") || link.starts_with("//") {
            continue;
        }
        let (path, fragment) = match link.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (link.as_str(), None),
        };
        let mut target = if path.is_empty() {
            page.clone()
        } else if path.starts_with('/') {
            path.to_string()
        } else {
            let dir = page.rsplit_once('/').map_or("", |(d, _)| d);
            format!("{dir}/{path}")
        };
        if target.ends_with('/') {
            target.push_str("index.html");
        }
        let exists = site.join(target.trim_start_matches('/')).is_file();
        let fragment_ok = match fragment {
            None | Some("") => true,
            Some(f) => ids.get(&target).is_some_and(|ids| ids.contains(f)),
        };
        if !exists || !fragment_ok {
            broken.insert(format!("{page} -> {link}"));
        }
    }
    Ok(broken)
}

fn check_links(out: &Path, baseline: &Path) -> Result<bool> {
    let new = broken_links(out)?;
    let old = broken_links(baseline)?;
    let mut ok = true;
    for link in &new {
        if old.contains(link) {
            println!("warning: broken link (also broken in the old site): {link}");
        } else {
            println!("ERROR: broken link: {link}");
            ok = false;
        }
    }
    if new.is_empty() {
        println!("ok: no broken internal links");
    }
    Ok(ok)
}

fn normalize_css(css: &str) -> Vec<String> {
    let comments = Regex::new(r"(?s)/\*.*?\*/").unwrap();
    let css = comments.replace_all(css, "");
    let css = css.split_whitespace().collect::<Vec<_>>().join(" ");
    let spaces = Regex::new(r"\s*([{};:,>])\s*").unwrap();
    let css = spaces.replace_all(&css, "$1").replace(";}", "}");
    let zero = Regex::new(r"([:\s,(])0\.").unwrap();
    let css = zero.replace_all(&css, "$1.");
    // Equivalent notations of colors and attribute selectors.
    let short_hex = Regex::new(r"#([0-9a-fA-F])([0-9a-fA-F])([0-9a-fA-F])\b").unwrap();
    let css = short_hex.replace_all(&css, "#$1$1$2$2$3$3");
    let css = css
        .replace(":teal", ":#008080")
        .replace(":purple", ":#800080")
        .replace(":navy", ":#000080");
    let quoted_attr = Regex::new(r#"\[([\w-]+)="([\w-]+)"\]"#).unwrap();
    let css = quoted_attr.replace_all(&css, "[$1=$2]");
    css.split_inclusive('}')
        .map(|r| r.trim().to_lowercase())
        .filter(|r| !r.is_empty())
        .collect()
}

fn compare_css(old: &str, new: &str) {
    let old = normalize_css(old).join("\n") + "\n";
    let new = normalize_css(new).join("\n") + "\n";
    if old == new {
        println!("ok: style.css");
        return;
    }
    let diff = TextDiff::from_lines(&old, &new);
    let unified = diff.unified_diff().context_radius(0).to_string();
    println!("DIFF: style.css");
    for line in unified.lines().take(MAX_DIFF_LINES) {
        println!("{line}");
    }
}
