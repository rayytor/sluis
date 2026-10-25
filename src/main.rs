mod application;
mod checks;
mod config;
mod helper;
mod methods;
mod network;
mod pages;
mod setup;
mod ui;
mod window;

use adw::prelude::*;
use gettextrs::{LocaleCategory, gettext};
use gtk::{gio, glib};

fn main() -> glib::ExitCode {
    // SAFETY: first thing in main, before GTK or anything else starts a thread
    unsafe {
        gettextrs::setlocale(LocaleCategory::LcAll, "");
    }
    gettextrs::bindtextdomain(config::GETTEXT_PACKAGE, config::LOCALEDIR)
        .expect("cannot bind the text domain");
    gettextrs::bind_textdomain_codeset(config::GETTEXT_PACKAGE, "UTF-8")
        .expect("cannot set the text domain encoding");
    gettextrs::textdomain(config::GETTEXT_PACKAGE).expect("cannot switch to the text domain");

    gio::resources_register_include!("sluis.gresource").expect("cannot load resources");
    glib::set_application_name(&gettext("Sluis"));

    application::Application::new().run()
}
