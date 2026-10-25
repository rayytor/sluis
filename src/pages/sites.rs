//! Which sites: all or only the user's, the user's own list, and the sites
//! Sluis must leave alone (design/screens/08-sites.png). Changes apply at
//! once; no Save button.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::glib;
use sluis_core::{services, site};

use crate::config::RESOURCE_BASE;
use crate::ui::Ui;

pub struct SitesPage {
    pub page: adw::NavigationPage,
    mode: adw::ComboRow,
    mine: adw::PreferencesGroup,
    excluded: adw::PreferencesGroup,
    add_mine: adw::EntryRow,
    add_excluded: adw::EntryRow,
    rows: RefCell<Vec<(adw::PreferencesGroup, adw::ActionRow)>>,
    rendering: Cell<bool>,
}

impl SitesPage {
    pub fn new() -> Self {
        let b = gtk::Builder::from_resource(&format!("{RESOURCE_BASE}/ui/sites-page.ui"));
        Self {
            page: b.object("page").unwrap(),
            mode: b.object("mode").unwrap(),
            mine: b.object("mine").unwrap(),
            excluded: b.object("excluded").unwrap(),
            add_mine: b.object("add_mine").unwrap(),
            add_excluded: b.object("add_excluded").unwrap(),
            rows: RefCell::default(),
            rendering: Cell::new(false),
        }
    }
}

/// Which list a site goes to.
#[derive(Clone, Copy)]
enum List {
    Mine,
    Excluded,
}

impl List {
    fn key(self) -> &'static str {
        match self {
            Self::Mine => "sites",
            Self::Excluded => "excluded-sites",
        }
    }
}

impl Ui {
    pub fn connect_sites(self: &Rc<Self>) {
        let page = &self.sites_page;
        page.mode.connect_selected_notify(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |row| {
                if ui.sites_page.rendering.get() {
                    return;
                }
                let mode = if row.selected() == 1 { "mine" } else { "all" };
                let _ = ui.settings.set_string("sites-mode", mode);
                ui.render();
                ui.reapply_sites();
            }
        ));
        for (entry, list) in [
            (&page.add_mine, List::Mine),
            (&page.add_excluded, List::Excluded),
        ] {
            entry.connect_apply(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                move |entry| ui.add_site(entry, list)
            ));
            entry.connect_entry_activated(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                move |entry| ui.add_site(entry, list)
            ));
            entry.connect_changed(|entry| entry.remove_css_class("error"));
        }
    }

    fn list(&self, list: List) -> Vec<String> {
        self.settings
            .strv(list.key())
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn set_list(self: &Rc<Self>, list: List, sites: &[String]) {
        let sites: Vec<&str> = sites.iter().map(String::as_str).collect();
        let _ = self.settings.set_strv(list.key(), sites.as_slice());
        self.render();
        // with "all sites", the user's list only decides what is checked
        if matches!(list, List::Excluded) || self.only_my_sites() {
            self.reapply_sites();
        }
        if matches!(list, List::Mine) {
            self.check_sites();
        }
    }

    fn add_site(self: &Rc<Self>, entry: &adw::EntryRow, list: List) {
        let text = entry.text();
        if text.trim().is_empty() {
            return;
        }
        let domain = match site::normalize(&text) {
            Ok(d) => d,
            Err(_) => {
                entry.add_css_class("error");
                self.toast(&gettext("“{text}” isn’t a site name").replace("{text}", text.trim()));
                return;
            }
        };
        let mut sites = self.list(list);
        if !sites.contains(&domain) {
            sites.push(domain);
            self.set_list(list, &sites);
        }
        entry.set_text("");
    }

    fn remove_site(self: &Rc<Self>, list: List, site: &str) {
        let sites: Vec<String> = self.list(list).into_iter().filter(|s| s != site).collect();
        self.set_list(list, &sites);
        let removed = site.to_owned();
        let toast = adw::Toast::builder()
            .title(gettext("Removed {site}").replace("{site}", &Self::site_name(site)))
            .button_label(gettext("_Undo"))
            .use_markup(false)
            .build();
        toast.connect_button_clicked(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| {
                let mut sites = ui.list(list);
                sites.push(removed.clone());
                ui.set_list(list, &sites);
            }
        ));
        self.window_toast(toast);
    }

    pub fn render_sites(self: &Rc<Self>) {
        let page = &self.sites_page;
        page.rendering.set(true);
        page.mode.set_selected(u32::from(self.only_my_sites()));
        for (group, row) in page.rows.borrow_mut().drain(..) {
            group.remove(&row);
        }
        let mut rows = Vec::new();
        for (list, group) in [(List::Mine, &page.mine), (List::Excluded, &page.excluded)] {
            for site in self.list(list) {
                let row = adw::ActionRow::builder()
                    .title(Self::site_name(&site))
                    .build();
                if let Some(service) = services::find(&site) {
                    let others = service.domains.len() - 1;
                    row.set_subtitle(&if others == 0 {
                        site.clone()
                    } else {
                        ngettext(
                            "{site} and {n} other {service} address",
                            "{site} and {n} other {service} addresses",
                            others as u32,
                        )
                        .replace("{site}", &site)
                        .replace("{n}", &others.to_string())
                        .replace("{service}", service.name)
                    });
                }
                let remove = gtk::Button::builder()
                    .icon_name("user-trash-symbolic")
                    .valign(gtk::Align::Center)
                    .tooltip_text(
                        gettext("Remove {site}").replace("{site}", &Self::site_name(&site)),
                    )
                    .build();
                remove.add_css_class("flat");
                remove.connect_clicked(glib::clone!(
                    #[weak(rename_to = ui)]
                    self,
                    #[strong]
                    site,
                    move |_| ui.remove_site(list, &site)
                ));
                row.add_suffix(&remove);
                group.add(&row);
                rows.push((group.clone(), row));
            }
        }
        *page.rows.borrow_mut() = rows;
        page.rendering.set(false);
    }
}
