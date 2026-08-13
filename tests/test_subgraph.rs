#[test]
fn subgraph_label_renders_inside_box() {
    // The subgraph label sits INSIDE the box at the top-left
    // (baseline at `y0 + 14`), matching Mermaid's default rendering.
    // Earlier versions placed it ABOVE the box (baseline at `y0 - 6`),
    // which both visually floated outside the cluster and could clip
    // against the top edge of the viewBox.
    let src = "flowchart LR\n  subgraph 构建\n    A[Markdown] --> B[解析器]\n    B --> C((渲染))\n    C --> D[嵌入 HTML]\n  end\n";
    let svg = mdweb::diagram::flowchart::render(src).expect("render");
    assert!(
        svg.contains("构建"),
        "subgraph name should appear in SVG: {svg}"
    );
    // The label is horizontally centred (text-anchor=middle at box
    // midpoint) but anchored near the TOP of the box. The text
    // baseline sits at y0 + 16, leaving ~6 px above the cap line
    // for the 13 px font.
    let rect_chunk = svg
        .split_once("rx=\"8\"")
        .map(|(head, _)| head)
        .and_then(|s| s.rsplit_once("<rect "))
        .map(|(_, tail)| tail)
        .unwrap_or("");
    let nums: Vec<f64> = rect_chunk
        .split('"')
        .filter_map(|p| p.trim().parse::<f64>().ok())
        .collect();
    let rect_x = nums.first().copied().unwrap_or(0.0);
    let rect_y = nums.get(1).copied().unwrap_or(0.0);
    let rect_w = nums.get(2).copied().unwrap_or(0.0);
    let _rect_h = nums.get(3).copied().unwrap_or(0.0);
    let after_rect = svg
        .split_once("rx=\"8\"")
        .map(|(_, tail)| tail)
        .unwrap_or("");
    let label_chunk = after_rect
        .split_once(">构建<")
        .map(|(head, _)| head)
        .and_then(|s| s.rsplit_once("<text "))
        .map(|(_, tail)| tail)
        .unwrap_or("");
    let mut label_nums = label_chunk
        .split('"')
        .filter_map(|p| p.trim().parse::<f64>().ok());
    let label_x = label_nums.next().unwrap_or(0.0);
    let label_y = label_nums.next().unwrap_or(0.0);
    let cx = rect_x + rect_w / 2.0;
    assert!(
        (label_x - cx).abs() < 1.0,
        "label x {label_x} should match box centre x {cx}"
    );
    // y must sit in the top portion of the box: within ~20 px below
    // the top edge so the cap line still has room to breathe. The
    // 13 px font's baseline is at y0 + 16, so the cap top sits at
    // ~y0 + 6 — leaving 6 px of clearance from the box top.
    assert!(
        label_y > rect_y && label_y < rect_y + 20.0,
        "label baseline {label_y} should sit near box top {}",
        rect_y + 20.0
    );
    assert!(
        after_rect.contains("text-anchor=\"middle\""),
        "label needs text-anchor=middle to centre horizontally: {svg}"
    );
    // viewBox y is 0 because nothing extends above the canvas any more.
    let vb_y: f64 = svg
        .split("viewBox=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .and_then(|s| s.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    assert!(
        vb_y.abs() < 0.001,
        "viewBox y should be 0 (label is inside the box now), got {vb_y}"
    );
    // Circle (`C((渲染))`) must be rendered with fill+stroke INLINE on
    // the <circle> tag — appending them after `/>` would orphan them
    // as text content and leave the circle without colour.
    assert!(
        svg.contains("<circle ") && svg.contains("fill=\"#7ed321\""),
        "circle must have fill attribute inline: {svg}"
    );
    // The label inside `C((渲染))` is `渲染` (not `渲染)`) — the
    // parser previously included the trailing `)` from `))`.
    assert!(
        svg.contains(">渲染<"),
        "circle label should be just `渲染`, got: {svg}"
    );
    assert!(
        !svg.contains(">渲染)<"),
        "circle label must not include trailing `)`: {svg}"
    );
    // The trailing node label must appear verbatim (possibly split
    // across <tspan>s with explicit per-run font-family). Both the
    // glyphs and a CJK-aware font are needed so viewers without a
    // default CJK font can still render these glyphs.
    assert!(svg.contains("嵌入"));
    assert!(svg.contains("HTML"));
    assert!(
        svg.contains("Noto Sans CJK SC"),
        "CJK runs should reference Noto Sans CJK SC explicitly: {svg}"
    );
}

#[test]
fn edge_label_exits_subgraph_box() {
    // When an edge has one endpoint inside a subgraph and the other
    // outside, the label used to sit at the edge midpoint — which
    // is also where the subgraph box's right padding lives. The
    // label would overlap the box visually (e.g. `B -- SVG --> C`
    // with B inside 构建, label `SVG` drawn on top of the grey
    // subgraph fill). The fix slides the label along the edge to
    // the OUTSIDE half so it sits past the box.
    let src = "flowchart LR\n  subgraph 构建\n    A[Markdown] --> B[解析器]\n  end\n  B -- SVG --> C((渲染))\n";
    let svg = mdweb::diagram::flowchart::render(src).expect("render");
    // The subgraph rect's right edge.
    let subgraph_rect = svg
        .split("rx=\"8\"")
        .next()
        .and_then(|s| s.rsplit_once("<rect "))
        .map(|(_, tail)| tail)
        .unwrap_or("");
    let nums: Vec<f64> = subgraph_rect
        .split('"')
        .filter_map(|p| p.trim().parse::<f64>().ok())
        .collect();
    let rect_x = nums.first().copied().unwrap_or(0.0);
    let rect_w = nums.get(2).copied().unwrap_or(0.0);
    let rect_right = rect_x + rect_w;
    // The edge label rect (the white-background rect with rx="3",
    // not the subgraph rx="8" one). Its left edge must be at or
    // past the subgraph right edge. Find the rect just before the
    // `>SVG</text>` text.
    let label_chunk = svg
        .split(">SVG</text>")
        .next()
        .and_then(|s| s.rsplit_once("<rect "))
        .map(|(_, tail)| tail)
        .unwrap_or("");
    let label_nums: Vec<f64> = label_chunk
        .split('"')
        .filter_map(|p| p.trim().parse::<f64>().ok())
        .collect();
    let label_rect_x = label_nums.first().copied().unwrap_or(0.0);
    let label_w = label_nums.get(2).copied().unwrap_or(0.0);
    let label_left = label_rect_x;
    let label_right = label_rect_x + label_w;
    assert!(
        label_left >= rect_right - 0.5,
        "edge label rect left {label_left} should be at or past subgraph right edge {rect_right} (label spans [{label_left}, {label_right}])"
    );
}

#[test]
fn no_visual_overlap_in_complex_subgraph() {
    // End-to-end layout sanity check. Render a subgraph + edges +
    // edge label and verify that the rendered bounding boxes don't
    // collide: the 构建 label sits clear of the contained nodes,
    // and the SVG edge label sits clear of both the subgraph box
    // and the 解析器 node. Earlier versions of the renderer would
    // centre the subgraph label at the box midpoint (y0 + 35),
    // which put it on top of the contained nodes' rectangles.
    let src = "flowchart LR\n  subgraph 构建\n    A[Markdown] --> B[解析器]\n  end\n  B -- SVG --> C((渲染))\n";
    let svg = mdweb::diagram::flowchart::render(src).expect("render");

    // Parse every <text> element's x/y attributes. Returns
    // (x_center, y_baseline, text).
    fn texts(svg: &str) -> Vec<(f64, f64, String)> {
        let mut out = Vec::new();
        let mut rest = svg;
        while let Some(idx) = rest.find("<text ") {
            let after = &rest[idx + 6..];
            let close = after.find('>').unwrap_or(0);
            let head = &after[..close];
            let body_close = after.find("</text>").unwrap_or(0);
            let body = &after[close + 1..body_close];
            let nums: Vec<f64> = head
                .split('"')
                .filter_map(|p| p.trim().parse::<f64>().ok())
                .collect();
            if nums.len() >= 2 {
                out.push((nums[0], nums[1], body.to_string()));
            }
            rest = &after[body_close + 7..];
        }
        out
    }

    // The subgraph rect.
    let rect_chunk = svg
        .split_once("rx=\"8\"")
        .map(|(head, _)| head)
        .and_then(|s| s.rsplit_once("<rect "))
        .map(|(_, tail)| tail)
        .unwrap_or("");
    let rect_nums: Vec<f64> = rect_chunk
        .split('"')
        .filter_map(|p| p.trim().parse::<f64>().ok())
        .collect();
    let rect_x = rect_nums.first().copied().unwrap_or(0.0);
    let rect_y = rect_nums.get(1).copied().unwrap_or(0.0);
    let rect_w = rect_nums.get(2).copied().unwrap_or(0.0);
    let rect_h = rect_nums.get(3).copied().unwrap_or(0.0);
    let (sub_left, sub_right, sub_top, sub_bot) =
        (rect_x, rect_x + rect_w, rect_y, rect_y + rect_h);

    // The 构建 label text element.
    let label_chunk = svg
        .split_once(">构建<")
        .map(|(head, _)| head)
        .and_then(|s| s.rsplit_once("<text "))
        .map(|(_, tail)| tail)
        .unwrap_or("");
    let label_nums: Vec<f64> = label_chunk
        .split('"')
        .filter_map(|p| p.trim().parse::<f64>().ok())
        .collect();
    let lx = label_nums.first().copied().unwrap_or(0.0);
    let ly = label_nums.get(1).copied().unwrap_or(0.0);

    // Cap-top of the label (approx 9 px above baseline for 13 px font).
    let label_cap_top = ly - 9.0;
    // Walk every `<rect>` element in the SVG and pull out its
    // (x, y, w, h). We'll use these to derive the contained-node
    // band and the source node's right edge dynamically instead of
    // hardcoding pixel values that shift when inter-layer spacing
    // changes.
    let mut rects: Vec<(f64, f64, f64, f64)> = Vec::new();
    {
        let mut rest: &str = svg.as_ref();
        while let Some(idx) = rest.find("<rect ") {
            let after = &rest[idx + 6..];
            let close = after.find('/').unwrap_or(after.len());
            let head = &after[..close];
            let nums: Vec<f64> = head
                .split('"')
                .filter_map(|p| p.trim().parse::<f64>().ok())
                .collect();
            if nums.len() >= 4 {
                rects.push((nums[0], nums[1], nums[2], nums[3]));
            }
            rest = &after[close..];
        }
    }
    // The subgraph rect is the one with rx="8" (large radius). The
    // contained node rects use rx="4" (or none). The label rects
    // use rx="3". Distinguish by rx attribute.
    let contained: Vec<(f64, f64, f64, f64)> = rects
        .iter()
        .copied()
        .filter(|&(x, _, _, _)| x >= sub_left && x + 0.5 <= sub_right)
        // Skip the subgraph rect itself (its rx is 8, not 4).
        .filter(|r| {
            // The subgraph rect spans (sub_left, sub_top, sub_right, sub_bot).
            !(r.0 <= sub_left + 0.5
                && r.1 <= sub_top + 0.5
                && r.0 + r.2 >= sub_right - 0.5
                && r.1 + r.3 >= sub_bot - 0.5)
        })
        .collect();
    let min_node_top = contained.iter().map(|r| r.1).fold(f64::INFINITY, f64::min);
    // Label must sit ENTIRELY ABOVE the contained node rectangles.
    // The earlier centred-at-midpoint version put label baseline at
    // y~50, which collided with the node band.
    assert!(
        label_cap_top < min_node_top,
        "构建 label cap-top {label_cap_top} (baseline {ly}) should sit above node band top {min_node_top}"
    );
    // And the label must stay inside the box horizontally — text
    // is centred at x=lx, but its cap line spans at least one
    // CJK glyph (~13 px on each side) so the glyph extent is
    // [lx - 15, lx + 15].
    let label_half_w = 15.0_f64;
    assert!(
        lx - label_half_w >= sub_left - 0.5 && lx + label_half_w <= sub_right + 0.5,
        "构建 label x-extent [{:.1}, {:.1}] should sit within box [{sub_left}, {sub_right}]",
        lx - label_half_w,
        lx + label_half_w
    );

    // Verify the SVG edge label (inside its rx=3 white rect) sits
    // clear of both the subgraph box and the 解析器 node. Parse
    // the text element with body `SVG`.
    let svg_label = texts(&svg)
        .into_iter()
        .find(|(_, _, body)| body == "SVG")
        .expect("SVG edge label must be present");
    let (svg_lx, svg_ly, _) = svg_label;
    // Parse the actual white-background label rect (rx=3) sitting
    // just before the SVG text — gives the true half-width instead
    // of a guessed estimate.
    let label_rect_chunk = svg
        .split_once(">SVG</text>")
        .and_then(|(head, _)| head.rsplit_once("<rect "))
        .map(|(_, tail)| tail)
        .unwrap_or("");
    let lr_nums: Vec<f64> = label_rect_chunk
        .split('"')
        .filter_map(|p| p.trim().parse::<f64>().ok())
        .collect();
    let label_rect_x = lr_nums.first().copied().unwrap_or(svg_lx - 12.0);
    let label_rect_w = lr_nums.get(2).copied().unwrap_or(24.0);
    let svg_label_left = label_rect_x;
    let svg_label_right = label_rect_x + label_rect_w;
    assert!(
        svg_label_left >= sub_right - 0.5,
        "SVG edge label rect left {svg_label_left} must clear subgraph right edge {sub_right} (label spans [{svg_label_left}, {svg_label_right}])"
    );
    // 解析器's right edge — derive it from the contained rects rather
    // than hardcoding (the inter-layer gap can shift this number).
    let parsed_right = contained
        .iter()
        .map(|r| r.0 + r.2)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        svg_label_left >= parsed_right - 0.5,
        "SVG edge label left edge {svg_label_left} must clear 解析器 node right edge {parsed_right}"
    );
    // Edges into the circle must land ON the circle's visible left
    // edge (cx − r), not on the bounding-box left edge. Earlier the
    // anchor used the bbox, leaving an 11 px gap between the line
    // tip and the circle stroke.
    for raw_path in svg.split('<').filter(|p| p.starts_with("path d=\"M 266.8")) {
        let nums: Vec<f64> = raw_path
            .split('"')
            .nth(1)
            .unwrap_or("")
            .split_whitespace()
            .filter_map(|s| s.trim_start_matches(['M', 'L']).parse::<f64>().ok())
            .collect();
        if nums.len() >= 4 {
            let end_x = nums[2];
            assert!(
                (end_x - 337.8).abs() < 0.5,
                "edge into circle should land at cx − r = 337.8, got {end_x}"
            );
        }
    }

    // Every text element that lies inside the subgraph (Markdown,
    // 解析器) must have its baseline BELOW the label cap line — i.e.
    // the label and the contained node labels occupy different
    // vertical bands.
    for (tx, ty, body) in texts(&svg) {
        if (body.contains("Markdown") || body.contains("解析器"))
            && tx >= sub_left
            && tx <= sub_right
        {
            assert!(
                ty > ly,
                "node label {body:?} baseline {ty} should sit below 构建 label baseline {ly}"
            );
        }
    }
}

#[test]
fn subgraph_box_vertically_centred_when_only_some_columns_inside() {
    // When the subgraph wraps only some of the columns (here just
    // A and B; C and D live outside and connect via labelled edges),
    // the box should still be vertically centred with the whole
    // diagram — not hug the contained-node band. Earlier the box
    // hugged A and B (y=42..112, centre 77) and floated well above
    // the diagram centre (y=90).
    let src = "flowchart LR\n  subgraph 构建\n    A[Markdown] --> B[解析器]\n  end\n  B --> C((渲染))\n  C --> D[嵌入 HTML]\n  B -- SVG --> C((渲染))\n";
    let svg = mdweb::diagram::flowchart::render(src).expect("render");
    let rect_chunk = svg
        .split_once("rx=\"8\"")
        .map(|(head, _)| head)
        .and_then(|s| s.rsplit_once("<rect "))
        .map(|(_, tail)| tail)
        .unwrap_or("");
    let nums: Vec<f64> = rect_chunk
        .split('"')
        .filter_map(|p| p.trim().parse::<f64>().ok())
        .collect();
    let rect_x = nums.first().copied().unwrap_or(0.0);
    let rect_y = nums.get(1).copied().unwrap_or(0.0);
    let rect_w = nums.get(2).copied().unwrap_or(0.0);
    let rect_h = nums.get(3).copied().unwrap_or(0.0);
    let box_cy = rect_y + rect_h / 2.0;
    // Walk every <rect> element so the wrap assertions below can
    // derive the contained nodes' positions rather than hardcode
    // them — inter-layer spacing can shift those numbers.
    let mut rects: Vec<(f64, f64, f64, f64)> = Vec::new();
    {
        let mut rest: &str = svg.as_ref();
        while let Some(idx) = rest.find("<rect ") {
            let after = &rest[idx + 6..];
            let close = after.find('/').unwrap_or(after.len());
            let head = &after[..close];
            let nums2: Vec<f64> = head
                .split('"')
                .filter_map(|p| p.trim().parse::<f64>().ok())
                .collect();
            if nums2.len() >= 4 {
                rects.push((nums2[0], nums2[1], nums2[2], nums2[3]));
            }
            rest = &after[close..];
        }
    }
    // Parse the viewBox to derive the diagram centre. viewBox is
    // "minX minY width height" — the centre y is `height / 2` once
    // the layout is symmetric about the origin, which it always is
    // because the normalize pass shifts every node so the topmost
    // sits at `gy` and the viewBox height is `max_y + 2*gy`.
    let vb: Vec<f64> = svg
        .split("viewBox=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .unwrap_or("")
        .split_whitespace()
        .filter_map(|p| p.parse().ok())
        .collect();
    let diagram_cy = vb.get(3).copied().unwrap_or(180.0) / 2.0;
    assert!(
        (box_cy - diagram_cy).abs() < 1.0,
        "subgraph box centre y={box_cy} should match viewBox centre y={diagram_cy}"
    );
    // Box should comfortably wrap Markdown and 解析器 with margin on
    // both sides — earlier used 4 px which looked cramped. Derive
    // the leftmost/rightmost contained-node edges from the SVG
    // rather than hardcoding (the inter-layer gap shifts them).
    // Skip the subgraph rect itself and the SVG edge-label rect
    // (which sits past the box on the right, not inside it).
    let non_sub: Vec<(f64, f64, f64, f64)> = rects
        .iter()
        .copied()
        .filter(|r| {
            // Skip the subgraph rect (its bounds match rect_*).
            let is_subgraph = r.0 <= rect_x + 0.5
                && r.1 <= rect_y + 0.5
                && r.0 + r.2 >= rect_x + rect_w - 0.5
                && r.1 + r.3 >= rect_y + rect_h - 0.5;
            // Skip the edge-label rect (it lives outside the box).
            let inside_box = r.0 >= rect_x && r.0 + r.2 <= rect_x + rect_w;
            !is_subgraph && inside_box
        })
        .collect();
    let md_left = non_sub.iter().map(|r| r.0).fold(f64::INFINITY, f64::min);
    assert!(
        rect_x <= md_left - 10.0,
        "subgraph left {rect_x} should leave ≥10 px margin before leftmost contained node left {md_left}"
    );
    let contained_right = non_sub
        .iter()
        .map(|r| r.0 + r.2)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        rect_x + rect_w >= contained_right + 5.0,
        "subgraph right {} should extend past rightmost contained node right {contained_right}",
        rect_x + rect_w
    );
}
