#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{path::Path, sync::Arc};

use anyhow::{anyhow, Result};
use itertools::Itertools;
use jj_cli::{cli_util::start_repo_transaction, config::{default_config, LayeredConfigs}};
use jj_lib::{backend::CommitId, id_prefix::IdPrefixContext, op_heads_store, operation::Operation, repo::{ReadonlyRepo, Repo, RepoLoader, StoreFactories}, revset::{self, RevsetAliasesMap, RevsetIteratorExt, RevsetParseContext, RevsetWorkspaceContext}, settings::{ConfigResultExt, UserSettings}, workspace::{self, WorkspaceLoader}};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![load_log])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn load_log() -> String {
    match get_log() {
        Ok(x) => x,
        Err(err) => format!("{err}")
    }
}

fn get_log() -> Result<String> {
    let cwd = std::env::current_dir()?;
    let loader = WorkspaceLoader::init(find_workspace_dir(&cwd))?;

    let mut configs = LayeredConfigs::from_environment(default_config());
    configs.read_user_config()?;
    configs.read_repo_config(loader.repo_path())?;
    let config = configs.merge();
    let settings = UserSettings::from_config(config);

    let workspace = loader.load(&settings, &StoreFactories::default(), &workspace::default_working_copy_factories())?;
    let op_head = resolve_op_head(&settings, workspace.repo_loader())?;
    let repo = workspace.repo_loader().load_at(&op_head)?;

    let workspace_context = RevsetWorkspaceContext {
        cwd: &cwd,
        workspace_id: workspace.workspace_id(),
        workspace_root: workspace.workspace_root()
    };
    let parse_context = RevsetParseContext {
        aliases_map: &load_revset_aliases(&configs)?,
        user_email: settings.user_email(),
        workspace: Some(workspace_context)
    };

    let default_revset = "@ | ancestors(immutable_heads().., 2) | heads(immutable_heads())";
    let expression = revset::parse(default_revset, &parse_context)?;
    let expression = revset::optimize(expression);
    let prefix_context = IdPrefixContext::default();
    let symbol_resolver = revset_symbol_resolver(&repo, &prefix_context)?;
    let resolved_expression =
    expression.resolve_user_expression(repo.as_ref(), &symbol_resolver)?;
    let revset = resolved_expression.evaluate(repo.as_ref())?;

    let mut output = String::new();
    for commit in revset.iter().commits(repo.store()).take(10) {
        output += commit?.description();
        output += "\n";
    }

    Ok(output)
}

fn find_workspace_dir(cwd: &Path) -> &Path {
    cwd.ancestors()
        .find(|path| path.join(".jj").is_dir())
        .unwrap_or(cwd)
}

fn load_revset_aliases(
    layered_configs: &LayeredConfigs,
) -> Result<RevsetAliasesMap> {
    const TABLE_KEY: &str = "revset-aliases";
    let mut aliases_map = RevsetAliasesMap::new();
    // Load from all config layers in order. 'f(x)' in default layer should be
    // overridden by 'f(a)' in user.
    for (_, config) in layered_configs.sources() {
        let table = if let Some(table) = config.get_table(TABLE_KEY).optional()? {
            table
        } else {
            continue;
        };
        for (decl, value) in table.into_iter().sorted_by(|a, b| a.0.cmp(&b.0)) {
            value
                .into_string()
                .map_err(|e| anyhow!(e))
                .and_then(|v| aliases_map.insert(&decl, v).map_err(|e| anyhow!(e)))?;
        }
    }
    Ok(aliases_map)
}

pub fn resolve_op_head(settings: &UserSettings, repo_loader: &RepoLoader) -> Result<Operation> {
    op_heads_store::resolve_op_heads(
        repo_loader.op_heads_store().as_ref(),
        repo_loader.op_store(),
        |op_heads| {
            let base_repo = repo_loader.load_at(&op_heads[0])?;
            let mut tx =
                start_repo_transaction(&base_repo, &settings, &vec![]);
            for other_op_head in op_heads.into_iter().skip(1) {
                tx.merge_operation(other_op_head)?;
                let _num_rebased = tx.mut_repo().rebase_descendants(&settings)?;
            }
            Ok(tx
                .write("resolve concurrent operations")
                .leave_unpublished()
                .operation()
                .clone())
        }
    )
}

fn revset_symbol_resolver<'context>(repo: &'context Arc<ReadonlyRepo>, id_prefix_context: &'context IdPrefixContext) -> Result<revset::DefaultSymbolResolver<'context>> {
    let commit_id_resolver: revset::PrefixResolver<CommitId> =
        Box::new(|repo, prefix| id_prefix_context.resolve_commit_prefix(repo, prefix));
    let change_id_resolver: revset::PrefixResolver<Vec<CommitId>> =
        Box::new(|repo, prefix| id_prefix_context.resolve_change_prefix(repo, prefix));
    let symbol_resolver = revset::DefaultSymbolResolver::new(repo.as_ref())
        .with_commit_id_resolver(commit_id_resolver)
        .with_change_id_resolver(change_id_resolver);
    Ok(symbol_resolver)
}
