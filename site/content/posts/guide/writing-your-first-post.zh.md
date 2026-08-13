---
title: "撰写第一篇文章"
date: "2026-08-03"
tags: ["tutorial", "content"]
---

文章放在 `content/posts/` 目录下，就是普通 Markdown。目录即分类，
子目录即子分类。形如 `hello.zh.md` 的文件名注册为 `hello.md` 的
中文版本。

frontmatter 支持 `title`、`date`、`updated`、`author`、`tags`、
`summary`，以及任意 `extra` 字段（可在模板里取到）。

几个值得了解的小约定：

- frontmatter 里的日期请用双引号包裹：`date: "2026-08-03"`。
- 文件命名为 `foo.en.md` / `foo.zh.md` 即可作为不同语言版本，
  `.en` / `.zh` 段即注册为语言变体。
- `draft: true` 会在写作时把文章从列表与 feed 中隐藏。