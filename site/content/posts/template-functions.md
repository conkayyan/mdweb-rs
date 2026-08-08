---
title: "Template expressions: conditions, filters and loops"
date: "2026-08-06"
updated: "2026-08-08"
tags: ["tutorial", "theme", "template"]
---

Theme files under `template/default/` are small templates. Besides the
basic `{{ var }}` output and `{% for %}`, they now support comparisons,
`and` / `or` / `not`, string filters and ordered loops. Every example
below is used verbatim on this site's pages.

## 1. `{% if %}` with comparisons

```html
{% if article.reading_minutes >= 5 %}
  <p>Long read — grab a coffee.</p>
{% else %}
  <p>Quick read.</p>
{% endif %}

{% if article.lang == "en" and article.tags | length > 2 %}
  <p>English post with several tags.</p>
{% endif %}
```

Supported operators: `== != < > <= >=`, plus `and`, `or`, `not`.
Empty strings, `0`, empty arrays and missing values are all falsy.

## 2. String filters

```html
{{ article.title | truncate:20:... }}      <!-- first 20 chars + "..." -->
{{ article.summary | slice:0:40 }}         <!-- characters 0..39 -->
{{ article.summary | replace:"mdweb":"md" | lower }}
{{ "  spaced  " | trim | upper }}
{{ article.tags | length }}                 <!-- count of items -->
```

- `truncate:N` keeps N characters and appends the suffix (default `…`);
  give your own third argument to change it: `truncate:20:...`.
- `slice:start:len` cuts out characters.
- `replace:"from":"to"` replaces every occurrence.
- `date:"%Y年%m月%d日"` reformats a date string (see below).

## 3. Date formatting

```html
{{ article.date_iso | date:"%Y-%m-%d" }}    <!-- 2026-08-08 -->
{{ article.date_iso | date:"%B %d, %Y" }}   <!-- August 08, 2026 -->
{{ article.date_iso | date:"%A" }}           <!-- Saturday -->
```

Tokens: `%Y %y %m %e %d %H %I %M %S %p`, `%a %A %b %B %j %w %u %V %%`.

## 4. Loops with filtering, ordering and slicing

A `{% for %}` iterates over the array produced by its expression, so
you can combine the array filters inline:

```html
{% for post in recent | sort:title | limit:3 %}
  <a href="{{ post.url }}">{{ post.title }}</a>
{% endfor %}

{% for tag in tags | sort_desc:count | limit:5 %}
  <span>{{ tag.name }} ({{ tag.count }})</span>
{% endfor %}
```

- `sort:field` / `sort_desc:field` order array items by a key; without
  a field they order scalar items.
- `limit:n` keeps the first n, `offset:n` skips n, `reverse` flips order.
- Inside a loop, `{{ x_index }}` (0-based) and `{{ x_length }}` are set.

## 5. DIY page styles

A unique-looking page is just a template choice: create any
`template/default/*.html` (e.g. `banner.html`) extending the base, then
render it with `{{ content | safe }}`. Give pages custom rows in
frontmatter and read them as `article.extra.<field>` — no Rust required.

Try editing `template/default/layout/side.html` or `article.html` and
refreshing the browser: mdweb re-reads templates on every request.