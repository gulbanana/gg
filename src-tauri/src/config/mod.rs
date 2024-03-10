use jj_lib::settings::UserSettings;

pub trait GGSettings {
    fn check_immutable(&self) -> bool;
    fn theme_override(&self) -> Option<String>;
}

impl GGSettings for UserSettings {
    fn check_immutable(&self) -> bool {
        self.config().get_bool("gg.check-immutable").unwrap_or(true)
    }

    fn theme_override(&self) -> Option<String> {
        self.config().get_string("gg.theme-override").ok()
    }
}
