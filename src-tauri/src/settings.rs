use jj_lib::settings::UserSettings;

pub trait GGSettings {
    fn check_immutable(&self) -> bool;
}

impl GGSettings for UserSettings {
    fn check_immutable(&self) -> bool {
        self.config().get_bool("gg.check-immutable").unwrap_or(true)
    }
}
