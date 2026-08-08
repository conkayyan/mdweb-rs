---
title: "添加静态页面"
date: "2026-08-02"
tags: ["tutorial", "pages"]
---

`posts/` 之外的任何内容都是**页面**，页面统一放在 `pages/` 下。
`pages/` 与 `posts/` 是互相对称的两个顶层容器：

- 像 `content/pages/docs/` 这样的子目录成为独立的 section
  （`/pages/docs/`），落地页上自动列出它的子页面。
- 像 `content/pages/about.md` 这样的独立 `.md` 文件变成平铺链接
  `/pages/about/`——适合不需要独立分区的快捷页面。

导航里的「**页面**」下拉展示 `pages/` 的所有直接孩子，所以任何放在
这个层级的 section（子目录或顶级 `.md`）都会自动出现在站内导航里。