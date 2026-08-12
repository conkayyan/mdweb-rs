use mdweb::drawing::render;

fn main() {
    let src = r"\xymatrix{
  A \ar[r]^f \ar[d]_g &
  B \ar[d]^{g'} \\
  D \ar[r]_{f'}        &
  C
}";
    let out = render(src).expect("svg");
    println!("{out}");
}
