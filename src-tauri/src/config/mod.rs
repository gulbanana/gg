use config::ConfigError;
use jj_lib::settings::UserSettings;

pub trait GGSettings {
    fn query_log_page_size(&self) -> usize;
    fn query_large_repo_heuristic(&self) -> i64;
    fn query_auto_snapshot(&self) -> Option<bool>;
    fn ui_theme_override(&self) -> Option<String>;
    fn ui_mark_unpushed_branches(&self) -> bool;
    fn ui_recent_workspaces(&self) -> Vec<String>;
}

impl GGSettings for UserSettings {
    fn query_log_page_size(&self) -> usize {
        self.config()
            .get_int("gg.queries.log-page-size")
            .unwrap_or(1000) as usize
    }

    fn query_large_repo_heuristic(&self) -> i64 {
        self.config()
            .get_int("gg.queries.large-repo-heuristic")
            .unwrap_or(100000)
    }

    fn query_auto_snapshot(&self) -> Option<bool> {
        self.config().get_bool("gg.queries.auto-snapshot").ok()
    }

    fn ui_theme_override(&self) -> Option<String> {
        self.config().get_string("gg.ui.theme-override").ok()
    }

    fn ui_mark_unpushed_branches(&self) -> bool {
        self.config()
            .get_bool("gg.ui.mark-unpushed-branches")
            .unwrap_or(true)
    }

    fn ui_recent_workspaces(&self) -> Vec<String> {
        let paths: Result<Vec<String>, ConfigError> = self
            .config()
            .get_array("gg.ui.recent-workspaces")
            .unwrap_or(vec![])
            .into_iter()
            .map(|value| value.into_string())
            .collect();
        paths.unwrap_or(vec![])
    }
}
