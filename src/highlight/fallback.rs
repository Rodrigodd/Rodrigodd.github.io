//! Hand-written highlighters for languages that arborium doesn't support.
//!
//! They output the same tags as arborium (e.g. `<a-k>` for keywords), so the
//! highlight themes apply to them too. The tags are listed in arborium-theme's
//! `highlights.rs`.

use super::escape_html;

/// Highlight `code`, or return `None` if there is no highlighter for `lang`.
pub fn highlight(lang: &str, code: &str) -> Option<String> {
    let tokens = match lang {
        "brainfuck" | "bf" => brainfuck(code),
        "clif" => clif(code),
        _ => return None,
    };
    Some(tokens.to_html())
}

/// A list of tokens, each with an optional tag.
#[derive(Default)]
struct Tokens(Vec<(Option<&'static str>, String)>);

impl Tokens {
    /// Push a token, merging it with the previous one if they have the same tag.
    fn push(&mut self, tag: Option<&'static str>, text: &str) {
        match self.0.last_mut() {
            Some((t, s)) if *t == tag => s.push_str(text),
            _ => self.0.push((tag, text.to_string())),
        }
    }

    fn to_html(&self) -> String {
        let mut out = String::new();
        for (tag, text) in &self.0 {
            match tag {
                Some(tag) => out.push_str(&format!("<a-{tag}>{}</a-{tag}>", escape_html(text))),
                None => out.push_str(&escape_html(text)),
            }
        }
        out
    }
}

fn brainfuck(code: &str) -> Tokens {
    let mut tokens = Tokens::default();
    for c in code.chars() {
        let tag = match c {
            '+' | '-' => "o",
            '<' | '>' => "k",
            '.' | ',' => "f",
            '[' | ']' => "pb",
            // Everything else is ignored by Brainfuck, so it is a comment.
            _ => "c",
        };
        tokens.push(Some(tag), c.encode_utf8(&mut [0; 4]));
    }
    tokens
}

const CLIF_KEYWORDS: &[&str] = &[
    "function",
    "system_v",
    "windows_fastcall",
    "fast",
    "cold",
    "tail",
    "apple_aarch64",
];

/// Cranelift IR, the textual format of Cranelift's intermediate representation.
fn clif(code: &str) -> Tokens {
    let mut tokens = Tokens::default();
    let mut rest = code;
    while let Some(c) = rest.chars().next() {
        let len = if rest.starts_with(';') || rest.starts_with("//") {
            let len = rest.find('\n').unwrap_or(rest.len());
            tokens.push(Some("c"), &rest[..len]);
            len
        } else if c.is_ascii_alphabetic() || c == '_' {
            let len = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let word = &rest[..len];
            tokens.push(Some(clif_word_tag(word, &rest[len..])), word);
            len
        } else if c.is_ascii_digit()
            || (c == '-' && rest[1..].starts_with(|c: char| c.is_ascii_digit()))
        {
            let len = 1 + rest[1..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len() - 1);
            tokens.push(Some("n"), &rest[..len]);
            len
        } else if rest.starts_with("->") {
            tokens.push(Some("o"), "->");
            2
        } else {
            let tag = match c {
                '=' | '!' | '+' | '*' | '-' => Some("o"),
                '(' | ')' | '{' | '}' | '[' | ']' => Some("pb"),
                ',' | ':' | '.' => Some("pd"),
                _ => None,
            };
            let len = c.len_utf8();
            tokens.push(tag, &rest[..len]);
            len
        };
        rest = &rest[len..];
    }
    tokens
}

/// The tag of a word in Cranelift IR, given the text that follows it.
fn clif_word_tag(word: &str, after: &str) -> &'static str {
    // Entity references are a prefix followed by a number, like `v0` or `block1`.
    let entity = |prefix: &str| {
        word.strip_prefix(prefix)
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    };
    if CLIF_KEYWORDS.contains(&word) {
        "k"
    } else if entity("block") {
        "l"
    } else if entity("v") {
        "v"
    } else if entity("u") && after.starts_with(':') {
        // External function names, like `u0:0`.
        "f"
    } else if is_clif_type(word) {
        "t"
    } else if entity("fn") || entity("ss") || entity("gv") || entity("sig") || entity("jt") {
        "co"
    } else {
        // Instructions, like `iadd` or `iconst` (in `iconst.i64`).
        "f"
    }
}

/// Integer, float and reference types, and their vectors, like `i64` or `i32x4`.
fn is_clif_type(word: &str) -> bool {
    let (base, lanes) = match word.split_once('x') {
        Some((base, lanes)) => (base, Some(lanes)),
        None => (word, None),
    };
    let scalar = match base.split_at_checked(1) {
        Some(("i" | "f" | "r", bits)) => {
            matches!(bits, "8" | "16" | "32" | "64" | "128")
        }
        _ => false,
    };
    scalar && lanes.is_none_or(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_digit()))
}
