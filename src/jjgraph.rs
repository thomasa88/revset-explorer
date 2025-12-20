use jj_lib::config::StackedConfig;
use jj_lib::ref_name::WorkspaceName;
use jj_lib::repo::{ReadonlyRepo, RepoLoader, StoreFactories};
use jj_lib::repo_path::RepoPathUiConverter;
use jj_lib::revset::{self, Revset, RevsetDiagnostics, RevsetWorkspaceContext};
use jj_lib::revset::{
    RevsetAliasesMap, RevsetExtensions, RevsetParseContext, SymbolResolver, SymbolResolverExtension,
};
use jj_lib::settings::UserSettings;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;

pub struct JjGraph {
    path_converter: RepoPathUiConverter,
    aliases_map: RevsetAliasesMap,
    repo: Arc<ReadonlyRepo>,
    revset_exts: RevsetExtensions,
    resolver_exts: Vec<Box<dyn SymbolResolverExtension>>,
}

#[derive(Error, Debug)]
pub enum RevsetError {
    #[error("Failed to parse revset: {0}")]
    ParseError(String),
}

impl JjGraph {
    pub fn new() -> anyhow::Result<Self> {
        let path_converter = RepoPathUiConverter::Fs {
            cwd: PathBuf::from_str(".").unwrap(),
            base: PathBuf::from_str(".").unwrap(),
        };
        let settings = UserSettings::from_config(StackedConfig::with_defaults())?;
        let store_factories = StoreFactories::default();
        let repo =
            RepoLoader::init_from_file_system(&settings, Path::new(".jj/repo"), &store_factories)?
                .load_at_head()?;
        Ok(Self {
            path_converter,
            aliases_map: RevsetAliasesMap::new(),
            repo,
            revset_exts: RevsetExtensions::new(),
            resolver_exts: vec![],
        })
    }

    pub fn get_revset<'r>(&'r self, revset_str: &str) -> Result<Box<dyn Revset + 'r>, RevsetError> {
        let now = chrono::Local::now();

        let resolver = SymbolResolver::new(self.repo.as_ref(), &self.resolver_exts);
        let workspace = RevsetWorkspaceContext {
            path_converter: &self.path_converter,
            workspace_name: WorkspaceName::DEFAULT,
        };
        let context = RevsetParseContext {
            aliases_map: &self.aliases_map,
            local_variables: HashMap::new(),
            user_email: "",
            date_pattern_context: now.into(),
            default_ignored_remote: None,
            use_glob_by_default: false,
            extensions: &self.revset_exts,
            workspace: Some(workspace),
        };

        let mut diagnostics = RevsetDiagnostics::new();
        let (expr, _modifier) = revset::parse_with_modifier(&mut diagnostics, revset_str, &context)
            .map_err(|e| RevsetError::ParseError(e.to_string()))?;
        let resolved = expr
            .resolve_user_expression(self.repo.as_ref(), &resolver)
            .map_err(|e| RevsetError::ParseError(e.to_string()))?;
        let revset = resolved
            .evaluate(self.repo.as_ref())
            .map_err(|e| RevsetError::ParseError(e.to_string()))?;

        Ok(revset)
    }

    pub fn get_repo(&self) -> Arc<ReadonlyRepo> {
        self.repo.clone()
    }
}
