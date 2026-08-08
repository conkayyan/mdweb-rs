---
title: "模板表达式：条件、过滤器与循环"
date: "2026-08-06"
updated: "2026-08-08"
tags: ["tutorial", "theme", "template"]
---

`template/default/` 下的主题文件是小型模板。除了基本的 `{{ var }}`
输出和 `{% for %}`，现在支持比较运算、`and` / `or` / `not`、字符串
过滤器与带排序的循环。下面每个例子都可在本站页面中直接使用。

## 1. 带比较运算的 `{% if %}`

```html
{% if article.reading_minutes >= 5 %}
  <p>长文，请准备好咖啡。</p>
{% else %}
  <p>短文。</p>
{% endif %}

{% if article.lang == "en" and article.tags | length > 2 %}
  <p>英文文章，且有多个标签。</p>
{% endif %}
```

支持的运算符：`== != < > <= >=`，以及 `and`、`or`、`not`。
空字符串、`0`、空数组、缺失值均视为假。

## 2. 字符串过滤器

```html
{{ article.title | truncate:20:... }}      <!-- 前 20 个字符 + "..." -->
{{ article.summary | slice:0:40 }}         <!-- 第 0..39 个字符 -->
{{ article.summary | replace:"mdweb":"md" | lower }}
{{ "  spaced  " | trim | upper }}
{{ article.tags | length }}                 <!-- 元素个数 -->
```

- `truncate:N` 保留 N 个字符并在末尾追加后缀（默认 `…`）；第三个参数
  可自定义后缀：`truncate:20:...`。
- `slice:start:len` 按字符切割。
- `replace:"旧":"新"` 替换全部出现的子串。
- `date:"%Y年%m月%d日"` 格式化日期字符串（见下）。

## 3. 日期格式化

```html
{{ article.date_iso | date:"%Y-%m-%d" }}    <!-- 2026-08-08 -->
{{ article.date_iso | date:"%B %d, %Y" }}   <!-- August 08, 2026 -->
{{ article.date_iso | date:"%A" }}           <!-- Saturday -->
```

可用记号：`%Y %y %m %e %d %H %I %M %S %p`、`%a %A %b %B %j %w %u %V %%`。

## 4. 带过滤、排序与截取（top N）的循环

`{% for %}` 遍历其表达式产生的结果数组，因此可以在循环内联使用
数组过滤器：

```html
{% for post in recent | sort:title | limit:3 %}
  <a href="{{ post.url }}">{{ post.title }}</a>
{% endfor %}

{% for tag in tags | sort_desc:count | limit:5 %}
  <span>{{ tag.name }} ({{ tag.count }})</span>
{% endfor %}
```

- `sort:field` / `sort_desc:field` 按字段对元素排序；不带字段时对
  标量元素排序。
- `limit:n` 只保留前 n 个，`offset:n` 跳过前 n 个，`reverse` 反转顺序。
- 循环内自动提供 `{{ x_index }}`（从 0 开始）和 `{{ x_length }}`。

## 5. 自定义页面风格（DIY）

要做出独一无二的页面只需要换模板：在 `template/default/` 下新建
任意 `*.html`（例如 `banner.html`）并继承 base，再用
`{{ content | safe }}` 渲染。页面的自定义字段写在 frontmatter 里，
模板中用 `article.extra.<字段>` 读取——无需修改任何 Rust 代码。

试着改一改 `template/default/layout/side.html` 或 `article.html`，
然后刷新浏览器：mdweb 每个请求都会重新读取模板。