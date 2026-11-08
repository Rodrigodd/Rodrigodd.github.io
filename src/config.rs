use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// The base URL of the site, without a trailing slash.
    pub url: String,
    #[serde(default = "default_lang")]
    pub lang: String,
    #[serde(default = "default_date_format")]
    pub date_format: String,
    /// Maximum number of posts in the feed.
    #[serde(default = "default_feed_limit")]
    pub feed_limit: usize,
    /// Links shown in the page header, before the pages.
    #[serde(default)]
    pub header_links: Vec<HeaderLink>,
    #[serde(default)]
    pub social_links: Vec<SocialLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderLink {
    pub title: String,
    pub url: String,
    /// An icon from minima-social-icons.svg, shown instead of the title.
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialLink {
    pub platform: String,
    pub user_url: String,
}

fn default_lang() -> String {
    "en".into()
}

fn default_date_format() -> String {
    "%b %-d, %Y".into()
}

fn default_feed_limit() -> usize {
    10
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }
}
