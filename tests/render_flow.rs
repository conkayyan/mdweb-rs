#[test]
fn render_user_flow_to_png() {
    let src = "graph TD\n A[写出新文章] --> B[撰写 Markdown]\n B --> C{包含公式?}\n C -- 是 --> D[加入行内公式]\n C -- 否 --> E{包含图表?}\n E -- 是 --> F[加入流程图]\n E -- 否 --> G[纯静态页面]\n D --> G\n F --> G\n";
    let svg = mdweb::diagram::flowchart::render(src).unwrap();
    std::fs::write("/tmp/diag.svg", &svg).unwrap();
}
