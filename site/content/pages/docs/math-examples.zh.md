---
title: "数学示例"
date: "2026-08-03"
updated: "2026-08-08"
author: "mdweb"
summary: "构建时数学与绘图渲染器能处理的公式与图形画廊。"
---

下面展示的是构建时编译为内联 SVG 的公式与图形画廊——无需 MathJax、无需
JavaScript。公式部分走 `tex.rs` 中的 LaTeX 子集；`picture`、xy-pic 与
TikZ 环境则由独立的绘图引擎负责，但共用同一套 `$…$` / `$$…$$` 行内
入口，因此可以与公式并排放置。块级公式使用 ` ```math ` 围栏、LaTeX
风格的 `\[…\]` 分隔符，或多行 `$$\n…\n$$` 块。

## 积分、根式与上下界

```math
\boxed{ \int\limits_{-\infty}^{\infty} e^{-x^2} \, dx = \sqrt{\pi} }
```

## 极限与求和

```math
\gamma \overset{\text{def}}{=}
\lim\limits_{n \to \infty}
  \left(
     \sum\limits_{k=1}^n {1 \over k}
     - \ln n
  \right)
\approx 0.577
```

## 多行公式

```math
\begin{align*}
 y &= x^4 + 4 =\\
   &= (x^2+2)^2 - 4x^2 \le\\
   &\le (x^2+2)^2
\end{align*}
```

## 矩阵

```math
A_{m,n} = \begin{pmatrix}
a_{1,1} & a_{1,2} & \cdots & a_{1,n} \\
a_{2,1} & a_{2,2} & \cdots & a_{2,n} \\
\vdots  & \vdots  & \ddots & \vdots  \\
a_{m,1} & a_{m,2} & \cdots & a_{m,n}
\end{pmatrix}
```

## 连分数

```math
e = 2 + \cfrac{1}{
  1 + \cfrac{1}{
  2 + \cfrac{1}{
  3 + \cfrac{3}{
  4 + \cfrac{4}{\ldots}
}}}}
```

## picture 环境

经典 LaTeX `picture` 环境：在两端点 $A$、$B$ 拉一条水平线，端点各画一个
实心点，再从水平线中央向上引一条带箭头的垂线：

```math
\begin{picture}(76,20)
\unitlength=1pt
\put(0,0){$A$}
\put(69,0){$B$}
\put(14,3){\line(1,0){50}}
\put(39,3){\vector(0,1){15}}
\put(14,3){\circle*{2}}
\put(64,3){\circle*{2}}
\end{picture}
```

## xy-pic 图示

一个交换四方图，`A` 上挂两个箭头、`B` 与 `D` 各挂一个，每条箭头
各自标注位置（`^` 在上、`_` 在下、`|` 居中）：

```math
\xymatrix{
  A \ar[r]^f \ar[d]_g &
  B \ar[d]^{g'} \\
  D \ar[r]_{f'}        &
  C
}
```

## TikZ 图形

三角形三个顶点用名字声明，再用 `--` 连线。`AB` 边上挂了两个标签：
上方的 `$c$` 表示边长，下方的 `$\alpha$` 用 `pos=0.03` 贴近 `$A$`
顶点：

```math
\begin{tikzpicture}\small
\def\r{1.8}
\coordinate[label=$A$] (A) at (0.5*\r,0.8*\r);
\coordinate[label=below:$B$] (B) at (-\r,0);
\coordinate[label=below:$C$] (C) at (\r,0);
\draw[thin] (A) -- node[above] {$c$}
   node[pos=0.03,below,inner sep=4] {$\alpha$}
   (B) -- (C) -- node[right] {$b$} (A);
\end{tikzpicture}
```

## TikZ 绘图

细网格加两条带箭头的坐标轴，第一条 `plot (\x,{sin(\x r)})` 画正弦
曲线，第二条蓝色 `plot (\x,\x)` 画直线 `y = x`。两条曲线共用
`domain=0:2`，写在 tikzpicture 环境上：

```math
\begin{tikzpicture}[domain=0:2]
\draw[very thin] (-0.1,-0.1) grid (2.1,2.1);
\draw[->] (-0.2,0)--(2.2,0) node[right] {$x$};
\draw[->] (0,-0.2)--(0,2.2) node[above] {$y$};
\draw plot (\x,{sin(\x r)}) node[right] {$y=\sin x$};
\draw[color=blue] plot (\x,\x) node[right] {$y=x$};
\end{tikzpicture}
```

## HTML 代码示例

原始 HTML 块会原样透传，但其中的 `$$…$$`（行内）与 `\[…\]`（块级）
片段仍会渲染成公式，因此写在 `<p>` 或 `<div>` 标签里的公式也会被排版。
下面是经典的 Jackson 电动力学示例：

<p>位于原点的磁矩 $$\vec{\mathfrak{m}}$$ 在 $$\vec{R}_0$$ 处产生磁矢势</p>

<p>$$\vec{A} = {\vec{\mathfrak{m}}
\times \vec{R}_0 \over R_0^3}.$$(1)</p>