The source code for my blog: [rodrigodd.github.io](https://rodrigodd.github.io/)

The site is built by a small static site generator written in Rust, in `src/`.

# Running locally

You need [Rust](https://rustup.rs/). Then:

```
cargo run -- serve
```

This builds the site, serves it on http://localhost:4000, and rebuilds and
reloads the page whenever a file changes. Use `--bind <ip>` (repeatable) and
`--port <port>` to listen on other addresses, e.g. `cargo run -- serve --bind
0.0.0.0` to open the site from another device on the network.

To only build the site into `_site/`, run `cargo run --release -- build`.

# Organization

- `site.toml`: site configuration (title, URL, social links, ...).
- `posts/`: the blog posts, in Markdown. The file name is `YYYY-MM-DD-slug.md`,
  and the post URL is `/YYYY/MM/DD/slug.html`, using the `date` from the front
  matter. Link to another post using its file name, e.g.
  `[part 1](2022-10-16-bf_compiler-part1.md)`. Don't change the `date` or the
  file name of a published post: they define its URL and its id in the feed, and
  feed readers would show it as a new post.
- `pages/`: other pages (Markdown or HTML), with a `layout` in the front matter.
- `templates/`: [minijinja](https://docs.rs/minijinja) templates.
- `sass/`: the stylesheet, including the vendored [minima](https://github.com/jekyll/minima) theme.
- `static/`: files copied as they are (images, favicon, ...).
- `src/`: the generator:
  - `markdown.rs`: Markdown to HTML, using [comrak](https://docs.rs/comrak);
  - `highlight.rs`: syntax highlighting with [arborium](https://github.com/bearcove/arborium)
    (to support another language, enable its `lang-*` feature in `Cargo.toml`),
    and `highlight/fallback.rs`: hand-written highlighters for languages that
    arborium doesn't support (Brainfuck and Cranelift IR);
  - `feed.rs`: the Atom feed;
  - `build.rs`, `serve.rs`: the commands.

The site is deployed to GitHub Pages by `.github/workflows/deploy.yml` on every
push to `master`.
