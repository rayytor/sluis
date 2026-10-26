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

    // On libadwaita 1.8+, app.shortcuts (Ctrl+?) comes from AdwApplication,
    // which loads shortcuts-dialog.ui from the resource base path. 1.7
    // (Debian 13) has neither, so we add the action and a plainer dialog.
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

        if adw::minor_version() < 8 {
            let shortcuts = gio::ActionEntry::builder("shortcuts")
                .activate(|app: &Self, _, _| app.show_shortcuts())
                .build();
            self.add_action_entries([shortcuts]);
            self.set_accels_for_action("app.shortcuts", &["<Control>question"]);
        }
    }

    // keep in step with shortcuts-dialog.ui
    fn show_shortcuts(&self) {
        let shortcuts = [
            (gettext("Preferences"), "app.preferences"),
            (gettext("Keyboard Shortcuts"), "app.shortcuts"),
            (gettext("Check Your Sites"), "win.check-sites"),
            (gettext("Unblock Sites"), "win.scan"),
            (gettext("Search Methods"), ""),
            (gettext("Close Window"), "window.close"),
            (gettext("Quit"), "app.quit"),
        ];
        let group = adw::PreferencesGroup::builder()
            .title(gettext("General"))
            .build();
        for (title, action) in shortcuts {
            let accel = if action.is_empty() {
                "<Control>f".to_owned()
            } else {
                self.accels_for_action(action)
                    .first()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            };
            let label = gtk::accelerator_parse(&accel)
                .map(|(key, mods)| gtk::accelerator_get_label(key, mods))
                .unwrap_or_default();
            let row = adw::ActionRow::builder().title(title).build();
            row.add_suffix(
                &gtk::Label::builder()
                    .label(label)
                    .css_classes(["dim-label"])
                    .build(),
            );
            group.add(&row);
        }
        let page = adw::PreferencesPage::new();
        page.add(&group);
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&page));
        let dialog = adw::Dialog::builder()
            .title(gettext("Keyboard Shortcuts"))
            .content_width(420)
            .child(&toolbar)
            .build();
        dialog.present(self.active_window().as_ref());
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
