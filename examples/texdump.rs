fn main() {
    let cases = [
        ("sum", "\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}", true),
        ("int", "\\int_a^b f(x) \\, dx", true),
        ("gamma", "\\gamma \\overset{\\mathrm{def}}{=} \\lim\\limits_{n \\to \\infty} \\left( \\sum_{k=1}^n \\frac{1}{k} - \\ln n \\right) \\approx 0.577", true),
        ("sqrt", "x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}", true),
        ("frac_inline", "\\frac{a}{b} + \\frac{c}{d}", false),
    ];
    for (name, src, disp) in cases {
        let out = if disp { mdweb::tex::render_block(src) } else { mdweb::tex::render(src) };
        println!("===== {name} =====");
        println!("{out}");
    }
}
