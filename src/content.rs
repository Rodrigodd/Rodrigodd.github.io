//! Loading of posts and pages from disk.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct FrontMatter {
    pub title: Option<String>,
    pub date: Option<String>,
    pub modified_date: Option<String>,
    pub layout: Option<String>,
    pub permalink: Option<String>,
}

pub struct Post {
    /// The file name without extension, e.g. `2022-10-16-bf_compiler-part1`.
    pub name: String,
    pub title: String,
    pub date: DateTime<Utc>,
    pub modified_date: Option<DateTime<Utc>>,
    /// e.g. `/2022/10/21/bf_compiler-part1.html`
    pub url: String,
    pub markdown: String,
}

pub struct Page {
    pub source: PathBuf,
    pub title: Option<String>,
    pub date: Option<DateTime<Utc>>,
    /// The template used to render the page.
    pub layout: String,
    pub url: String,
    pub body: String,
    pub is_markdown: bool,
}

/// Split a file into its YAML front matter and body.
fn split_front_matter(text: &str) -> Result<(FrontMatter, &str)> {
    let Some(rest) = text.strip_prefix("---\n") else {
        return Ok((FrontMatter::default(), text));
    };
    let (yaml, body) = match rest.strip_prefix("---\n") {
        Some(body) => ("", body),
        None => rest
            .split_once("\n---\n")
            .ok_or_else(|| anyhow!("unterminated front matter"))?,
    };
    let fm = if yaml.trim().is_empty() {
        FrontMatter::default()
    } else {
        serde_yaml::from_str(yaml)?
    };
    Ok((fm, body))
}

/// Parse a date in Jekyll's format, e.g. `2022-11-07 8:00:00 -0300`.
pub fn parse_date(s: &str) -> Result<DateTime<Utc>> {
    let date = DateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S %z")
        .with_context(|| format!("invalid date {s:?}"))?;
    Ok(date.with_timezone(&Utc))
}

pub fn load_posts(dir: &Path) -> Result<Vec<Post>> {
    let mut posts = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let post = load_post(&path).with_context(|| format!("loading {}", path.display()))?;
        posts.push(post);
    }
    // Newest first.
    posts.sort_by_key(|p| std::cmp::Reverse(p.date));
    Ok(posts)
}

fn load_post(path: &Path) -> Result<Post> {
    let name = path.file_stem().unwrap().to_string_lossy().into_owned();
    // The slug is the file name without the `YYYY-MM-DD-` prefix.
    let slug = name
        .get(11..)
        .filter(|_| name.as_bytes().get(10) == Some(&b'-'))
        .ok_or_else(|| anyhow!("post file name must start with YYYY-MM-DD-"))?;
    let text = std::fs::read_to_string(path)?;
    let (fm, body) = split_front_matter(&text)?;
    let Some(title) = fm.title else {
        bail!("missing title");
    };
    let date = match &fm.date {
        Some(date) => parse_date(date)?,
        None => parse_date(&format!("{} 00:00:00 +0000", &name[..10]))?,
    };
    let modified_date = fm.modified_date.as_deref().map(parse_date).transpose()?;
    // Like Jekyll's default permalink `/:year/:month/:day/:title.html`, which
    // uses the date from the front matter, not from the file name.
    let url = format!("/{}/{}.html", date.format("%Y/%m/%d"), slug);
    Ok(Post {
        name,
        title,
        date,
        modified_date,
        url,
        markdown: body.to_string(),
    })
}

pub fn load_pages(dir: &Path) -> Result<Vec<Page>> {
    let mut pages = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        let page = load_page(&path).with_context(|| format!("loading {}", path.display()))?;
        pages.push(page);
    }
    pages.sort_by(|a, b| a.source.cmp(&b.source));
    Ok(pages)
}

fn load_page(path: &Path) -> Result<Page> {
    let text = std::fs::read_to_string(path)?;
    let (fm, body) = split_front_matter(&text)?;
    let stem = path.file_stem().unwrap().to_string_lossy();
    let url = match fm.permalink {
        Some(p) => p,
        None if stem == "index" => "/".to_string(),
        None => format!("/{stem}.html"),
    };
    Ok(Page {
        source: path.to_path_buf(),
        title: fm.title,
        date: fm.date.as_deref().map(parse_date).transpose()?,
        layout: fm.layout.unwrap_or_else(|| "page".into()),
        url,
        body: body.to_string(),
        is_markdown: path.extension().is_some_and(|e| e == "md"),
    })
}
