---
title: "配置、RSS 与 SEO"
date: "2026-08-08"
updated: "2026-08-09"
tags: ["tutorial", "config", "seo"]
---

mdweb 的大部分行为都由项目根目录下的单个 `site.toml` 控制。设置标题、
基础 URL、作者、默认语言与主题：

```toml
title = "My Blog"
base_url = "http://localhost:8080"
author = "Jane Doe"
language = "en"
languages = ["en", "zh"]
theme = "default"
```

## 各语言字符串

本地化标题与描述放在 `[lang.<code>]` 之下，界面文案放在
`[i18n.<code>]` 之下。每个配置的语言都有独立的 URL 前缀（`zh` 对应
`/zh/…`）：

```toml
[lang.zh]
title = "我的博客"
display_name = "简体中文"
description = "使用 mdweb 构建的演示站点，支持多语言。"
keywords = "博客, rust"

[i18n.zh]
home     = "首页"
categories = "分类"
```

## 路由规则

`[routes]` 表重命名常见的路由。改一个名字会同步更新服务器、生成的
链接、底部/主题模板，以及 feed、站点地图与搜索索引的 URL：

```toml
[routes]
search = "search"
rss    = "rss.xml"
posts  = "posts"
pages  = "pages"
```

## RSS、站点地图与 SEO

mdweb 自动生成 `/rss.xml`、`/sitemap.xml`，并在 `<head>` 中放置
`<link rel=alternate>`。通过 `site.toml` 中的 `show_rss` 和
`show_sitemap` 控制底部链接显示。站点地图覆盖所有语言下的所有
页面、文章和分类。

## 列表分页

`home_limit`、`category_limit`、`pages_limit` 与 `tags_limit` 控制对应
列表的分页大小，设为 `0` 即关闭分页。`show_tag_cloud` 控制侧边栏的
标签云是否显示。

## 统计与安全

在 `[analytics.google]` 或 `[analytics.baidu]` 下设置 `id` 即可向
`<head>` 注入对应的统计脚本。安全响应头（`X-Content-Type-Options`、
`X-Frame-Options`、`Referrer-Policy`、CSP）默认开启，可在 `[security]`
下调整。