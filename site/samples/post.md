---
# Post title (required).
title: "Sample Post"

# Creation date (quoted strings recommended).
date: "2026-08-08"

# Last update date (optional).
updated: "2026-08-08"

# Author (optional; falls back to site.toml's author).
author: "Author Name"

# Tags (optional array of strings).
tags: ["mdweb", "rust"]

# One-line summary (optional). Shown in category listings and used as
# the default value for <meta name="description"> when meta.description
# is unset.
summary: "A one-line description."

# Draft: true hides the post from listings and feeds.
draft: false

# Arbitrary metadata exposed to templates as post.meta (optional).
meta:
  description: "A longer description for SEO."
  keywords: "post, sample, mdweb"

# Extra fields are available in templates as post.<name>.
custom_field: "any value"
---

Replace this with your post content. Posts are rendered with
`article.html` from the active theme and appear in category
listings and chronological feeds.

```rust
fn main() {
    println!("Hello, mdweb!");
}
```