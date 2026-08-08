---
title: "自定义主题"
date: "2026-07-30"
tags: ["tutorial", "theme"]
---

在 `template/default/` 下覆盖任意文件即可。mdweb 会先加载内嵌的
默认模板，再用你提供的文件覆盖，所以你只需替换 `layout/header.html`
就能重新着色导航栏。

可用插槽：`header`、`footer`、`side`、`inject`。用 `inject.html`
在 `</head>` 之前注入统计脚本。

## 图片

`template/default/static/` 下的文件会在 `/static/<路径>` 提供。
可在自己的 CSS 或正文中引用：

```css
/* static/hero.png 会以 /static/hero.png 提供 */
.hero {
  background-image: url("/static/hero.png");
  background-size: cover;
  min-height: 240px;
}
```

在 markdown 中使用同样的前缀：`![替代文字](/static/hero.png)`。

这个前缀用于全站素材。只属于某一篇文档的图片，放在它旁边的 `_image/`
目录里——见[插入图片](/zh/posts/guide/embedding-images/)。