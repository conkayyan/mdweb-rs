---
title: "Writing your first post"
date: "2026-08-03"
tags: ["tutorial", "content"]
---

Posts live under `content/posts/` as plain Markdown. The directory
becomes a category, subdirectories become subcategories. Filenames
like `hello.zh.md` register as a Chinese variant of `hello.md`.

Frontmatter accepts `title`, `date`, `updated`, `author`, `tags`,
`summary`, and arbitrary `extra` keys exposed to templates.

A few conventions worth knowing:

- Use double quotes around dates in frontmatter: `date: "2026-08-03"`.
- Name files like `foo.en.md` / `foo.zh.md` for translations — the
  `.en` / `.zh` segment registers the language variant.
- `draft: true` hides the post from listings and feeds while you work.