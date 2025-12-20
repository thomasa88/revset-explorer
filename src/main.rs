use std::fs::File;
use std::io::Write;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::Context;
use jj_lib::repo::Repo;
use jj_lib::revset::Revset;
use jj_lib::{
    config::StackedConfig,
    ref_name::WorkspaceName,
    repo::{RepoLoader, StoreFactories},
    repo_path::RepoPathUiConverter,
    revset::{
        self, RevsetAliasesMap, RevsetDiagnostics, RevsetExtensions, RevsetParseContext,
        RevsetWorkspaceContext, SymbolResolver, SymbolResolverExtension,
    },
    settings::UserSettings,
};

fn main() -> anyhow::Result<()> {
    let now = chrono::Local::now();
    let path_converter = RepoPathUiConverter::Fs {
        cwd: PathBuf::from_str(".")?,
        base: PathBuf::from_str(".")?,
    };
    let workspace = RevsetWorkspaceContext {
        path_converter: &path_converter,
        workspace_name: WorkspaceName::DEFAULT,
    };
    let context = RevsetParseContext {
        aliases_map: &RevsetAliasesMap::new(),
        local_variables: HashMap::new(),
        user_email: "",
        date_pattern_context: now.into(),
        default_ignored_remote: None,
        use_glob_by_default: false,
        extensions: &RevsetExtensions::new(),
        workspace: Some(workspace),
    };

    let settings = UserSettings::from_config(StackedConfig::with_defaults())?;
    let store_factories = StoreFactories::default();
    let repo =
        RepoLoader::init_from_file_system(&settings, Path::new(".jj/repo"), &store_factories)?
            .load_at_head()?;
    let store = repo.store();
    let extensions: Vec<Box<dyn SymbolResolverExtension>> = vec![];
    let resolver = SymbolResolver::new(repo.as_ref(), &extensions);
    // let mut diagnostics = RevsetDiagnostics::new();

    let filter_spec = std::env::args()
        .nth(1)
        .context("Expected a revset specification as the first argument")?;
    let filter_revset = get_revset(&*repo, &resolver, &context, &filter_spec)?;
    // let all_revset = get_revset(&*repo, &resolver, &context, "::")?;
    // present(@) | ancestors(immutable_heads().., 2) | present(trunk())
    let all_revset = get_revset(&*repo, &resolver, &context, "::")?;
    // TODO: Import the revset aliases from the config
    // let immutable_revset = get_revset(&*repo, &resolver, &context, "immutable()")?;
    let immutable_revset = get_revset(
        &*repo,
        &resolver,
        &context,
        "::(present(latest(
  remote_bookmarks(exact:\"main\", exact:\"origin\") |
  remote_bookmarks(exact:\"master\", exact:\"origin\") |
  remote_bookmarks(exact:\"trunk\", exact:\"origin\") |
  remote_bookmarks(exact:\"main\", exact:\"upstream\") |
  remote_bookmarks(exact:\"master\", exact:\"upstream\") |
  remote_bookmarks(exact:\"trunk\", exact:\"upstream\") |
  root()
)) | tags() | untracked_remote_bookmarks() | root())",
    )?;

    let in_filter = filter_revset.containing_fn();
    let is_immutable = immutable_revset.containing_fn();

    let mut dot_file = File::create("graph.dot")?;
    writeln!(dot_file, "digraph G {{")?;
    writeln!(dot_file, "label=\"{}\";", filter_spec)?;
    writeln!(dot_file, "labelloc=top;")?;
    // writeln!(dot_file, "fontsize=20;")?;
    for r in all_revset.iter_graph() {
        let r = r.unwrap();
        let commit_id = r.0;
        // all_revset.commit_change_ids();
        let edges = r.1;
        let commit = store.get_commit(&commit_id)?;
        let change_id = commit.change_id();
        let change_id_len = repo.shortest_unique_change_id_prefix_len(&change_id)?;
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
        let desc = desc.replace('"', "\\\"");
        let mut attrs = vec![];
        let mut style = String::new();
        if in_filter(&commit_id)? {
            style += "filled,";
            attrs.push("fillcolor=lightblue");
        }
        if is_immutable(&commit_id)? {
            style += "dashed,";
        };
        let style = &format!("style=\"{style}\"");
        attrs.push(style);
        let attrs = attrs.join(", ");
        writeln!(
            dot_file,
            "    \"{commit_id}\" [label=\"{change_id_prefix}: {desc}\", {attrs}];"
        )?;
        for edge in edges {
            writeln!(dot_file, "    \"{}\" -> \"{}\";", commit_id, edge.target)?;
        }
    }
    writeln!(dot_file, "}}")?;
    drop(dot_file); // Ensure the file is closed before running the command

    // Use the `dot` command from Graphviz to generate a PNG
    let output = std::process::Command::new("dot")
        .args(&["-Tpng", "graph.dot", "-o", "graph.png"])
        .output()?;

    if !output.status.success() {
        eprintln!(
            "Failed to generate PNG: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    } else {
        println!("PNG file 'graph.png' generated successfully.");
    }
    // Grouped by heads/branches
    // let topo_iter = TopoGroupedGraphIterator::new(revset.iter_graph(), |id| id);
    // for node in topo_iter {
    //     dbg!(&node);
    // }

    Ok(())
}

fn get_revset<'index>(
    repo: &'index dyn Repo,
    resolver: &SymbolResolver,
    context: &RevsetParseContext<'_>,
    revset_str: &str,
) -> anyhow::Result<Box<dyn Revset + 'index>> {
    let mut diagnostics = RevsetDiagnostics::new();
    let (expr, _modifier) = revset::parse_with_modifier(&mut diagnostics, revset_str, &context)?;
    let resolved = expr.resolve_user_expression(repo, &resolver)?;
    let revset = resolved.evaluate(repo)?;
    return Ok(revset);
}
