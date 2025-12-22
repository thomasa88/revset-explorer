use anyhow::Context;
use clap::Parser;
use eframe::egui::{self, RichText, ecolor};
use jj_lib::backend::CommitId;
use jj_lib::repo::Repo;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod jjgraph;

const MAX_NODES: usize = 100;

// The undirected graph does not put nodes in nice positions when rendering a hierarchical graph view.
// type GraphType = egui_graphs::Graph<CommitId, (), petgraph::Undirected>;
type GraphType = egui_graphs::Graph<CommitId, (), petgraph::Directed>;

#[derive(Parser)]
#[command(name = "Revset Explorer")]
struct Args {
    /// Path to the JJ repository to explore
    #[arg(short = 'R', long, default_value = ".")]
    repository: PathBuf,
    /// Generate a sample repository to explore. It will create the directory "revset-sample".
    #[arg(long, default_value_t = false)]
    create_sample: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.create_sample {
        create_sample_repo()?;
        return Ok(());
    }

    let repo_path = args
        .repository
        .canonicalize()
        .context("Cannot find the specified repository")?;
    println!("Using repository in {}", repo_path.display());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1024., 768.]),
        ..Default::default()
    };
    eframe::run_native(
        &format!(
            "Revset Explorer - {}",
            repo_path.file_name().unwrap_or_default().display()
        ),
        options,
        Box::new(|_cc| Ok(Box::new(ExplorerApp::new(&repo_path)))),
    )
    .unwrap();
    Ok(())
}

fn create_sample_repo() -> Result<(), anyhow::Error> {
    let sample_repo_path = PathBuf::from("revset-sample");
    if sample_repo_path.exists() {
        anyhow::bail!(
            "Sample repository directory \"{}\" already exists. Please remove it first.",
            sample_repo_path.display()
        );
    }
    let sample_script = include_str!("create_sample_repo.sh");
    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(&sample_script)
        .stderr(std::process::Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    if !output.status.success() {
        println!("{}", String::from_utf8_lossy(&output.stderr));
        anyhow::bail!("Failed to create sample repository");
    }
    println!(
        "Sample repository created in \"{0}\". Run the following command to explore it:\nrevset-explorer -R {0}",
        sample_repo_path.display()
    );
    Ok(())
}

struct ExplorerApp {
    filter_revset: RevsetEntry,
    view_revset: RevsetEntry,
    graph: GraphType,
    node_idxs: Vec<petgraph::graph::NodeIndex>,
    jj_graph: jjgraph::JjGraph,
    working_copy_commit_id: Option<CommitId>,
}

struct RevsetEntry {
    value: String,
    old_value: String,
    error: Option<String>,
}

impl RevsetEntry {
    fn new(initial_value: &str) -> Self {
        Self {
            value: initial_value.to_owned(),
            old_value: "".to_owned(),
            error: None,
        }
    }
}

#[derive(Debug, PartialEq)]
enum CreateError {
    RevsetParseError(String),
    JjError(String),
}

fn create_graph(
    jj_graph: &jjgraph::JjGraph,
    revset_str: &str,
) -> Result<(GraphType, Vec<petgraph::graph::NodeIndex>, bool), CreateError> {
    let mut graph: GraphType =
        egui_graphs::Graph::new(petgraph::stable_graph::StableGraph::default());

    let all_revset = jj_graph
        .get_revset(revset_str)
        .map_err(|e| CreateError::RevsetParseError(e.to_string()))?;

    let repo = jj_graph.get_repo();
    let working_copy_commit_id = repo
        .view()
        .get_wc_commit_id(jj_lib::ref_name::WorkspaceName::DEFAULT);
    let store = repo.store();
    let mut node_idxs = vec![];
    let mut node_map = HashMap::new();
    let mut edges = vec![];
    // TODO: Warn when max nodes is hit
    for rev in all_revset.iter_graph().take(MAX_NODES) {
        let rev = rev.map_err(|e| CreateError::JjError(e.to_string()))?;
        let commit_id = rev.0;
        let commit_edges = rev.1;
        let commit = store
            .get_commit(&commit_id)
            .map_err(|e| CreateError::JjError(e.to_string()))?;
        let change_id = commit.change_id();
        let change_id_len = repo
            .shortest_unique_change_id_prefix_len(change_id)
            .map_err(|e| CreateError::JjError(e.to_string()))?;
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
        let Some(start) = node_map.get(&edge.0) else {
            continue;
        };
        let Some(end) = node_map.get(&edge.1) else {
            continue;
        };
        graph.add_edge_with_label(*start, *end, (), "".to_owned());
    }

    let limit_hit = node_idxs.len() == MAX_NODES;

    Ok((graph, node_idxs, limit_hit))
}

#[derive(Debug, PartialEq)]
enum MarkError {
    RevsetParseError(String),
    JjError,
}

impl ExplorerApp {
    fn new(repository_path: &Path) -> Self {
        let initial_filter = "@".to_owned();
        // This is the default log macro in jj: present(@) | ancestors(immutable_heads().., 2) | present(trunk())
        let initial_view =
            "present(@) | ancestors(immutable_heads().., 5) | present(trunk())".to_owned();
        let jj_graph = jjgraph::JjGraph::new(repository_path).unwrap();
        let (g, node_idxs, _) = create_graph(&jj_graph, &initial_view).unwrap();
        let repo = jj_graph.get_repo();
        let working_copy_commit_id = repo
            .view()
            .get_wc_commit_id(jj_lib::ref_name::WorkspaceName::DEFAULT);
        Self {
            filter_revset: RevsetEntry::new(&initial_filter),
            view_revset: RevsetEntry::new(&initial_view),
            graph: g,
            node_idxs,
            jj_graph,
            working_copy_commit_id: working_copy_commit_id.cloned(),
        }
    }

    fn mark_graph(&mut self) -> anyhow::Result<(), MarkError> {
        let filter_revset = self.jj_graph.get_revset(&self.filter_revset.value);

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
            .get_revset("immutable()")
            .map_err(|_| MarkError::JjError)?;

        let is_immutable = immutable_revset.containing_fn();

        for node_idx in self.node_idxs.iter() {
            let node = self.graph.node_mut(*node_idx).unwrap();
            let commit_id = node.payload();
            let immutable = is_immutable(commit_id).map_err(|_| MarkError::JjError)?;
            let matches_filter = in_filter(commit_id).map_err(|_| MarkError::JjError)?;
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
            Err(MarkError::RevsetParseError(e.to_string()))
        } else {
            Ok(())
        }
    }
}

impl eframe::App for ExplorerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let revset_edit = |ui: &mut egui::Ui, label: &str, revset: &mut RevsetEntry| {
            let revset_label = ui.label(label);
            ui.scope(|ui| {
                if revset.error.is_some() {
                    ui.visuals_mut().extreme_bg_color = ecolor::Color32::DARK_RED;
                }
                ui.text_edit_singleline(&mut revset.value)
                    .labelled_by(revset_label.id);
            });
            let err_msg = if let Some(err_msg) = revset.error.as_ref() {
                // Remove empty lines, to make the error message more compact
                err_msg.replace("  |\n", "")
            } else {
                "".to_owned()
            };
            let _error_label = ui.add_sized(
                [1., ui.text_style_height(&egui::TextStyle::Monospace) * 4.],
                egui::Label::new(RichText::new(err_msg).family(egui::FontFamily::Monospace)),
            );
        };

        egui::CentralPanel::default().show(ctx, |ui| {
            let view_updated = self.view_revset.value != self.view_revset.old_value;
            if view_updated {
                let create_result = create_graph(&self.jj_graph, &self.view_revset.value);
                self.view_revset.error = match create_result {
                    Ok((g, node_idxs, limit_hit)) => {
                        self.graph = g;
                        self.node_idxs = node_idxs;
                        egui_graphs::reset_layout::<egui_graphs::LayoutStateHierarchical>(ui, None);
                        if limit_hit {
                            Some(format!("Node limit reached. The graph is incomplete."))
                        } else {
                            None
                        }
                    }
                    Err(CreateError::RevsetParseError(msg)) => Some(msg),
                    Err(CreateError::JjError(msg)) => Some(msg),
                };
                self.view_revset.old_value = self.view_revset.value.clone();
            }

            if view_updated || self.filter_revset.value != self.filter_revset.old_value {
                let update_result = self.mark_graph();
                self.filter_revset.error = match update_result {
                    Ok(()) => None,
                    Err(MarkError::RevsetParseError(msg)) => Some(msg),
                    Err(MarkError::JjError) => None,
                };
                self.filter_revset.old_value = self.filter_revset.value.clone();
            }

            ui.horizontal(|ui| {
                revset_edit(ui, "Select: ", &mut self.filter_revset);
                revset_edit(ui, "View: ", &mut self.view_revset);
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
