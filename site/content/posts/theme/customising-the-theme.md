---
title: "Customising the theme"
date: "2026-07-30"
tags: ["tutorial", "theme"]
---

Override individual files under `template/default/`. mdweb loads
anything you ship there on top of the embedded defaults, so a
single `layout/header.html` is enough to recolour the navigation.

Slots available: `header`, `footer`, `side`, `inject`. Use
`inject.html` to add analytics snippets before `</head>`.

## Images

Files under `template/default/static/` are served at
`/static/<path>`. Reference them from your own CSS or content:

```css
/* static/hero.png is served at /static/hero.png */
.hero {
  background-image: url("/static/hero.png");
  background-size: cover;
  min-height: 240px;
}
```

In markdown use the same prefix: `![Alt](/static/hero.png)`.

That prefix is for theme-wide artwork. Images belonging to a single
document go in an `_image/` folder beside it — see
[Embedding images](/posts/guide/embedding-images/).