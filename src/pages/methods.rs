//! Methods: every way Sluis knows, best first, one in use
//! (design/screens/03-strategies.png). Choosing one switches to it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::glib;
use sluis_core::describe;

use crate::config::RESOURCE_BASE;
use crate::methods::Standing;
use crate::ui::Ui;

pub struct MethodsPage {
    pub page: adw::NavigationPage,
    preferences: adw::PreferencesPage,
    search_bar: gtk::SearchBar,
    search: gtk::SearchEntry,
    groups: RefCell<Vec<adw::PreferencesGroup>>,
    /// Set while radios are set from code, not by the user.
    rendering: Cell<bool>,
}

impl MethodsPage {
    pub fn new() -> Self {
        let b = gtk::Builder::from_resource(&format!("{RESOURCE_BASE}/ui/methods-page.ui"));
        Self {
            page: b.object("page").unwrap(),
            preferences: b.object("preferences").unwrap(),
            search_bar: b.object("search_bar").unwrap(),
            search: b.object("search").unwrap(),
            groups: RefCell::default(),
            rendering: Cell::new(false),
        }
    }
}

impl Ui {
    /// Search: the button, Ctrl+F, or just typing on the page.
    pub fn connect_methods(self: &Rc<Self>) {
        let page = &self.methods_page;
        page.search_bar.connect_entry(&page.search);
        page.search_bar.set_key_capture_widget(Some(&page.page));
        let controller = gtk::ShortcutController::new();
        let bar = page.search_bar.clone();
        controller.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string("<Control>f"),
            Some(gtk::CallbackAction::new(move |_, _| {
                bar.set_search_mode(true);
                glib::Propagation::Stop
            })),
        ));
        page.page.add_controller(controller);
        page.search.connect_search_changed(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| ui.render_methods()
        ));
    }

    pub fn render_methods(self: &Rc<Self>) {
        let page = &self.methods_page;
        page.rendering.set(true);
        for group in page.groups.borrow_mut().drain(..) {
            page.preferences.remove(&group);
        }
        let sites = self.sites();
        let current = self.current_method();
        let technical = self.settings.boolean("show-technical-details");
        let store = self.methods.borrow();
        let ranked = store.ranked(&sites, current.as_deref());
        let fastest = store.fastest(&sites);

        let works = adw::PreferencesGroup::builder()
            .title(gettext("Open Your Sites"))
            .build();
        let partly = adw::PreferencesGroup::builder()
            .title(gettext("Leave Some Sites Blocked"))
            .build();
        let untried_group = adw::PreferencesGroup::new();
        let untried = adw::ExpanderRow::builder()
            .title(gettext("Not Tried Yet"))
            .build();
        untried_group.add(&untried);
        let query = page.search.text().to_lowercase();
        let mut counts = (0u32, 0u32, 0u32);
        let mut radio_group: Option<gtk::CheckButton> = None;

        for (i, method) in ranked.iter().enumerate() {
            let in_use = current.as_deref() == Some(method.args.as_str());
            let standing = store.standing(&method.args, &sites);
            // Translators: {} is a number
            let title = gettext("Method {}").replace("{}", &(i + 1).to_string());
            let name = describe(&method.strategy()).to_string();
            if !query.is_empty()
                && ![&title, &name, &method.args]
                    .iter()
                    .any(|t| t.to_lowercase().contains(&query))
            {
                continue;
            }
            let subtitle = match standing {
                Standing::Works => {
                    let opened = method
                        .result
                        .as_ref()
                        .map_or(0, |r| sites.iter().filter(|s| r.opened.contains(s)).count());
                    match opened {
                        1 if sites.len() == 1 => gettext("Opens your site"),
                        n if n == sites.len() => {
                            gettext("Opens all {n} of your sites").replace("{n}", &n.to_string())
                        }
                        // tried with only some of them (a scan of the blocked ones)
                        n => ngettext(
                            "Opened the {n} site it was tried with",
                            "Opened all {n} sites it was tried with",
                            n as u32,
                        )
                        .replace("{n}", &n.to_string()),
                    }
                }
                Standing::Partly { opened, total } => {
                    gettext("Opens {opened} of {total} of your sites")
                        .replace("{opened}", &opened.to_string())
                        .replace("{total}", &total.to_string())
                }
                Standing::NotTested if sites.is_empty() => {
                    gettext("Add your sites to see which methods open them")
                }
                Standing::NotTested => gettext("Not tried with your sites yet"),
            };
            // what tells methods apart: how fast they open sites, and what
            // they do (the arguments themselves only for experts)
            let ms = method.result.as_ref().and_then(|r| r.ms);
            let mut subtitle = match (standing, ms) {
                (Standing::NotTested, _) | (_, None) => subtitle,
                (_, Some(ms)) => format!("{subtitle} · {}", seconds(ms)),
            };
            subtitle.push('\n');
            subtitle.push_str(&name);
            if technical {
                subtitle.push('\n');
                subtitle.push_str(&method.args);
            }
            let row = adw::ActionRow::builder()
                .title(&title)
                .subtitle(subtitle)
                .use_markup(false)
                .build();
            let radio = gtk::CheckButton::builder()
                .valign(gtk::Align::Center)
                .active(in_use)
                .build();
            radio.set_group(radio_group.as_ref());
            radio_group.get_or_insert_with(|| radio.clone());
            row.add_prefix(&radio);
            row.set_activatable_widget(Some(&radio));
            if fastest == Some(method.args.as_str()) {
                let tag = gtk::Label::builder()
                    .label(gettext("Fastest"))
                    .valign(gtk::Align::Center)
                    .build();
                tag.add_css_class("tag");
                tag.add_css_class("fastest");
                row.add_suffix(&tag);
            }
            if in_use {
                let tag = gtk::Label::builder()
                    .label(gettext("In Use"))
                    .valign(gtk::Align::Center)
                    .build();
                tag.add_css_class("tag");
                row.add_suffix(&tag);
            }
            let details = gtk::Button::builder()
                .icon_name("go-next-symbolic")
                .valign(gtk::Align::Center)
                .tooltip_text(gettext("Details"))
                .build();
            details.add_css_class("flat");
            row.add_suffix(&details);

            let args = method.args.clone();
            radio.connect_toggled(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                #[strong]
                args,
                move |radio| {
                    if radio.is_active() && !ui.methods_page.rendering.get() && !ui.busy.get() {
                        ui.switch_to(args.clone());
                    }
                }
            ));
            details.connect_clicked(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                move |_| ui.show_method(&args)
            ));

            match standing {
                Standing::Works => {
                    works.add(&row);
                    counts.0 += 1;
                }
                Standing::Partly { .. } => {
                    partly.add(&row);
                    counts.1 += 1;
                }
                Standing::NotTested => {
                    untried.add_row(&row);
                    counts.2 += 1;
                    if in_use || !query.is_empty() {
                        untried.set_expanded(true);
                    }
                }
            }
            row.set_sensitive(!self.busy.get());
        }
        untried.set_subtitle(
            &ngettext("{n} method", "{n} methods", counts.2).replace("{n}", &counts.2.to_string()),
        );

        let mut groups = Vec::new();
        for (group, count) in [
            (works, counts.0),
            (partly, counts.1),
            (untried_group, counts.2),
        ] {
            if count > 0 {
                page.preferences.add(&group);
                groups.push(group);
            }
        }
        if groups.is_empty() && !query.is_empty() {
            let none = adw::PreferencesGroup::new();
            none.add(
                &adw::ActionRow::builder()
                    .title(
                        gettext("No methods match “{query}”")
                            .replace("{query}", &page.search.text()),
                    )
                    .use_markup(false)
                    .build(),
            );
            page.preferences.add(&none);
            groups.push(none);
        }
        *page.groups.borrow_mut() = groups;
        page.rendering.set(false);
    }
}

/// "0.4 s": how long a site took to open with a method.
fn seconds(ms: u32) -> String {
    let tenths = (ms + 50) / 100;
    // Translators: how long a site takes to open, like "0.4 s"; {whole} and
    // {tenth} are the seconds before and after the decimal separator
    gettext("{whole}.{tenth} s")
        .replace("{whole}", &(tenths / 10).to_string())
        .replace("{tenth}", &(tenths % 10).to_string())
}
