//! Ugly override of DefaultNodeShape to get larger label text

use eframe::egui::{FontFamily, FontId};
use egui_graphs::{DefaultNodeShape, DisplayNode, NodeProps};
use petgraph::{EdgeType, csr::IndexType};

#[derive(Debug, Clone)]
pub struct NodeShape {
    default_node: DefaultNodeShape,
}

impl<N: Clone> From<NodeProps<N>> for NodeShape {
    fn from(node_props: NodeProps<N>) -> Self {
        Self {
            default_node: node_props.into(),
        }
    }
}

impl<N: Clone, E: Clone, Ty: EdgeType, Ix: IndexType> DisplayNode<N, E, Ty, Ix> for NodeShape {
    fn closest_boundary_point(&self, dir: eframe::egui::Vec2) -> eframe::egui::Pos2 {
        <DefaultNodeShape as DisplayNode<N, E, Ty, Ix>>::closest_boundary_point(
            &self.default_node,
            dir,
        )
    }

    fn shapes(&mut self, ctx: &egui_graphs::DrawContext) -> Vec<eframe::egui::Shape> {
        let mut r =
            <DefaultNodeShape as DisplayNode<N, E, Ty, Ix>>::shapes(&mut self.default_node, ctx);

        for shape in r.iter_mut() {
            if let eframe::egui::Shape::Text(shape) = shape {
                let size = ctx
                    .meta
                    .canvas_to_screen_size(self.default_node.radius * 2.5);
                shape.galley = ctx.ctx.fonts_mut(|f| {
                    f.layout_no_wrap(
                        shape.galley.text().to_owned(),
                        FontId::new(size, FontFamily::Monospace),
                        self.default_node.color.unwrap_or_default(),
                    )
                });
                shape.pos.x += size;
                break;
            }
        }

        r
    }

    fn update(&mut self, state: &egui_graphs::NodeProps<N>) {
        <DefaultNodeShape as DisplayNode<N, E, Ty, Ix>>::update(&mut self.default_node, state)
    }

    fn is_inside(&self, pos: eframe::egui::Pos2) -> bool {
        <DefaultNodeShape as DisplayNode<N, E, Ty, Ix>>::is_inside(&self.default_node, pos)
    }
}
