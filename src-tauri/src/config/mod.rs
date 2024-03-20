use jj_lib::settings::UserSettings;

pub trait GGSettings {
    fn query_log_page_size(&self) -> usize;
    fn query_large_repo_heuristic(&self) -> i64;
    fn query_auto_snapshot(&self) -> Option<bool>;
    fn query_check_immutable(&self) -> Option<bool>;
    fn ui_theme_override(&self) -> Option<String>;
    fn ui_indicate_disconnected_branches(&self) -> bool;
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

    fn query_check_immutable(&self) -> Option<bool> {
        self.config().get_bool("gg.queries.check-immutable").ok()
    }

    fn ui_theme_override(&self) -> Option<String> {
        self.config().get_string("gg.ui.theme-override").ok()
    }

    fn ui_indicate_disconnected_branches(&self) -> bool {
        self.config()
            .get_bool("gg.ui.indicate-disconnected-branches")
            .unwrap_or(true)
    }
}
