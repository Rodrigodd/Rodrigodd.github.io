The source code for my blog: [rodrigodd.github.io](https://rodrigodd.github.io/)

The site is built by a small static site generator written in Rust, in `src/`.

# Running locally

You need [Rust](https://rustup.rs/). Then:

```
cargo run -- serve
```

This builds the site, serves it on http://localhost:4000, and rebuilds and
reloads the page whenever a file changes.

Other commands:

- `cargo run --release -- build`: build the site into `_site/`.
- `cargo run -- check`: compare `_site/` with `tests/baseline/`, a snapshot of
  the site when it was built with Jekyll. It fails if a page of the old site is
  missing, if a section id of an old page (the target of `page.html#section`
  links) is gone, if an internal link is broken, or if a feed entry changed its
  id or dates (feed readers would show it as a new or updated post).

# Organization

- `site.toml`: site configuration (title, URL, social links, ...).
- `posts/`: the blog posts, in Markdown. The file name is `YYYY-MM-DD-slug.md`,
  and the post URL is `/YYYY/MM/DD/slug.html`, using the `date` from the front
  matter. Link to another post using its file name, e.g.
  `[part 1](2022-10-16-bf_compiler-part1.md)`.
- `pages/`: other pages (Markdown or HTML), with a `layout` in the front matter.
- `templates/`: [minijinja](https://docs.rs/minijinja) templates.
- `sass/`: the stylesheet, including the vendored [minima](https://github.com/jekyll/minima) theme.
- `static/`: files copied as they are (images, favicon, ...).
- `src/`: the generator:
  - `markdown.rs`: Markdown to HTML, compatible with kramdown (attribute lists,
    smart quotes, header ids, footnotes);
  - `highlight.rs`: syntax highlighting with [arborium](https://github.com/bearcove/arborium)
    (to support another language, enable its `lang-*` feature in `Cargo.toml`);
  - `feed.rs`: the Atom feed;
  - `build.rs`, `serve.rs`, `check.rs`: the commands.

The site is deployed to GitHub Pages by `.github/workflows/deploy.yml` on every
push to `master`.
