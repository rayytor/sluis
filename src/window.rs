use std::cell::OnceCell;
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::application::Application;
use crate::config::APP_ID;
use crate::ui::Ui;

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/rayytor/Sluis/ui/window.ui")]
    pub struct Window {
        #[template_child]
        pub toasts: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub nav: TemplateChild<adw::NavigationView>,
        pub ui: OnceCell<Rc<Ui>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "SluisWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.remember_size();
            obj.add_actions();
            let ui = Ui::new(obj.upcast_ref(), self.nav.get(), self.toasts.get());
            ui.settings.connect_changed(
                Some("show-technical-details"),
                glib::clone!(
                    #[weak]
                    ui,
                    move |_, _| ui.render_methods()
                ),
            );
            ui.start();
            let _ = self.ui.set(ui);
        }
    }

    impl WidgetImpl for Window {}
    impl WindowImpl for Window {}
    impl ApplicationWindowImpl for Window {}
    impl AdwApplicationWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
            gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl Window {
    pub fn new(app: &Application) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    pub fn ui(&self) -> Rc<Ui> {
        self.imp().ui.get().expect("set in constructed").clone()
    }

    pub fn toast(&self, title: &str) {
        self.imp().toasts.add_toast(adw::Toast::new(title));
    }

    /// Window actions are GActions, so GTK also offers them on D-Bus (the
    /// headless tests use that).
    fn add_actions(&self) {
        let check = gio::ActionEntry::builder("check-sites")
            .activate(|win: &Self, _, _| win.ui().check_sites())
            .build();
        let methods = gio::ActionEntry::builder("show-methods")
            .activate(|win: &Self, _, _| {
                let ui = win.ui();
                ui.render_methods();
                ui.nav.push(&ui.methods_page.page);
            })
            .build();
        let sites = gio::ActionEntry::builder("show-sites")
            .activate(|win: &Self, _, _| {
                let ui = win.ui();
                ui.render_sites();
                ui.nav.push(&ui.sites_page.page);
            })
            .build();
        let use_method = gio::ActionEntry::builder("use-method")
            .parameter_type(Some(glib::VariantTy::STRING))
            .activate(|win: &Self, _, arg| {
                if let Some(args) = arg.and_then(|a| a.get::<String>()) {
                    win.ui().switch_to(args);
                }
            })
            .build();
        let get_started = gio::ActionEntry::builder("get-started")
            .activate(|win: &Self, _, _| win.ui().get_started())
            .build();
        let scan = gio::ActionEntry::builder("scan")
            .activate(|win: &Self, _, _| win.ui().show_scan())
            .build();
        // the scan dialog's Start and Stop; start-deep-scan skips its
        // Quick/Deep choice
        let start_scan = gio::ActionEntry::builder("start-scan")
            .activate(|win: &Self, _, _| win.ui().start_chosen_scan(None))
            .build();
        let start_deep_scan = gio::ActionEntry::builder("start-deep-scan")
            .activate(|win: &Self, _, _| win.ui().start_chosen_scan(Some(true)))
            .build();
        let stop_scan = gio::ActionEntry::builder("stop-scan")
            .activate(|win: &Self, _, _| win.ui().stop_scan())
            .build();
        let undo = gio::ActionEntry::builder("undo-switch")
            .activate(|win: &Self, _, _| win.ui().undo_switch())
            .build();
        self.add_action_entries([
            check,
            get_started,
            methods,
            sites,
            use_method,
            scan,
            start_scan,
            start_deep_scan,
            stop_scan,
            undo,
        ]);
    }

    fn remember_size(&self) {
        let settings = gio::Settings::new(APP_ID);
        settings.bind("window-width", self, "default-width").build();
        settings
            .bind("window-height", self, "default-height")
            .build();
        settings.bind("window-maximized", self, "maximized").build();
    }
}
