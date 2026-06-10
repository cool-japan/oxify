//! SVG generation for workflow DAG previews.
//!
//! Produces compact, self-contained `<svg>` strings from node/edge data using
//! a layered Sugiyama-inspired layout: topological sort → layer assignment →
//! within-layer ordering → Bezier edge routing.

use std::collections::{HashMap, HashSet, VecDeque};

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

/// A node in the workflow DAG used for SVG rendering.
#[derive(Debug, Clone)]
pub struct SvgNode {
    pub id: String,
    pub label: String,
    /// Semantic type: `"start"`, `"end"`, `"llm"`, `"code"`, `"conditional"`, or anything else.
    pub node_type: String,
    /// Pre-assigned x position in pixels, or 0.0 to trigger auto-layout.
    pub x: f32,
    /// Pre-assigned y position in pixels, or 0.0 to trigger auto-layout.
    pub y: f32,
}

/// A directed edge between two nodes.
#[derive(Debug, Clone)]
pub struct SvgEdge {
    pub from_id: String,
    pub to_id: String,
}

// ---------------------------------------------------------------------------
// Colour palette (accessible, high-contrast)
// ---------------------------------------------------------------------------

fn node_fill(node_type: &str) -> &'static str {
    match node_type {
        "start" => "#10b981",
        "end" => "#ef4444",
        "llm" => "#3b82f6",
        "code" => "#22c55e",
        "conditional" => "#eab308",
        _ => "#6b7280",
    }
}

fn node_stroke(node_type: &str) -> &'static str {
    match node_type {
        "start" => "#047857",
        "end" => "#b91c1c",
        "llm" => "#1d4ed8",
        "code" => "#15803d",
        "conditional" => "#a16207",
        _ => "#374151",
    }
}

// ---------------------------------------------------------------------------
// Layout constants
// ---------------------------------------------------------------------------

const NODE_W: f32 = 90.0;
const NODE_H: f32 = 32.0;
const V_GAP: f32 = 20.0; // vertical gap between nodes in same layer
const MARGIN: f32 = 16.0;

// ---------------------------------------------------------------------------
// Auto-layout: layered/hierarchical
// ---------------------------------------------------------------------------

/// Assign each node to a layer (column) via a longest-path algorithm
/// (so sources get layer 0 and sinks get the last layer).
fn assign_layers(nodes: &[SvgNode], edges: &[SvgEdge]) -> HashMap<String, usize> {
    // Build adjacency: in-degree & successors
    let mut in_degree: HashMap<String, usize> = nodes.iter().map(|n| (n.id.clone(), 0)).collect();
    let mut successors: HashMap<String, Vec<String>> =
        nodes.iter().map(|n| (n.id.clone(), vec![])).collect();

    for e in edges {
        if in_degree.contains_key(&e.from_id) && in_degree.contains_key(&e.to_id) {
            *in_degree.entry(e.to_id.clone()).or_insert(0) += 1;
            successors
                .entry(e.from_id.clone())
                .or_default()
                .push(e.to_id.clone());
        }
    }

    // Kahn's topological sort → assign layer = max predecessor layer + 1
    let mut layer: HashMap<String, usize> = HashMap::new();
    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(id, _)| id.clone())
        .collect();

    // Give sources layer 0
    for id in &queue {
        layer.insert(id.clone(), 0);
    }

    let mut remaining_in: HashMap<String, usize> = in_degree.clone();

    while let Some(id) = queue.pop_front() {
        let current_layer = *layer.get(&id).unwrap_or(&0);
        if let Some(succs) = successors.get(&id) {
            for succ in succs.clone() {
                let new_layer = current_layer + 1;
                let entry = layer.entry(succ.clone()).or_insert(0);
                if new_layer > *entry {
                    *entry = new_layer;
                }
                let deg = remaining_in.entry(succ.clone()).or_insert(0);
                if *deg > 0 {
                    *deg -= 1;
                }
                if *deg == 0 {
                    queue.push_back(succ);
                }
            }
        }
    }

    // Any node not reached (disconnected or cycle) goes to layer 0
    for n in nodes {
        layer.entry(n.id.clone()).or_insert(0);
    }

    layer
}

/// Compute (x, y) positions for all nodes using the layer map.
/// Returns a map from node id → (cx, cy) where cx/cy is the centre point.
fn compute_positions(
    nodes: &[SvgNode],
    edges: &[SvgEdge],
    canvas_w: u32,
    canvas_h: u32,
) -> HashMap<String, (f32, f32)> {
    let layer_map = assign_layers(nodes, edges);

    // Collect nodes per layer
    let mut layers: HashMap<usize, Vec<String>> = HashMap::new();
    for n in nodes {
        let l = *layer_map.get(&n.id).unwrap_or(&0);
        layers.entry(l).or_default().push(n.id.clone());
    }

    let num_layers = layers.keys().copied().max().unwrap_or(0) + 1;

    // Horizontal spacing
    let usable_w = canvas_w as f32 - 2.0 * MARGIN - NODE_W;
    let col_step = if num_layers > 1 {
        usable_w / (num_layers - 1) as f32
    } else {
        0.0
    };

    let mut positions: HashMap<String, (f32, f32)> = HashMap::new();

    for layer_idx in 0..num_layers {
        let ids = match layers.get(&layer_idx) {
            Some(v) => v,
            None => continue,
        };
        let count = ids.len();
        let total_h = count as f32 * NODE_H + (count.saturating_sub(1)) as f32 * V_GAP;
        let start_y = (canvas_h as f32 - total_h) / 2.0;
        let cx = MARGIN + NODE_W / 2.0 + layer_idx as f32 * col_step;

        for (i, id) in ids.iter().enumerate() {
            let cy = start_y + i as f32 * (NODE_H + V_GAP) + NODE_H / 2.0;
            positions.insert(id.clone(), (cx, cy));
        }
    }

    positions
}

// ---------------------------------------------------------------------------
// SVG rendering helpers
// ---------------------------------------------------------------------------

fn render_node(id: &str, label: &str, node_type: &str, cx: f32, cy: f32) -> String {
    let x = cx - NODE_W / 2.0;
    let y = cy - NODE_H / 2.0;
    let fill = node_fill(node_type);
    let stroke = node_stroke(node_type);

    // Truncate label to avoid overflow
    let display_label = if label.len() > 12 {
        format!("{}…", &label[..11])
    } else {
        label.to_string()
    };

    format!(
        r#"<rect x="{x:.1}" y="{y:.1}" width="{NODE_W}" height="{NODE_H}" rx="6" ry="6" fill="{fill}" stroke="{stroke}" stroke-width="1.5" role="img" aria-label="{id}"/>
<text x="{cx:.1}" y="{cy:.1}" dominant-baseline="central" text-anchor="middle" fill="white" font-size="9" font-family="system-ui,sans-serif" font-weight="600">{display_label}</text>"#,
    )
}

fn render_edge(x1: f32, y1: f32, x2: f32, y2: f32) -> String {
    // Cubic Bezier: control points at 1/3 and 2/3 of the horizontal span
    let dx = (x2 - x1).abs() / 3.0;
    let bx1 = x1 + dx;
    let bx2 = x2 - dx;
    // Avoid # inside r#"..."# raw string by using standard string escaping
    format!(
        "<path d=\"M{:.1},{:.1} C{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" \
         fill=\"none\" stroke=\"#9ca3af\" stroke-width=\"1.5\" marker-end=\"url(#arrowhead)\"/>",
        x1, y1, bx1, y1, bx2, y2, x2, y2,
    )
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Generate a self-contained SVG string for a workflow DAG.
///
/// If nodes have `x == 0.0 && y == 0.0` they are auto-laid-out via a
/// topological-sort-based layer assignment.  Otherwise their pre-supplied
/// coordinates are used directly as centre points.
pub fn workflow_to_svg(nodes: &[SvgNode], edges: &[SvgEdge], width: u32, height: u32) -> String {
    if nodes.is_empty() {
        return format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}"></svg>"#
        );
    }

    // Decide whether to auto-layout: if ALL nodes have (0,0) treat it as unset
    let all_zero = nodes.iter().all(|n| n.x == 0.0 && n.y == 0.0);

    // Build id → position map
    let node_positions: HashMap<String, (f32, f32)> = if all_zero {
        compute_positions(nodes, edges, width, height)
    } else {
        nodes.iter().map(|n| (n.id.clone(), (n.x, n.y))).collect()
    };

    // Collect node ids for quick lookup
    let valid_ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();

    // Arrow marker definition (avoid # in raw-string delimiters by using \")
    let defs = "<defs>\n  \
        <marker id=\"arrowhead\" markerWidth=\"8\" markerHeight=\"8\" \
        refX=\"7\" refY=\"3\" orient=\"auto\" markerUnits=\"strokeWidth\">\n    \
        <path d=\"M0,0 L0,6 L8,3 z\" fill=\"#9ca3af\"/>\n  \
        </marker>\n\
        </defs>";

    // Render edges first (below nodes)
    let edge_svg: String = edges
        .iter()
        .filter_map(|e| {
            if !valid_ids.contains(e.from_id.as_str()) || !valid_ids.contains(e.to_id.as_str()) {
                return None;
            }
            let &(x1, y1) = node_positions.get(&e.from_id)?;
            let &(x2, y2) = node_positions.get(&e.to_id)?;
            // Start from right edge, end at left edge of target node
            Some(render_edge(x1 + NODE_W / 2.0, y1, x2 - NODE_W / 2.0, y2))
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Render nodes
    let node_svg: String = nodes
        .iter()
        .filter_map(|n| {
            let &(cx, cy) = node_positions.get(&n.id)?;
            Some(render_node(&n.id, &n.label, &n.node_type, cx, cy))
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}" role="img" aria-label="Workflow diagram">
{defs}
{edge_svg}
{node_svg}
</svg>"#
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_svg() {
        let svg = workflow_to_svg(&[], &[], 300, 150);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_single_node() {
        let nodes = vec![SvgNode {
            id: "n1".into(),
            label: "Start".into(),
            node_type: "start".into(),
            x: 0.0,
            y: 0.0,
        }];
        let svg = workflow_to_svg(&nodes, &[], 300, 150);
        assert!(svg.contains("rect"));
        assert!(svg.contains("Start"));
        assert!(svg.contains("#10b981")); // start colour
    }

    #[test]
    fn test_linear_dag() {
        let nodes = vec![
            SvgNode {
                id: "a".into(),
                label: "A".into(),
                node_type: "start".into(),
                x: 0.0,
                y: 0.0,
            },
            SvgNode {
                id: "b".into(),
                label: "B".into(),
                node_type: "llm".into(),
                x: 0.0,
                y: 0.0,
            },
            SvgNode {
                id: "c".into(),
                label: "C".into(),
                node_type: "end".into(),
                x: 0.0,
                y: 0.0,
            },
        ];
        let edges = vec![
            SvgEdge {
                from_id: "a".into(),
                to_id: "b".into(),
            },
            SvgEdge {
                from_id: "b".into(),
                to_id: "c".into(),
            },
        ];
        let svg = workflow_to_svg(&nodes, &edges, 400, 200);
        assert!(svg.contains("path")); // edges
        assert!(svg.contains("#3b82f6")); // llm colour
        assert!(svg.contains("#ef4444")); // end colour
    }

    #[test]
    fn test_label_truncation() {
        let nodes = vec![SvgNode {
            id: "n1".into(),
            label: "This is a very long label name".into(),
            node_type: "code".into(),
            x: 150.0,
            y: 75.0,
        }];
        let svg = workflow_to_svg(&nodes, &[], 300, 150);
        assert!(svg.contains("…")); // truncated
    }

    #[test]
    fn test_assign_layers_linear() {
        let nodes = vec![
            SvgNode {
                id: "a".into(),
                label: "".into(),
                node_type: "".into(),
                x: 0.0,
                y: 0.0,
            },
            SvgNode {
                id: "b".into(),
                label: "".into(),
                node_type: "".into(),
                x: 0.0,
                y: 0.0,
            },
            SvgNode {
                id: "c".into(),
                label: "".into(),
                node_type: "".into(),
                x: 0.0,
                y: 0.0,
            },
        ];
        let edges = vec![
            SvgEdge {
                from_id: "a".into(),
                to_id: "b".into(),
            },
            SvgEdge {
                from_id: "b".into(),
                to_id: "c".into(),
            },
        ];
        let layers = assign_layers(&nodes, &edges);
        assert_eq!(layers["a"], 0);
        assert_eq!(layers["b"], 1);
        assert_eq!(layers["c"], 2);
    }
}
