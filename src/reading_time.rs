//! Estimated reading time, ported from the Liquid code of the old Jekyll post
//! layout: code words count as half a word, and the rest is read at 250 words
//! per minute.

use std::sync::LazyLock;

use regex::Regex;

pub fn minutes(content_html: &str) -> u64 {
    let words_total = count_words(&strip_html(content_html));
    let without_code = content_html
        .replace("<pre>", "<!--")
        .replace("</pre>", "-->");
    let words_without_code = count_words(&strip_html(&without_code));

    let words_code = (words_total - words_without_code) as f64 / 2.0;
    let words = (words_without_code as f64 + words_code).round() as u64;
    (words / 250).max(1)
}

fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Strip HTML tags and comments, like Liquid's `strip_html`.
fn strip_html(html: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->|<.*?>").unwrap());
    RE.replace_all(html, "").into_owned()
}
