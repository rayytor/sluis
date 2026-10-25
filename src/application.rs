use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::config::{APP_ID, RESOURCE_BASE, VERSION};
use crate::window::Window;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Application;

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "SluisApplication";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {}

    impl ApplicationImpl for Application {
        fn startup(&self) {
            self.parent_startup();
            self.obj().setup_actions();
        }

        fn activate(&self) {
            let app = self.obj();
            let window = app
                .active_window()
                .unwrap_or_else(|| Window::new(&app).upcast());
            window.present();
        }
    }

    impl GtkApplicationImpl for Application {}
    impl AdwApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends adw::Application, gtk::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Application {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", APP_ID)
            .property("resource-base-path", RESOURCE_BASE)
            .build()
    }

    // app.shortcuts (Ctrl+?) comes from AdwApplication, which loads
    // shortcuts-dialog.ui from the resource base path.
    fn setup_actions(&self) {
        let quit = gio::ActionEntry::builder("quit")
            .activate(|app: &Self, _, _| app.quit())
            .build();
        let preferences = gio::ActionEntry::builder("preferences")
            .activate(|app: &Self, _, _| {
                if let Some(win) = app.active_window().and_downcast::<Window>() {
                    win.ui().present_preferences();
                }
            })
            .build();
        let about = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| app.show_about())
            .build();
        // a scan's notification: bring the window back with the result
        let show_scan = gio::ActionEntry::builder("show-scan")
            .activate(|app: &Self, _, _| {
                app.activate();
                if let Some(win) = app.active_window() {
                    let _ = WidgetExt::activate_action(&win, "win.scan", None);
                }
            })
            .build();
        self.add_action_entries([quit, preferences, about, show_scan]);

        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("app.preferences", &["<Control>comma"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
        self.set_accels_for_action("win.check-sites", &["<Control>r"]);
        self.set_accels_for_action("win.scan", &["<Control><Shift>s"]);
    }

    fn show_about(&self) {
        let about =
            adw::AboutDialog::from_appdata(&format!("{RESOURCE_BASE}/metainfo.xml"), Some(VERSION));
        about.set_developers(&["rayytor https://github.com/rayytor"]);
        about.set_copyright("© 2026 rayytor");
        about.add_credit_section(
            Some(&gettext("Zapret engine")),
            &["bol-van https://github.com/bol-van/zapret"],
        );
        // Translators: replace with your name and email, one per line
        about.set_translator_credits(&gettext("translator-credits"));
        about.present(self.active_window().as_ref());
    }
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}
