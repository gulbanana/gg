use jj_lib::settings::UserSettings;

pub trait GGSettings {
    fn query_large_repo_heuristic(&self) -> i64;
    fn query_auto_snapshot(&self) -> Option<bool>;
    fn query_check_immutable(&self) -> Option<bool>;
    fn ui_theme_override(&self) -> Option<String>;
}

impl GGSettings for UserSettings {
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
}
