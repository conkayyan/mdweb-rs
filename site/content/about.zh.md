---
title: "关于"
summary: "本演示站点的简介。"
---

本演示展示了 mdweb 如何把一个文档目录渲染成博客。

- `posts/` 下的 Markdown 文件是文章。
- `posts/` 的子文件夹是分类。
- 其它位置（如 `pages/`）的 Markdown 文件是独立页面，
  支持嵌套目录形成层级。
- 直接放在 `content/` 下的 Markdown 文件（如本页）是顶层页面，
  URL 平铺（`/about/`），导航上作为独立链接出现。
- 像 `hello.zh.md` 这样的文件名代表不同语言版本。