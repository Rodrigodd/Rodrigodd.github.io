mod build;
mod check;
mod config;
mod content;
mod feed;
mod highlight;
mod markdown;
mod reading_time;
mod serve;

use std::path::Path;

use anyhow::bail;

const USAGE: &str = "\
usage: site <command>

commands:
  build    build the site into _site/
  serve    build, serve on http://localhost:4000 and rebuild on changes
  check    compare _site/ with the snapshot of the old Jekyll site in tests/baseline/";

fn main() -> anyhow::Result<()> {
    let root = Path::new(".");
    let out = Path::new("_site");
    let command = std::env::args().nth(1);
    match command.as_deref() {
        Some("build") => {
            build::build(root, out, None)?;
            println!("site built into {}", out.display());
        }
        Some("serve") => serve::serve(root, out, 4000)?,
        Some("check") => {
            if !check::check(out, &root.join("tests/baseline"))? {
                std::process::exit(1);
            }
        }
        _ => bail!("{USAGE}"),
    }
    Ok(())
}
