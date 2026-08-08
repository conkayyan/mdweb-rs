---
title: "插入图片"
date: "2026-08-06"
tags: ["tutorial", "images"]
---

把图片放进文档旁边的 `_image/` 目录。`content/` 下的任意目录都可以有一个:

```text
content/posts/guide/
├── _image/
│   └── hero.svg
└── embedding-images.zh.md
```

用普通的相对路径引用它——和你直接在编辑器里打开这个 `.md` 时的写法完全一样:

![mdweb 横幅](_image/hero.svg)

渲染结果是 `<img src="/posts/guide/hero.svg">`,URL 里的 `_image` 这一段被去掉了。
这个约定的意义就在于**两头都不误**:编辑器预览按磁盘上的相对路径找图,
站点则按改写后的 URL 提供。

## 跨目录引用

相对路径可以用 `../` 跨目录,所以公用素材放在它该在的地方就行。
下面这张来自 `content/pages/_image/`:

![公用 logo](../../pages/_image/logo.svg)

改写后的 URL 是 `/pages/logo.svg`。如果 `../` 爬出了 `content/`,路径会原样保留
并最终 404 —— 所有内容都只能待在 content 目录内。

## 控制尺寸

markdown 没有表示尺寸的语法,需要时直接写 HTML,`src` 一样会被改写:

<img src="_image/hero.svg" alt="半宽横幅" width="160">

## 规则

- 图片必须**直接**放在 `_image/` 里。需要分子目录时,给子目录自己建一个 `_image/`,
  而不是在 `_image/` 内部再套目录。
- 已经是绝对路径(`/static/hero.png`)、外链(`https://…`)或 data URI 的,原样保留。
- 图片与语言无关:`/posts/guide/hero.svg` 和 `/zh/posts/guide/hero.svg` 是同一个文件。
- 文件名里避免空格以及 `?`、`#`。

如果是属于主题而非某篇文档的全站素材——logo、背景、favicon——请放
`template/<theme>/static/` 并用 `/static/` 前缀。
