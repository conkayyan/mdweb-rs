---
title: "About"
summary: "What this demo site is about."
---

This demo shows how mdweb renders a doc directory into a blog.

- Markdown files under `posts/` become articles.
- Folders under `posts/` become categories.
- Markdown files anywhere else (e.g. `pages/`) become standalone pages,
  with nested folders for hierarchy.
- A Markdown file directly under `content/` (like this one) becomes a
  top-level page with a flat URL (`/about/`) and a flat nav link.
- File names like `hello.zh.md` become language variants.