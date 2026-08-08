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