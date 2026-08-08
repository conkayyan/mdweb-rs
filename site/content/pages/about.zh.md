---
title: "关于"
summary: "本演示站点的简介。"
---

本演示展示了 mdweb 如何把一个文档目录渲染成博客。

- `posts/<分类>/` 下的 Markdown 文件是文章。直接放在 `posts/` 根下
  的文件不参与时间线。
- `posts/` 的子文件夹是分类，对应导航里的「分类」下拉。
- **所有不是文章的内容都放在 `pages/` 下。** `pages/` 的直接子目录
  （`pages/notes/`、`pages/docs/` 等）以及 `pages/` 根下的 `.md` 文件
  是导航里「Pages / 页面」下拉的条目。
- 像 `hello.zh.md` 这样的文件名代表不同语言版本。

`pages/_image/` 目录放 `pages/*.md` 共用的图片，每个
`pages/<章节>/_image/` 则是该章节文档专用的图床。本地直接打开
`.md` 与浏览生成的网页效果一致。