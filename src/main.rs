mod build;
mod config;
mod content;
mod feed;
mod highlight;
mod markdown;
mod reading_time;
mod serve;

use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;

use anyhow::{Context, bail};

const USAGE: &str = "\
usage: site <command> [options]

commands:
  build    build the site into _site/
  serve    build, serve the site and rebuild it on changes

serve options:
  --bind <ip>    address to listen on (default: 127.0.0.1), e.g. `0.0.0.0` for
                 all IPv4 interfaces, or `::` for all IPv6 (and, on most
                 systems, IPv4) interfaces; can be repeated
  --port <port>  port to listen on (default: 4000)";

fn main() -> anyhow::Result<()> {
    let root = Path::new(".");
    let out = Path::new("_site");
    let mut args = std::env::args().skip(1);
    let command = args.next();
    match command.as_deref() {
        Some("build") => {
            build::build(root, out, None)?;
            println!("site built into {}", out.display());
        }
        Some("serve") => {
            let mut addrs = Vec::new();
            let mut port = 4000;
            while let Some(arg) = args.next() {
                let mut value = || {
                    args.next()
                        .with_context(|| format!("missing value for {arg}"))
                };
                match arg.as_str() {
                    "--bind" => {
                        let ip = value()?;
                        addrs.push(
                            ip.parse()
                                .with_context(|| format!("invalid IP address {ip:?}"))?,
                        );
                    }
                    "--port" => {
                        let p = value()?;
                        port = p.parse().with_context(|| format!("invalid port {p:?}"))?;
                    }
                    _ => bail!("unknown option {arg}\n\n{USAGE}"),
                }
            }
            if addrs.is_empty() {
                addrs.push(IpAddr::V4(Ipv4Addr::LOCALHOST));
            }
            serve::serve(root, out, &addrs, port)?
        }
        _ => bail!("{USAGE}"),
    }
    Ok(())
}
