---
title: "About"
summary: "What this demo site is about."
---

This demo shows how mdweb renders a doc directory into a blog.

- Markdown files under `posts/<category>/` become articles. The folder
  name is the category — bare files directly in `posts/` are not part
  of any feed.
- Folders under `posts/` become the categories shown in the header's
  "Categories" dropdown.
- Everything that is **not** a post lives under `pages/`. The first-level
  sub-directories of `pages/` (`pages/docs/`, …) and any standalone
  `.md` files at `pages/`'s root are the page sections that populate the
  "Pages" dropdown.
- File names like `hello.zh.md` become language variants.

The `pages/_image/` directory holds images used by `pages/*.md`, and
each `pages/<section>/_image/` directory serves the same role for that
section's documents. Both render-time and disk-view paths resolve the
same way, so opening a `.md` file in an editor and viewing the
generated HTML both work without changes.

Start with the [Beginner's guide](/posts/guide/) to see the whole
workflow in order.