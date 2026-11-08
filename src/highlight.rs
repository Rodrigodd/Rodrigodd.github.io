//! Syntax highlighting of code blocks, using arborium (tree-sitter grammars).
//!
//! Languages are enabled by the `lang-*` features of arborium in Cargo.toml.
//! Languages that arborium doesn't support may have a highlighter in
//! `fallback.rs`; code in other languages is rendered as plain text.

mod fallback;

use std::sync::{LazyLock, Mutex};

use anyhow::Result;
use arborium::Highlighter;
use arborium::theme::builtin;

static HIGHLIGHTER: LazyLock<Mutex<Highlighter>> = LazyLock::new(|| Mutex::new(Highlighter::new()));

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Highlight `code` as HTML, or escape it if the language is not supported.
pub fn highlight(lang: &str, code: &str) -> String {
    if lang.is_empty() {
        return escape_html(code);
    }
    if let Some(html) = fallback::highlight(lang, code) {
        return html;
    }
    match HIGHLIGHTER.lock().unwrap().highlight(lang, code) {
        Ok(html) => html,
        Err(_) => escape_html(code),
    }
}

/// The CSS for the highlighted code, with a light and a dark theme following
/// the system preference.
pub fn css() -> Result<String> {
    let light = builtin::github_light().to_css("pre > code");
    let dark = builtin::github_dark().to_css("pre > code");
    // The themes use CSS nesting; compile them to plain CSS.
    let scss = format!("{light}\n@media (prefers-color-scheme: dark) {{\n{dark}\n}}\n");
    grass::from_string(scss, &grass::Options::default())
        .map_err(|e| anyhow::anyhow!("compiling highlight css: {e}"))
}
