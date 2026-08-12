//! Manual visual check for the three drawing front ends.
//!
//! Renders four canonical examples (one `picture`, one `xypic`, two
//! TikZ flavours) to stdout. Paste the SVGs into a browser to verify
//! they look right.
//!
//! ```sh
//! cargo run --example dump_drawings > /tmp/drawings.svg
//! ```

use mdweb::drawing::render;

const PICTURE: &str = r"\begin{picture}(76,20)
\unitlength=1pt
\put(0,0){$A$}
\put(69,0){$B$}
\put(14,3){\line(1,0){50}}
\put(39,3){\vector(0,1){15}}
\put(14,3){\circle*{2}}
\put(64,3){\circle*{2}}
\end{picture}";

const XYPIC: &str = r"\xymatrix{
  A \ar[r]^f \ar[d]_g &
  B \ar[d]^{g'} \\
  D \ar[r]_{f'}        &
  C
}";

const TRIANGLE: &str = r"\begin{tikzpicture}\small
\def\r{1.8}
\coordinate[label=$A$] (A) at (0.5*\r,0.8*\r);
\coordinate[label=below:$B$] (B) at (-\r,0);
\coordinate[label=below:$C$] (C) at (\r,0);
\draw[thin] (A) -- node[above] {$c$}
   node[pos=0.03,below,inner sep=4] {$\alpha$}
   (B) -- (C) -- node[right] {$b$} (A);
\end{tikzpicture}";

const AXES: &str = r"\begin{tikzpicture}[domain=0:2]
\draw[very thin] (-0.1,-0.1) grid (2.1,2.1);
\draw[->] (-0.2,0)--(2.2,0) node[right] {$x$};
\draw[->] (0,-0.2)--(0,2.2) node[above] {$y$};
\draw plot (\x,{sin(\x r)}) node[right] {$y=\sin x$};
\draw[color=blue] plot (\x,\x) node[right] {$y=x$};
\end{tikzpicture}";

fn dump(label: &str, src: &str) {
    println!("\n=== {label} ===");
    match render(src) {
        Some(svg) => {
            let m = svg.matches("M ").count();
            let l = svg.matches(" L ").count();
            let p = svg.matches("<path").count();
            let t = svg.matches("<text").count();
            println!("<{m} M, {l} L, {p} paths, {t} texts>");
            println!("{svg}");
        }
        None => println!("(render returned None)"),
    }
}

fn main() {
    dump("picture", PICTURE);
    dump("xypic", XYPIC);
    dump("tikz triangle", TRIANGLE);
    dump("tikz axes", AXES);
}
