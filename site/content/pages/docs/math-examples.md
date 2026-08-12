---
title: "Math examples"
summary: "A gallery of formulas and diagrams the build-time renderer handles."
---

A tour of what the built-in math and drawing renderers can do. Everything
below is compiled to inline SVG at build time — no MathJax, no JavaScript.
Formulas go through the LaTeX subset in `tex.rs`; the `picture`, xy-pic and
TikZ environments are drawn by a separate graphics engine, but they share
the same `$…$` / `$$…$$` inline entry point so they sit next to the
equations. Display formulas use a fenced ` ```math ` block, the
`\[…\]` LaTeX delimiters, or a multi-line `$$\n…\n$$` block.

## Boxed integrals with limits

```math
\boxed{ \int\limits_{-\infty}^{\infty} e^{-x^2} \, dx = \sqrt{\pi} }
```

## Limits and sums

```math
\gamma \overset{\text{def}}{=}
\lim\limits_{n \to \infty}
  \left(
     \sum\limits_{k=1}^n {1 \over k}
     - \ln n
  \right)
\approx 0.577
```

## Multi-line aligned equations

```math
\begin{align*}
 y &= x^4 + 4 =\\
   &= (x^2+2)^2 - 4x^2 \le\\
   &\le (x^2+2)^2
\end{align*}
```

## Matrices

```math
A_{m,n} = \begin{pmatrix}
a_{1,1} & a_{1,2} & \cdots & a_{1,n} \\
a_{2,1} & a_{2,2} & \cdots & a_{2,n} \\
\vdots  & \vdots  & \ddots & \vdots  \\
a_{m,1} & a_{m,2} & \cdots & a_{m,n}
\end{pmatrix}
```

## Continued fractions

```math
e = 2 + \cfrac{1}{
  1 + \cfrac{1}{
  2 + \cfrac{1}{
  3 + \cfrac{3}{
  4 + \cfrac{4}{\ldots}
}}}}
```

## picture environment

Classic LaTeX `picture`: two labelled endpoints $A$ and $B$ joined by a
horizontal line with dots at each end, plus a vertical arrow rising from
the middle:

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

## xy-pic diagrams

A commutative square with two arrows on `A`, one on `B`, and one on `D`,
each carrying its own label position (`^` above, `_` below, `|` inline):

```math
\xymatrix{
  A \ar[r]^f \ar[d]_g &
  B \ar[d]^{g'} \\
  D \ar[r]_{f'}        &
  C
}
```

## TikZ graphics

A triangle whose three nodes are placed by name and connected with
`--`. The `AB` edge carries two labels: an `above` `$c$` for the side
length and a `pos=0.03,below` `$\alpha$` placed near the `$A$` vertex:

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

## TikZ plots

A fine grid plus two coordinate axes, with `plot (\x,{sin(\x r)})` for
the sine curve and a second blue `plot (\x,\x)` for the line `y = x`.
Both plots share `domain=0:2`, set on the environment:

```math
\begin{tikzpicture}[domain=0:2]
\draw[very thin] (-0.1,-0.1) grid (2.1,2.1);
\draw[->] (-0.2,0)--(2.2,0) node[right] {$x$};
\draw[->] (0,-0.2)--(0,2.2) node[above] {$y$};
\draw plot (\x,{sin(\x r)}) node[right] {$y=\sin x$};
\draw[color=blue] plot (\x,\x) node[right] {$y=x$};
\end{tikzpicture}
```

## Math inside HTML

Raw HTML blocks are passed through as-is, but their `$$…$$` (inline) and
`\[…\]` (display) spans are still rendered — so formulas inside a `<p>` or
`<div>` wrapper get typeset. This reproduces the classic Jackson
electrodynamics example:

<p>Placed in the origin, magnetic moment $$\vec{\mathfrak{m}}$$ produces at point $$\vec{R}_0$$ magnetic vector potential</p>

<p>$$\vec{A} = {\vec{\mathfrak{m}}
\times \vec{R}_0 \over R_0^3}.$$(1)</p>