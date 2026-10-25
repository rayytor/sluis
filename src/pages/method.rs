//! One method: how it did with each site, and its technical details for
//! experts (design/screens/04-strategy-details.png).

use std::rc::Rc;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use sluis_core::describe;

use crate::config::RESOURCE_BASE;
use crate::methods::Standing;
use crate::ui::Ui;

impl Ui {
    pub fn show_method(self: &Rc<Self>, args: &str) {
        let Some(method) = self.methods.borrow().get(args).cloned() else {
            return;
        };
        let b = gtk::Builder::from_resource(&format!("{RESOURCE_BASE}/ui/method-page.ui"));
        let page: adw::NavigationPage = b.object("page").unwrap();
        let preferences: adw::PreferencesPage = b.object("preferences").unwrap();
        let results: adw::PreferencesGroup = b.object("results").unwrap();
        let name_row: adw::ActionRow = b.object("name_row").unwrap();
        let args_row: adw::ActionRow = b.object("args_row").unwrap();
        let copy_button: gtk::Button = b.object("copy_button").unwrap();
        let use_button: gtk::Button = b.object("use_button").unwrap();

        page.set_title(&self.method_name(args));
        let sites = self.sites();
        preferences.set_description(&match self.methods.borrow().standing(args, &sites) {
            Standing::Works => gettext("Opened all of your sites the last time it was used."),
            Standing::Partly { .. } => {
                gettext("Left some of your sites blocked the last time it was used.")
            }
            Standing::NotTested => gettext(
                "Not used with your sites yet. Sluis checks them as soon as you switch to it.",
            ),
        });

        for site in &sites {
            let row = adw::ActionRow::builder()
                .title(Self::site_name(site))
                .subtitle(site)
                .build();
            let result = method.result.as_ref();
            let (text, icon, class) = if result.is_some_and(|r| r.opened.contains(site)) {
                (gettext("Opened"), Some("object-select-symbolic"), "success")
            } else if result.is_some_and(|r| r.blocked.contains(site)) {
                (
                    gettext("Blocked"),
                    Some("dialog-warning-symbolic"),
                    "warning",
                )
            } else {
                (gettext("Not checked"), None, "dim-label")
            };
            let label = gtk::Label::new(Some(&text));
            label.add_css_class(if class == "success" {
                "dim-label"
            } else {
                class
            });
            row.add_suffix(&label);
            if let Some(icon) = icon {
                let image = gtk::Image::builder()
                    .icon_name(icon)
                    .accessible_role(gtk::AccessibleRole::Presentation)
                    .build();
                image.add_css_class(class);
                row.add_suffix(&image);
            }
            results.add(&row);
        }
        results.set_visible(!sites.is_empty());

        name_row.set_subtitle(&describe(&method.strategy()).to_string());
        args_row.set_title(args);
        copy_button.connect_clicked(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            #[strong]
            args_row,
            move |_| {
                ui.window.clipboard().set_text(&args_row.title());
                ui.toast(&gettext("Copied"));
            }
        ));

        let in_use = self.current_method().as_deref() == Some(args);
        use_button.set_visible(!in_use);
        use_button.set_action_target_value(Some(&args.to_variant()));
        use_button.set_action_name(Some("win.use-method"));
        use_button.connect_clicked(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| {
                ui.nav.pop();
            }
        ));
        self.nav.push(&page);
    }
}
