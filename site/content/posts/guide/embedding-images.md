---
title: "Embedding images"
date: "2026-08-06"
tags: ["tutorial", "images"]
---

Put images in an `_image/` folder next to the document that uses them.
Any directory under `content/` can have one:

```text
content/posts/guide/
├── _image/
│   └── hero.svg
└── embedding-images.md
```

Reference it by ordinary relative path, exactly as you would if you
were just opening the `.md` in an editor:

![The mdweb banner](_image/hero.svg)

That renders as `<img src="/posts/guide/hero.svg">` — the `_image`
segment is dropped from the URL. The point of the convention is that
**both views work**: your editor's preview follows the relative path on
disk, and the site serves the rewritten one.

## Referencing another folder

Relative paths may cross directories with `../`, so shared artwork can
live wherever it belongs. This one comes from `content/pages/_image/`:

![Shared logo](../../pages/_image/logo.svg)

The rewritten URL is `/pages/logo.svg`. Paths that would climb out of
`content/` are left untouched and will 404 — everything stays inside the
content tree.

## Controlling size

Markdown has no syntax for dimensions, so drop to HTML when you need it.
The `src` is rewritten the same way:

<img src="_image/hero.svg" alt="Half-width banner" width="160">

## Rules

- The image must sit **directly** inside `_image/`. For sub-folders, give
  the sub-folder its own `_image/` rather than nesting inside one.
- Paths that are already absolute (`/static/hero.png`), external
  (`https://…`) or data URIs are passed through unchanged.
- Images are language-neutral: `/posts/guide/hero.svg` and
  `/zh/posts/guide/hero.svg` serve the same file.
- Avoid spaces and `?` or `#` in filenames.

For site-wide artwork that belongs to the theme rather than to any one
document — logos, backgrounds, favicons — use `template/<theme>/static/`
and the `/static/` prefix instead.
