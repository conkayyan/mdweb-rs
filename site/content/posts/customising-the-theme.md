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