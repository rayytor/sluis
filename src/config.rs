pub const APP_ID: &str = "io.github.rayytor.Sluis";
pub const RESOURCE_BASE: &str = "/io/github/rayytor/Sluis";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GETTEXT_PACKAGE: &str = "sluis";

/// Set by meson for installed builds; `cargo run` falls back to the system dir.
pub const LOCALEDIR: &str = match option_env!("SLUIS_LOCALEDIR") {
    Some(dir) => dir,
    None => "/usr/share/locale",
};
