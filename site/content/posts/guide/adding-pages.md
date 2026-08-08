---
title: "Adding static pages"
date: "2026-08-02"
tags: ["tutorial", "pages"]
---

Anything outside `posts/` is a **page**, and pages always live under
`pages/`. Pages are the counterpart to posts:

- A sub-directory like `content/pages/docs/` becomes its own section
  (`/pages/docs/`) and lists its direct children on its landing page.
- A standalone `.md` like `content/pages/about.md` becomes a flat
  link at `/pages/about/` — perfect for one-off pages that don't
  need their own section.

The header's **Pages** dropdown surfaces each first-level child of
`pages/`, so any section you add at that level (sub-directory or
top-level `.md`) automatically becomes part of the site nav.