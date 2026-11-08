//! Compares the generated site with a snapshot of the old Jekyll site, in
//! `tests/baseline/`, to make sure that nothing that readers depend on breaks:
//! - every page of the old site still exists, at the same URL;
//! - feed entries keep their id and dates, otherwise feed readers would show
//!   old posts as new or updated;
//! - links to sections of a page (`page.html#section`) still work;
//! - there are no broken internal links.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;
use walkdir::WalkDir;

pub fn check(out: &Path, baseline: &Path) -> Result<bool> {
    let mut ok = true;

    println!("== pages");
    for entry in WalkDir::new(baseline) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(baseline)?;
        let Ok(new) = std::fs::read_to_string(out.join(rel)) else {
            println!("ERROR: missing {}", rel.display());
            ok = false;
            continue;
        };
        if rel.extension().is_some_and(|e| e == "html") {
            let old = std::fs::read_to_string(entry.path())?;
            ok &= check_ids(&rel.display().to_string(), &old, &new);
        }
    }

    println!("== feed");
    ok &= check_feed(out, baseline)?;

    println!("== links");
    ok &= check_links(out)?;

    println!();
    println!("{}", if ok { "check passed" } else { "check FAILED" });
    Ok(ok)
}

fn ids(html: &str) -> BTreeSet<String> {
    static RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r#"\sid="([^"]*)""#).unwrap());
    RE.captures_iter(html).map(|c| c[1].to_string()).collect()
}

/// Check that the ids of the old page, which may be the target of links, still
/// exist. Footnote ids and the id of the old mobile menu are an exception, as
/// they are not linked from outside.
fn check_ids(page: &str, old: &str, new: &str) -> bool {
    let new_ids = ids(new);
    let missing: Vec<_> = ids(old)
        .into_iter()
        .filter(|id| !id.starts_with("fn:") && !id.starts_with("fnref:") && id != "nav-trigger")
        .filter(|id| !new_ids.contains(id))
        .collect();
    if missing.is_empty() {
        println!("ok: {page}");
        true
    } else {
        println!("ERROR: {page}: missing ids {missing:?}");
        false
    }
}

#[derive(Debug, PartialEq)]
struct FeedEntry {
    title: String,
    link: String,
    published: String,
    updated: String,
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
            },
        );
    }
    (meta, entries)
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

fn broken_links(site: &Path) -> Result<BTreeSet<String>> {
    let mut ids: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut links = Vec::new();
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
        ids.insert(rel.clone(), self::ids(&text));
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

fn check_links(out: &Path) -> Result<bool> {
    let broken = broken_links(out)?;
    for link in &broken {
        println!("ERROR: broken link: {link}");
    }
    if broken.is_empty() {
        println!("ok: no broken internal links");
    }
    Ok(broken.is_empty())
}
