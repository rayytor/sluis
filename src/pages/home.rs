//! Home: does everything work? (design/screens/01-home.png, 02-home-problem.png)

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::{gio, glib};

use crate::checks::Check;
use crate::config::RESOURCE_BASE;
use crate::methods::Standing;
use crate::ui::{Ui, list};

pub struct HomePage {
    pub page: adw::NavigationPage,
    title: adw::WindowTitle,
    banner: adw::Banner,
    hero_icon_stack: gtk::Stack,
    hero_icon: gtk::Image,
    hero_title: gtk::Label,
    hero_description: gtk::Label,
    hero_button: gtk::Button,
    unblocking: adw::SwitchRow,
    sites_group: adw::PreferencesGroup,
    site_rows: RefCell<Vec<adw::ActionRow>>,
    method_row: adw::ActionRow,
    sites_row: adw::ActionRow,
    check_button: gtk::Button,
    /// Set while the switch is moved from code, not by the user.
    rendering: Cell<bool>,
}

/// What the hero says.
enum Hero {
    Loading,
    HelperFailed,
    Off,
    NoSites,
    Checking,
    Unblocked(Vec<String>),
    Blocked(Vec<String>),
    /// Nothing is blocked that a method could fix, but the internet
    /// provider's DNS gives wrong addresses for these.
    WrongAddress(Vec<String>),
}

impl HomePage {
    pub fn new(menu: Option<&gio::MenuModel>) -> Self {
        let b = gtk::Builder::from_resource(&format!("{RESOURCE_BASE}/ui/home-page.ui"));
        b.object::<gtk::MenuButton>("menu_button")
            .unwrap()
            .set_menu_model(menu);
        Self {
            page: b.object("page").unwrap(),
            title: b.object("title").unwrap(),
            banner: b.object("banner").unwrap(),
            hero_icon_stack: b.object("hero_icon_stack").unwrap(),
            hero_icon: b.object("hero_icon").unwrap(),
            hero_title: b.object("hero_title").unwrap(),
            hero_description: b.object("hero_description").unwrap(),
            hero_button: b.object("hero_button").unwrap(),
            unblocking: b.object("unblocking").unwrap(),
            sites_group: b.object("sites_group").unwrap(),
            site_rows: RefCell::default(),
            method_row: b.object("method_row").unwrap(),
            sites_row: b.object("sites_row").unwrap(),
            check_button: b.object("check_button").unwrap(),
            rendering: Cell::new(false),
        }
    }
}

impl Ui {
    pub fn connect_home(self: &Rc<Self>) {
        let home = &self.home;
        home.unblocking.connect_active_notify(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |row| {
                if !ui.home.rendering.get() {
                    ui.set_enabled(row.is_active());
                }
            }
        ));
        home.banner.connect_button_clicked(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| {
                if ui.status().is_some_and(|s| !s.problems.is_empty()) {
                    ui.repair();
                } else {
                    ui.update_zapret();
                }
            }
        ));
        home.hero_button.connect_clicked(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| {
                let action = if ui.sites().is_empty() {
                    "win.show-sites"
                } else if matches!(ui.hero(), Hero::WrongAddress(_)) {
                    // no method helps; check again once secure DNS is on
                    "win.check-sites"
                } else {
                    "win.scan"
                };
                let _ = WidgetExt::activate_action(&ui.window, action, None);
            }
        ));
    }

    fn hero(&self) -> Hero {
        if self.helper_failed() {
            return Hero::HelperFailed;
        }
        let Some(status) = self.status() else {
            return Hero::Loading;
        };
        if !status.running {
            return Hero::Off;
        }
        let checks = self.checks();
        if checks.is_empty() {
            return Hero::NoSites;
        }
        if checks.values().any(|c| *c == Check::Pending) {
            return Hero::Checking;
        }
        let of = |kind: fn(&Check) -> bool| -> Vec<String> {
            checks
                .iter()
                .filter(|(_, c)| kind(c))
                .map(|(s, _)| s.clone())
                .collect()
        };
        // what a method can fix comes first
        let blocked = of(Check::is_blocked);
        let wrong = of(|c| *c == Check::WrongAddress);
        if !blocked.is_empty() {
            Hero::Blocked(blocked)
        } else if !wrong.is_empty() {
            Hero::WrongAddress(wrong)
        } else {
            Hero::Unblocked(checks.into_keys().collect())
        }
    }

    pub fn render_home(self: &Rc<Self>) {
        let home = &self.home;
        home.rendering.set(true);
        let status = self.status();
        let names = |sites: &[String]| sites.iter().map(|s| Self::site_name(s)).collect::<Vec<_>>();

        // hero
        let (icon, style, title, description, button) = match self.hero() {
            Hero::Loading => (
                None,
                "busy",
                gettext("Checking Zapret…"),
                String::new(),
                None,
            ),
            Hero::HelperFailed => (
                Some("network-error-symbolic"),
                "bad",
                gettext("Can’t Reach Zapret"),
                gettext(
                    "The helper that changes Zapret’s settings isn’t working. Reinstalling Sluis usually fixes this.",
                ),
                None,
            ),
            Hero::Off => (
                Some("sluis-chained-symbolic"),
                "",
                gettext("Unblocking Is Off"),
                gettext("Turn it on to open the sites your internet provider blocks."),
                None,
            ),
            Hero::NoSites => (
                Some("sluis-freed-symbolic"),
                "ok",
                gettext("Sluis Is On"),
                gettext("Add the sites you use, and Sluis will check that they open."),
                Some(gettext("_Add Sites")),
            ),
            Hero::Checking => (
                None,
                "busy",
                gettext("Checking Your Sites…"),
                String::new(),
                None,
            ),
            Hero::Unblocked(sites) => (
                Some("sluis-freed-symbolic"),
                "ok",
                gettext("Your Sites Are Unblocked"),
                match sites.len() {
                    1 => gettext("{site} opens").replace("{site}", &names(&sites)[0]),
                    _ => gettext("{sites} all open").replace("{sites}", &list(&names(&sites))),
                },
                None,
            ),
            Hero::Blocked(sites) => (
                Some("sluis-chained-symbolic"),
                "bad",
                match sites.len() {
                    1 => gettext("{site} Is Blocked").replace("{site}", &names(&sites)[0]),
                    n => ngettext("{n} Site Is Blocked", "{n} Sites Are Blocked", n as u32)
                        .replace("{n}", &n.to_string()),
                },
                ngettext(
                    "Your internet provider found a way to block it. Another method can usually get around it.",
                    "Your internet provider found a way to block them. Another method can usually get around that.",
                    sites.len() as u32,
                ),
                Some(gettext("_Unblock It")),
            ),
            Hero::WrongAddress(sites) => (
                Some("sluis-chained-symbolic"),
                "bad",
                gettext("Your Internet Provider Gives Wrong Addresses"),
                ngettext(
                    "Your internet provider sends {sites} to the wrong place, so no method can open it. Turn on secure DNS (DNS over HTTPS) in your browser or in your network settings.",
                    "Your internet provider sends {sites} to the wrong place, so no method can open them. Turn on secure DNS (DNS over HTTPS) in your browser or in your network settings.",
                    sites.len() as u32,
                )
                .replace("{sites}", &list(&names(&sites))),
                Some(gettext("_Check Again")),
            ),
        };
        match icon {
            Some(icon) => {
                home.hero_icon.set_icon_name(Some(icon));
                for class in ["ok", "bad", "busy"] {
                    home.hero_icon.remove_css_class(class);
                }
                if !style.is_empty() {
                    home.hero_icon.add_css_class(style);
                }
                home.hero_icon_stack.set_visible_child_name("icon");
            }
            None => home.hero_icon_stack.set_visible_child_name("spinner"),
        }
        // screen readers hear a new state once, not every redraw
        if home.hero_title.label() != title {
            home.hero_title
                .announce(&title, gtk::AccessibleAnnouncementPriority::Medium);
        }
        home.hero_title.set_label(&title);
        home.hero_description.set_label(&description);
        home.hero_description.set_visible(!description.is_empty());
        home.hero_button.set_visible(button.is_some());
        if let Some(label) = button {
            home.hero_button.set_label(&label);
        }

        // the switch
        let running = status.as_ref().is_some_and(|s| s.running);
        home.unblocking.set_active(running);
        home.unblocking
            .set_sensitive(status.is_some() && !self.busy.get());
        home.unblocking.set_subtitle(&match &status {
            Some(s) if s.running && s.enabled => {
                gettext("On · Starts by itself with your computer")
            }
            Some(s) if s.running => gettext("On"),
            _ => gettext("Off"),
        });

        // a damaged config first, then an update
        home.title
            .set_subtitle(&self.network().map(|n| n.name).unwrap_or_default());
        let damaged = status.as_ref().is_some_and(|s| !s.problems.is_empty());
        if damaged {
            home.banner
                .set_title(&gettext("Zapret’s settings file is damaged"));
            home.banner.set_button_label(Some(&gettext("_Repair")));
        } else if let Some(version) = self.update_available() {
            home.banner.set_title(
                &gettext("Zapret {version} is available").replace("{version}", &version),
            );
            home.banner.set_button_label(Some(&gettext("_Update")));
        }
        home.banner
            .set_revealed(damaged || self.update_available().is_some());

        // site rows
        for row in home.site_rows.borrow_mut().drain(..) {
            home.sites_group.remove(&row);
        }
        let checks = self.checks();
        let mut rows = Vec::new();
        if checks.is_empty() {
            let row = adw::ActionRow::builder()
                .title(gettext("Add the sites you use"))
                .activatable(true)
                .action_name("win.show-sites")
                .build();
            row.add_suffix(
                &gtk::Image::builder()
                    .icon_name("go-next-symbolic")
                    .accessible_role(gtk::AccessibleRole::Presentation)
                    .build(),
            );
            rows.push(row);
        }
        for (site, check) in &checks {
            let row = adw::ActionRow::builder()
                .title(Self::site_name(site))
                .build();
            let (text, icon, class) = match check {
                Check::Pending => (String::new(), None, ""),
                Check::Opens => (gettext("Opens"), Some("object-select-symbolic"), "success"),
                Check::Blocked(reason) => {
                    row.set_subtitle(&format!("{site} · {reason}"));
                    (
                        gettext("Blocked"),
                        Some("dialog-warning-symbolic"),
                        "warning",
                    )
                }
                Check::WrongAddress => {
                    row.set_subtitle(&format!("{site} · {}", gettext("Turn on secure DNS")));
                    (
                        gettext("Wrong address"),
                        Some("dialog-warning-symbolic"),
                        "warning",
                    )
                }
            };
            if matches!(check, Check::Pending | Check::Opens) {
                row.set_subtitle(site);
            }
            if *check == Check::Pending {
                row.add_suffix(&adw::Spinner::new());
            } else {
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
                    image.set_tooltip_text(Some(&text));
                    row.add_suffix(&image);
                }
            }
            rows.push(row);
        }
        for row in &rows {
            home.sites_group.add(row);
        }
        *home.site_rows.borrow_mut() = rows;
        home.check_button.set_visible(!checks.is_empty());

        // summaries
        home.method_row.set_subtitle(&match self.current_method() {
            Some(method) => {
                let name = self.method_name(&method);
                match self.methods.borrow().standing(&method, &self.sites()) {
                    Standing::Works => {
                        gettext("{name} · Opens your sites").replace("{name}", &name)
                    }
                    Standing::Partly { .. } => {
                        gettext("{name} · Some sites stay blocked").replace("{name}", &name)
                    }
                    Standing::NotTested => name,
                }
            }
            None if status.is_some() => gettext("Custom settings"),
            None => String::new(),
        });
        let excluded = self.excluded_sites().len();
        home.sites_row
            .set_subtitle(&match (self.only_my_sites(), excluded) {
                (true, _) => gettext("Only your sites"),
                (false, 0) => gettext("All sites"),
                (false, n) => ngettext("All sites, except {n}", "All sites, except {n}", n as u32)
                    .replace("{n}", &n.to_string()),
            });
        home.rendering.set(false);
    }
}
