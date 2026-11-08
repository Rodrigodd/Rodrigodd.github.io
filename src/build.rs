use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use minijinja::value::Serde;
use minijinja::{AutoEscape, Environment, Value, context};
use serde::Serialize;
use serde_json::json;
use walkdir::WalkDir;

use crate::config::Config;
use crate::content::{self, Post};
use crate::highlight::{self, escape_html};
use crate::{feed, markdown, reading_time};

pub struct RenderedPost {
    pub post: Post,
    pub html: String,
    pub excerpt: String,
}

#[derive(Serialize)]
struct NavPage {
    title: String,
    url: String,
}

/// Metadata for search engines and social media, like the old jekyll-seo-tag.
#[derive(Serialize)]
struct Seo {
    title: String,
    og_title: String,
    description: Option<String>,
    url: String,
    og_type: &'static str,
    published_time: Option<String>,
    json_ld: String,
}

fn seo(
    config: &Config,
    title: Option<&str>,
    url: &str,
    date: Option<DateTime<Utc>>,
    description: Option<&str>,
) -> Seo {
    let url = format!("{}{}", config.url, url);
    let is_home = url == format!("{}/", config.url);
    let og_title = title.unwrap_or(&config.title).to_string();
    let full_title = match title {
        Some(t) => format!("{t} | {}", config.title),
        None => config.title.clone(),
    };
    let published_time = date.map(|d| d.to_rfc3339());

    let mut ld = json!({
        "@context": "https://schema.org",
        "headline": og_title,
        "url": url,
    });
    if is_home {
        ld["@type"] = json!("WebSite");
        ld["name"] = json!(config.title);
    } else if let Some(date) = &published_time {
        ld["@type"] = json!("BlogPosting");
        ld["datePublished"] = json!(date);
        ld["dateModified"] = json!(date);
        ld["mainEntityOfPage"] = json!({ "@type": "WebPage", "@id": url });
    } else {
        ld["@type"] = json!("WebPage");
    }
    if let Some(d) = description {
        ld["description"] = json!(d);
    }

    Seo {
        title: full_title,
        og_title,
        description: description.map(str::to_string),
        og_type: if published_time.is_some() {
            "article"
        } else {
            "website"
        },
        published_time,
        // Escape `</` so the JSON can't close the script tag.
        json_ld: ld.to_string().replace("</", "<\\/"),
        url,
    }
}

fn template_env(root: &Path) -> Result<Environment<'static>> {
    let mut env = Environment::new();
    env.set_auto_escape_callback(|name| {
        if name.ends_with(".html") {
            AutoEscape::Html
        } else {
            AutoEscape::None
        }
    });
    // minijinja's default HTML escaping also escapes `/`, which makes URLs
    // unreadable in the output, so use our own escaping.
    env.set_formatter(|out, state, value| {
        if *state.auto_escape() == AutoEscape::None || value.is_safe() {
            return minijinja::escape_formatter(out, state, value);
        }
        if value.is_undefined() || value.is_none() {
            return Ok(());
        }
        let s = match value.as_str() {
            Some(s) => escape_html(s),
            None => escape_html(&value.to_string()),
        };
        out.write_str(&s).map_err(minijinja::Error::from)
    });
    env.add_filter(
        "date",
        |value: String, format: String| -> Result<String, minijinja::Error> {
            let date = DateTime::parse_from_rfc3339(&value).map_err(|e| {
                minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
            })?;
            Ok(date.format(&format).to_string())
        },
    );

    let dir = root.join("templates");
    for entry in WalkDir::new(&dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry
            .path()
            .strip_prefix(&dir)?
            .to_string_lossy()
            .replace('\\', "/");
        let source = std::fs::read_to_string(entry.path())?;
        env.add_template_owned(name.clone(), source)
            .with_context(|| format!("parsing template {name}"))?;
    }
    Ok(env)
}

fn write(out: &Path, url: &str, contents: &str) -> Result<()> {
    let mut path = out.join(url.trim_start_matches('/'));
    if url.ends_with('/') {
        path.push("index.html");
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    for entry in WalkDir::new(from) {
        let entry = entry?;
        let target = to.join(entry.path().strip_prefix(from)?);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            std::fs::copy(entry.path(), &target)
                .with_context(|| format!("copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// Build the site in `root` into the `out` directory. `dev_script` is HTML
/// injected at the end of every page, used by the dev server for live reload.
pub fn build(root: &Path, out: &Path, dev_script: Option<&str>) -> Result<()> {
    let config = Config::load(&root.join("site.toml"))?;
    let posts = content::load_posts(&root.join("posts"))?;
    let pages = content::load_pages(&root.join("pages"))?;
    let env = template_env(root)?;

    // Links to other Markdown files are resolved by file name.
    let link_targets: Vec<(String, String)> = posts
        .iter()
        .map(|p| (format!("{}.md", p.name), p.url.clone()))
        .chain(pages.iter().map(|p| {
            (
                p.source.file_name().unwrap().to_string_lossy().into_owned(),
                p.url.clone(),
            )
        }))
        .collect();
    let resolve_link = |target: &str| {
        let name = target.rsplit('/').next().unwrap();
        link_targets
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, url)| url.clone())
    };

    let posts: Vec<RenderedPost> = posts
        .into_iter()
        .map(|post| {
            let r = markdown::render(&post.markdown, &resolve_link)
                .with_context(|| format!("rendering post {}", post.name))?;
            Ok(RenderedPost {
                post,
                html: r.html,
                excerpt: r.excerpt,
            })
        })
        .collect::<Result<_>>()?;

    if out.exists() {
        std::fs::remove_dir_all(out)?;
    }
    std::fs::create_dir_all(out)?;
    copy_dir(&root.join("static"), out)?;

    let css = grass::from_path(
        root.join("sass/main.scss"),
        &grass::Options::default().load_path(root.join("sass")),
    )
    .map_err(|e| anyhow::anyhow!("compiling sass: {e}"))?;
    write(out, "/assets/css/style.css", &css)?;
    write(out, "/assets/css/highlight.css", &highlight::css()?)?;

    let nav_pages: Vec<NavPage> = pages
        .iter()
        .filter_map(|p| {
            Some(NavPage {
                title: p.title.clone()?,
                url: p.url.clone(),
            })
        })
        .collect();
    let site = Value::from(Serde(&config));
    let nav_pages = Value::from(Serde(&nav_pages));
    let post_list: Vec<Value> = posts
        .iter()
        .map(|p| context! { title => p.post.title.clone(), url => p.post.url.clone(), date => p.post.date.to_rfc3339() })
        .collect();

    let render = |layout: &str, page: Value, seo: Seo| -> Result<String> {
        let template = env.get_template(&format!("{layout}.html"))?;
        let html = template.render(context! {
            site => site.clone(),
            nav_pages => nav_pages.clone(),
            posts => post_list.clone(),
            page => page,
            seo => Value::from(Serde(&seo)),
            dev_script => dev_script,
        })?;
        Ok(html)
    };

    for p in &posts {
        let post = &p.post;
        let page = context! {
            title => post.title.clone(),
            url => post.url.clone(),
            date => post.date.to_rfc3339(),
            modified_date => post.modified_date.map(|d| d.to_rfc3339()),
            reading_time => reading_time::minutes(&p.html),
            content => p.html.clone(),
        };
        let seo = seo(
            &config,
            Some(&post.title),
            &post.url,
            Some(post.date),
            Some(&p.excerpt),
        );
        let html = render("post", page, seo).with_context(|| format!("rendering {}", post.url))?;
        write(out, &post.url, &html)?;
    }

    for page in &pages {
        let content = if page.is_markdown {
            markdown::render(&page.body, &resolve_link)?.html
        } else {
            page.body.clone()
        };
        let ctx = context! {
            title => page.title.clone(),
            url => page.url.clone(),
            content => content,
        };
        let seo = seo(&config, page.title.as_deref(), &page.url, page.date, None);
        let html = render(&page.layout, ctx, seo)
            .with_context(|| format!("rendering {}", page.source.display()))?;
        write(out, &page.url, &html)?;
    }

    write(out, "/feed.xml", &feed::atom(&config, &posts))?;
    Ok(())
}
