use eframe::egui::{self, RichText, ecolor};
use jj_lib::backend::CommitId;
use jj_lib::repo::Repo;
use std::collections::HashMap;

mod jjgraph;

// The undirected graph does not put nodes in nice positions when rendering a hierarchical graph view.
// type GraphType = egui_graphs::Graph<CommitId, (), petgraph::Undirected>;
type GraphType = egui_graphs::Graph<CommitId, (), petgraph::Directed>;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1024., 768.]),
        ..Default::default()
    };
    eframe::run_native(
        "Revset Explorer",
        options,
        Box::new(|_cc| Ok(Box::<ExplorerApp>::default())),
    )
}

struct ExplorerApp {
    revset: String,
    old_revset: String,
    revset_error: Option<String>,
    graph: GraphType,
    node_idxs: Vec<petgraph::graph::NodeIndex>,
    jj_graph: jjgraph::JjGraph,
    working_copy_commit_id: Option<CommitId>,
}

impl Default for ExplorerApp {
    fn default() -> Self {
        let initial_revset = "@".to_owned();
        let jj_graph = jjgraph::JjGraph::new().unwrap();
        let (g, node_idxs) = generate_graph(&jj_graph).unwrap();
        let repo = jj_graph.get_repo();
        let working_copy_commit_id = repo
            .view()
            .get_wc_commit_id(jj_lib::ref_name::WorkspaceName::DEFAULT);
        Self {
            revset: initial_revset.clone(),
            old_revset: "".to_owned(),
            revset_error: None,
            graph: g,
            node_idxs,
            jj_graph,
            working_copy_commit_id: working_copy_commit_id.cloned(),
        }
    }
}

fn generate_graph(
    jj_graph: &jjgraph::JjGraph,
) -> anyhow::Result<(GraphType, Vec<petgraph::graph::NodeIndex>)> {
    let mut graph: GraphType =
        egui_graphs::Graph::new(petgraph::stable_graph::StableGraph::default());

    // TODO: Import the revset aliases from the config
    // present(@) | ancestors(immutable_heads().., 2) | present(trunk())
    let all_revset = jj_graph.get_revset("::")?;

    let repo = jj_graph.get_repo();
    let working_copy_commit_id = repo
        .view()
        .get_wc_commit_id(jj_lib::ref_name::WorkspaceName::DEFAULT);
    let store = repo.store();
    let mut node_idxs = vec![];
    let mut node_map = HashMap::new();
    let mut edges = vec![];
    for rev in all_revset.iter_graph() {
        let rev = rev?;
        let commit_id = rev.0;
        let commit_edges = rev.1;
        let commit = store.get_commit(&commit_id)?;
        let change_id = commit.change_id();
        let change_id_len = repo.shortest_unique_change_id_prefix_len(change_id)?;
        let change_id_prefix = change_id.to_string()[..change_id_len].to_string();

        let mut desc: String = commit
            .description()
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(12)
            .collect();
        if desc.len() == 12 {
            desc += "...";
        }
        let mut label = change_id_prefix;
        if Some(&commit_id) == working_copy_commit_id {
            label = format!("@ {label}");
        }
        let node_idx = graph.add_node_with_label(commit_id.clone(), label);
        node_idxs.push(node_idx);
        node_map.insert(commit_id.clone(), node_idx);

        for commit_edge in commit_edges {
            edges.push((commit_id.clone(), commit_edge.target));
        }
    }
    for edge in edges {
        let Some(start) = node_map.get(&edge.0) else { continue };
        let Some(end) = node_map.get(&edge.1) else { continue };
        graph.add_edge_with_label(*start, *end, (), "".to_owned());
    }

    Ok((graph, node_idxs))
}

#[derive(Debug, PartialEq)]
enum UpdateError {
    RevsetParseError(String),
    JjError,
}

impl ExplorerApp {
    fn update_graph(&mut self) -> anyhow::Result<(), UpdateError> {
        let filter_revset = self.jj_graph.get_revset(&self.revset);

        #[expect(clippy::type_complexity)]
        let (in_filter, revset_parse_error): (Box<dyn Fn(&CommitId) -> Result<_, _>>, _) =
            match filter_revset {
                Ok(filter_revset) => (filter_revset.containing_fn(), None),
                Err(e) => (
                    // Revset is bad, so unmark all nodes
                    Box::new(|_| Ok(false)),
                    Some(e),
                ),
            };

        // TODO: Global var
        let immutable_revset = self
            .jj_graph
            .get_revset(
                "::(present(latest(
  remote_bookmarks(exact:\"main\", exact:\"origin\") |
  remote_bookmarks(exact:\"master\", exact:\"origin\") |
  remote_bookmarks(exact:\"trunk\", exact:\"origin\") |
  remote_bookmarks(exact:\"main\", exact:\"upstream\") |
  remote_bookmarks(exact:\"master\", exact:\"upstream\") |
  remote_bookmarks(exact:\"trunk\", exact:\"upstream\") |
  root()
)) | tags() | untracked_remote_bookmarks() | root())",
            )
            .map_err(|_| UpdateError::JjError)?;

        let is_immutable = immutable_revset.containing_fn();

        for node_idx in self.node_idxs.iter() {
            let node = self.graph.node_mut(*node_idx).unwrap();
            let commit_id = node.payload();
            let immutable = is_immutable(commit_id).map_err(|_| UpdateError::JjError)?;
            let matches_filter = in_filter(commit_id).map_err(|_| UpdateError::JjError)?;
            let is_wc_commit = self
                .working_copy_commit_id
                .as_ref()
                .is_some_and(|wc| commit_id == wc);

            #[derive(Debug, PartialEq, Eq, Hash)]
            enum NodeType {
                WorkingCopy,
                Immutable,
                Regular,
            }
            #[derive(Debug, PartialEq, Eq, Hash)]
            enum FilterMatch {
                Match,
                NoMatch,
            }
            let node_type = if is_wc_commit {
                NodeType::WorkingCopy
            } else if immutable {
                NodeType::Immutable
            } else {
                NodeType::Regular
            };
            let filter_match = if matches_filter {
                FilterMatch::Match
            } else {
                FilterMatch::NoMatch
            };
            #[rustfmt::skip]
            let color_map = HashMap::from([
                ((NodeType::WorkingCopy, FilterMatch::Match), ecolor::Color32::from_hex("#26ff00ff").unwrap()),
                ((NodeType::WorkingCopy, FilterMatch::NoMatch), ecolor::Color32::from_hex("#295923").unwrap()),
                ((NodeType::Immutable, FilterMatch::Match), ecolor::Color32::from_hex("#21cdff").unwrap()),
                ((NodeType::Immutable, FilterMatch::NoMatch), ecolor::Color32::from_hex("#2e5059").unwrap()),
                ((NodeType::Regular, FilterMatch::Match), ecolor::Color32::from_hex("#fffc00").unwrap()),
                ((NodeType::Regular, FilterMatch::NoMatch), ecolor::Color32::from_hex("#636222").unwrap()),
                // ((NodeType::Regular, FilterMatch::Match), ecolor::Color32::from_hex("#ffa400").unwrap()),
                // ((NodeType::Regular, FilterMatch::NoMatch), ecolor::Color32::from_hex("#634c22").unwrap()),
            ]);
            node.set_color(color_map[&(node_type, filter_match)]);
        }

        if let Some(e) = revset_parse_error {
            Err(UpdateError::RevsetParseError(e.to_string()))
        } else {
            Ok(())
        }
    }
}

impl eframe::App for ExplorerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.revset != self.old_revset {
                let update_result = self.update_graph();
                self.revset_error = match update_result {
                    Ok(()) => None,
                    Err(UpdateError::RevsetParseError(msg)) => Some(msg),
                    Err(UpdateError::JjError) => None,
                };
                self.old_revset = self.revset.clone();
            }
            ui.horizontal(|ui| {
                let revset_label = ui.label("Revset: ");
                ui.scope(|ui| {
                    if self.revset_error.is_some() {
                        ui.visuals_mut().extreme_bg_color = ecolor::Color32::DARK_RED;
                    }
                    ui.text_edit_singleline(&mut self.revset)
                        .labelled_by(revset_label.id)
                        .request_focus();
                });
                let err_msg = if let Some(err_msg) = self.revset_error.as_ref() {
                    err_msg.replace("  |\n", "")
                } else {
                    "".to_owned()
                };
                let _error_label = ui.add_sized(
                    [1., ui.text_style_height(&egui::TextStyle::Monospace) * 4.],
                    egui::Label::new(RichText::new(err_msg).family(egui::FontFamily::Monospace)),
                );
            });

            let navigation = egui_graphs::SettingsNavigation::default()
                .with_fit_to_screen_enabled(true)
                .with_zoom_and_pan_enabled(true);
            let interaction = egui_graphs::SettingsInteraction::default()
                .with_dragging_enabled(false)
                .with_edge_clicking_enabled(false)
                .with_edge_selection_enabled(false)
                .with_hover_enabled(true)
                .with_node_clicking_enabled(false)
                .with_node_selection_enabled(false);

            let mut view = egui_graphs::GraphView::<
                _,
                _,
                _,
                _,
                _,
                _,
                egui_graphs::LayoutStateHierarchical,
                egui_graphs::LayoutHierarchical,
            >::new(&mut self.graph)
            .with_navigations(&navigation)
            .with_interactions(&interaction)
            .with_styles(&egui_graphs::SettingsStyle::default().with_labels_always(true));
            ui.add(&mut view);
        });
    }
}
