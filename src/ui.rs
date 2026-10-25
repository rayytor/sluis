//! The window's state and the flows that change it: talking to the helper,
//! checking sites, switching methods with automatic switch-back and Undo.
//! Each page renders from this state (src/pages/).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use sluis_core::{Strategy, services};

use crate::checks::{self, Check};
use crate::config::{APP_ID, RESOURCE_BASE};
use crate::helper::{Helper, Status};
use crate::methods::Store;
use crate::network::{self, Network};
use crate::pages::{home::HomePage, methods::MethodsPage, scan::ScanPage, sites::SitesPage};

/// "A, B and C"
pub fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => gettext("{list} and {last}")
            .replace("{list}", &rest.join(", "))
            .replace("{last}", last),
    }
}

/// How long the switch-back question waits before switching back.
const SWITCH_BACK_SECONDS: u32 = 12;

pub struct Ui {
    pub window: adw::ApplicationWindow,
    pub nav: adw::NavigationView,
    toasts: adw::ToastOverlay,
    pub settings: gio::Settings,
    helper: RefCell<Option<Helper>>,
    /// `None` until the helper answered.
    status: RefCell<Option<Status>>,
    helper_failed: Cell<bool>,
    /// The latest check of each site.
    checks: RefCell<BTreeMap<String, Check>>,
    /// Bumped by every new round of checks, so late answers of an older
    /// round are ignored.
    check_round: Cell<u64>,
    pub methods: RefCell<Store>,
    /// A change is being applied; controls that change things wait.
    pub busy: Cell<bool>,
    /// What the last switch replaced, for its Undo.
    undo: RefCell<Option<String>>,
    pub home: HomePage,
    pub methods_page: MethodsPage,
    pub sites_page: SitesPage,
    pub scan: ScanPage,
    welcome: adw::NavigationPage,
    pub welcome_start: gtk::Button,
    pub welcome_progress: gtk::Box,
    pub welcome_label: gtk::Label,
    /// A newer zapret release, when this app installed zapret.
    pub update: RefCell<Option<String>>,
    /// The network the computer is on.
    network: RefCell<Option<Network>>,
    /// Bumped by every network change, so only the last one is acted on.
    network_round: Cell<u64>,
}

impl std::fmt::Debug for Ui {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ui")
    }
}

impl Ui {
    pub fn new(
        window: &adw::ApplicationWindow,
        nav: adw::NavigationView,
        toasts: adw::ToastOverlay,
    ) -> Rc<Self> {
        let builder = gtk::Builder::from_resource(&format!("{RESOURCE_BASE}/ui/welcome-page.ui"));
        let welcome: adw::NavigationPage = builder.object("page").unwrap();
        // the window is not attached to the application yet here
        let menu = gio::Application::default()
            .and_then(|app| app.downcast::<gtk::Application>().ok())
            .and_then(|app| app.menu_by_id("primary-menu"))
            .map(|m| m.upcast::<gio::MenuModel>());
        builder
            .object::<gtk::MenuButton>("menu_button")
            .unwrap()
            .set_menu_model(menu.as_ref());

        let ui = Rc::new(Self {
            window: window.clone(),
            nav,
            toasts,
            settings: gio::Settings::new(APP_ID),
            helper: RefCell::default(),
            status: RefCell::default(),
            helper_failed: Cell::new(false),
            checks: RefCell::default(),
            check_round: Cell::new(0),
            methods: RefCell::new(Store::load()),
            busy: Cell::new(false),
            undo: RefCell::default(),
            home: HomePage::new(menu.as_ref()),
            methods_page: MethodsPage::new(),
            sites_page: SitesPage::new(),
            scan: ScanPage::new(),
            welcome,
            welcome_start: builder.object("start_button").unwrap(),
            welcome_progress: builder.object("progress").unwrap(),
            welcome_label: builder.object("progress_label").unwrap(),
            update: RefCell::default(),
            network: RefCell::default(),
            network_round: Cell::new(0),
        });
        ui.connect_home();
        ui.connect_sites();
        ui.connect_methods();
        ui.connect_scan();
        ui.nav.add(&ui.home.page);
        ui
    }

    /// Connects to the helper, shows the right page and checks the sites.
    pub fn start(self: &Rc<Self>) {
        let ui = self.clone();
        glib::spawn_future_local(async move {
            match Helper::connect().await {
                Ok(helper) => *ui.helper.borrow_mut() = Some(helper),
                Err(e) => {
                    eprintln!("cannot reach the system bus: {e}");
                    ui.helper_failed.set(true);
                }
            }
            ui.refresh_status().await;
            *ui.network.borrow_mut() = network::current().await;
            ui.render_home();
            ui.check_sites();
            ui.check_for_update();
        });
        self.follow_network();
    }

    pub fn network(&self) -> Option<Network> {
        self.network.borrow().clone()
    }

    /// Acts on network changes once they settle (connecting fires several
    /// changes in a row).
    fn follow_network(self: &Rc<Self>) {
        gio::NetworkMonitor::default().connect_network_changed(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_, _| {
                let round = ui.network_round.get() + 1;
                ui.network_round.set(round);
                glib::timeout_add_seconds_local_once(
                    3,
                    glib::clone!(
                        #[weak]
                        ui,
                        move || {
                            if ui.network_round.get() == round {
                                glib::spawn_future_local(async move { ui.network_changed().await });
                            }
                        }
                    ),
                );
            }
        ));
    }

    /// On another network: use the method that worked there before (with
    /// Undo), or check whether the sites still open.
    async fn network_changed(self: &Rc<Self>) {
        let now = network::current().await;
        if now.as_ref().map(|n| &n.id) == self.network.borrow().as_ref().map(|n| &n.id) {
            return;
        }
        *self.network.borrow_mut() = now.clone();
        self.render_home();
        let Some(now) = now else {
            return;
        };
        let saved = self.methods.borrow().saved(&now.id).cloned();
        let running = self.status().is_some_and(|s| s.running);
        if let (true, true, Some(saved)) =
            (self.settings.boolean("remember-networks"), running, saved)
            && self.current_method().as_deref() != Some(saved.method.as_str())
            && let Some(strategy) = Strategy::parse(&saved.method)
                .ok()
                .and_then(|m| self.compose(&m))
        {
            let previous = self.status().map(|s| s.strategy).unwrap_or_default();
            if self.apply(&strategy.to_string()).await {
                let title = gettext("On {network}, Sluis uses {method}")
                    .replace("{network}", &now.name)
                    .replace("{method}", &self.method_name(&saved.method));
                self.offer_undo(&title, previous);
            }
        }
        self.check_sites();
    }

    /// Remembers the method in use for this network once every site opens.
    fn remember_network(&self, method: &str) {
        if let Some(network) = self.network.borrow().as_ref() {
            self.methods
                .borrow_mut()
                .remember(&network.id, &network.name, method);
        }
    }

    pub fn helper(&self) -> Option<Helper> {
        self.helper.borrow().clone()
    }

    pub fn status(&self) -> Option<Status> {
        self.status.borrow().clone()
    }

    pub fn helper_failed(&self) -> bool {
        self.helper_failed.get()
    }

    pub fn sites(&self) -> Vec<String> {
        self.settings
            .strv("sites")
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    pub fn excluded_sites(&self) -> Vec<String> {
        self.settings
            .strv("excluded-sites")
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    pub fn only_my_sites(&self) -> bool {
        self.settings.string("sites-mode") == "mine"
    }

    pub fn checks(&self) -> BTreeMap<String, Check> {
        self.checks.borrow().clone()
    }

    /// The method in use, without the app's site filters, as stored.
    /// `None` when there is none, or zapret's config has something the app
    /// does not accept.
    pub fn current_method(&self) -> Option<String> {
        let status = self.status.borrow();
        Strategy::parse(&status.as_ref()?.strategy)
            .ok()?
            .method()
            .map(|m| m.to_string())
    }

    /// What people call a site: the service's name, or the domain.
    pub fn site_name(site: &str) -> String {
        services::find(site).map_or_else(|| site.to_owned(), |s| s.name.to_owned())
    }

    pub fn toast(&self, title: &str) {
        self.toasts.add_toast(adw::Toast::new(title));
    }

    pub fn window_toast(&self, toast: adw::Toast) {
        self.toasts.add_toast(toast);
    }

    /// Re-reads the helper's status and redraws everything.
    pub async fn refresh_status(self: &Rc<Self>) {
        let helper = self.helper.borrow().clone();
        let status = match helper {
            Some(h) => match h.status().await {
                Ok(s) => Some(s),
                Err(e) => {
                    eprintln!("sluis-helper: {e}");
                    self.helper_failed.set(true);
                    None
                }
            },
            None => None,
        };
        if let Some(status) = &status {
            self.helper_failed.set(false);
            if let Some(method) = Strategy::parse(&status.strategy)
                .ok()
                .and_then(|s| s.method())
            {
                let mut store = self.methods.borrow_mut();
                store.add(&method);
                store.save();
            }
        }
        *self.status.borrow_mut() = status;
        self.show_main_page();
        self.render();
    }

    fn show_main_page(&self) {
        let installed = self.status.borrow().as_ref().is_none_or(|s| s.installed);
        let showing_welcome = self.nav.visible_page().as_ref() == Some(&self.welcome);
        if installed && showing_welcome {
            self.nav.replace(std::slice::from_ref(&self.home.page));
        } else if !installed && !showing_welcome {
            self.nav.replace(std::slice::from_ref(&self.welcome));
        }
    }

    pub fn render(self: &Rc<Self>) {
        self.render_home();
        self.render_methods();
        self.render_sites();
    }

    /// Checks every site again; each row updates as its answer comes in,
    /// and the result is remembered for the method in use.
    pub fn check_sites(self: &Rc<Self>) {
        let sites = self.sites();
        let round = self.check_round.get() + 1;
        self.check_round.set(round);
        *self.checks.borrow_mut() = sites.iter().map(|s| (s.clone(), Check::Pending)).collect();
        self.render_home();
        for site in sites {
            let ui = self.clone();
            glib::spawn_future_local(async move {
                let result = checks::check(&site).await;
                if ui.check_round.get() != round {
                    return;
                }
                ui.checks.borrow_mut().insert(site, result);
                if ui.checks.borrow().values().all(|c| *c != Check::Pending) {
                    ui.record_checks();
                }
                ui.render_home();
                ui.render_methods();
            });
        }
    }

    /// Waits for the current round of checks to finish.
    async fn checked(self: &Rc<Self>) -> BTreeMap<String, Check> {
        self.check_sites();
        while self.checks.borrow().values().any(|c| *c == Check::Pending) {
            glib::timeout_future(std::time::Duration::from_millis(200)).await;
        }
        self.checks()
    }

    fn record_checks(&self) {
        let (Some(method), true) = (
            self.current_method(),
            self.status().is_some_and(|s| s.running),
        ) else {
            return;
        };
        let checks = self.checks.borrow();
        // a site the provider's DNS sends elsewhere never reached the
        // method: it counts neither way
        let (opened, blocked): (Vec<_>, Vec<_>) = checks
            .iter()
            .filter(|(_, c)| **c != Check::WrongAddress)
            .partition(|(_, c)| **c == Check::Opens);
        let names = |v: Vec<(&String, &Check)>| v.into_iter().map(|(s, _)| s.clone()).collect();
        let all_open = blocked.is_empty() && !opened.is_empty();
        self.methods
            .borrow_mut()
            .record(&method, names(opened), names(blocked), None);
        if all_open {
            self.remember_network(&method);
        }
        self.methods.borrow().save();
    }

    /// The full strategy for `method` with the user's site settings.
    pub fn compose(&self, method: &Strategy) -> Option<Strategy> {
        let only = self
            .only_my_sites()
            .then(|| services::expand(&self.sites()));
        method
            .for_sites(only.as_deref(), &services::expand(&self.excluded_sites()))
            .ok()
    }

    /// Applies `strategy` through the helper; shows the error as a toast.
    pub async fn apply(self: &Rc<Self>, strategy: &str) -> bool {
        let Some(helper) = self.helper.borrow().clone() else {
            return false;
        };
        self.busy.set(true);
        self.render();
        let result = helper.apply(strategy).await;
        self.busy.set(false);
        if let Err(message) = &result {
            self.toast(message);
        }
        self.refresh_status().await;
        result.is_ok()
    }

    /// Re-applies the method in use with the current site settings, after
    /// they changed.
    pub fn reapply_sites(self: &Rc<Self>) {
        let Some(method) = self.current_method().and_then(|m| Strategy::parse(&m).ok()) else {
            return;
        };
        let Some(strategy) = self.compose(&method) else {
            return;
        };
        if self
            .status()
            .is_some_and(|s| s.strategy == strategy.to_string())
        {
            return;
        }
        let ui = self.clone();
        glib::spawn_future_local(async move {
            if ui.apply(&strategy.to_string()).await {
                ui.check_sites();
            }
        });
    }

    pub fn set_enabled(self: &Rc<Self>, on: bool) {
        let Some(helper) = self.helper.borrow().clone() else {
            return;
        };
        let ui = self.clone();
        glib::spawn_future_local(async move {
            ui.busy.set(true);
            ui.render();
            let result = helper.set_enabled(on).await;
            ui.busy.set(false);
            if let Err(message) = result {
                ui.toast(&message);
            }
            ui.refresh_status().await;
            if on {
                ui.check_sites();
            }
        });
    }

    pub fn repair(self: &Rc<Self>) {
        let Some(helper) = self.helper.borrow().clone() else {
            return;
        };
        let ui = self.clone();
        glib::spawn_future_local(async move {
            match helper.repair().await {
                Ok(_) => ui.toast(&gettext("Zapret’s settings file is repaired")),
                Err(message) => ui.toast(&message),
            }
            ui.refresh_status().await;
        });
    }

    /// "Method N"
    pub fn method_name(&self, args: &str) -> String {
        self.method_name_in_use(args, self.current_method().as_deref())
    }

    /// "Method N" as it will be numbered while `in_use` is in use.
    fn method_name_in_use(&self, args: &str, in_use: Option<&str>) -> String {
        match self.methods.borrow().number(args, &self.sites(), in_use) {
            // Translators: {} is a number
            Some(n) => gettext("Method {}").replace("{}", &n.to_string()),
            None => gettext("Custom method"),
        }
    }

    /// Switches to the method `args`, then checks the sites. If a site that
    /// opened before no longer does, asks whether to keep it and switches
    /// back by itself unless told to keep it; otherwise offers Undo.
    pub fn switch_to(self: &Rc<Self>, args: String) {
        let Some(previous) = self.status().map(|s| s.strategy) else {
            return;
        };
        let previous_method = self.current_method();
        let Some(strategy) = Strategy::parse(&args).ok().and_then(|m| self.compose(&m)) else {
            self.toast(&gettext("That method isn’t valid"));
            return;
        };
        let opened_before: Vec<String> = self
            .checks
            .borrow()
            .iter()
            .filter(|(_, c)| **c == Check::Opens)
            .map(|(s, _)| s.clone())
            .collect();
        let ui = self.clone();
        glib::spawn_future_local(async move {
            if !ui.apply(&strategy.to_string()).await {
                return;
            }
            let name = ui.method_name(&args);
            let checks = ui.checked().await;
            let lost: Vec<String> = opened_before
                .iter()
                .filter(|s| checks.get(*s).is_some_and(Check::is_blocked))
                .cloned()
                .collect();
            if lost.is_empty() || previous.is_empty() {
                let opened = checks.values().filter(|c| **c == Check::Opens).count();
                let title = if checks.is_empty() {
                    gettext("Switched to {name}").replace("{name}", &name)
                } else if opened == checks.len() {
                    gettext("Switched to {name} · All {n} sites open")
                        .replace("{name}", &name)
                        .replace("{n}", &checks.len().to_string())
                } else {
                    gettext("Switched to {name} · {opened} of {n} sites open")
                        .replace("{name}", &name)
                        .replace("{opened}", &opened.to_string())
                        .replace("{n}", &checks.len().to_string())
                };
                ui.offer_undo(&title, previous);
            } else {
                let back_to = previous_method.map_or_else(
                    || gettext("the previous method"),
                    |m| ui.method_name_in_use(&m, Some(&m)),
                );
                ui.ask_switch_back(&name, &back_to, &lost, previous);
            }
        });
    }

    /// Uses the method `args` a scan found: switches to it (with the usual
    /// switch-back and Undo) when zapret runs, or applies it and turns
    /// zapret on.
    pub fn turn_on_with(self: &Rc<Self>, args: String) {
        if self.status().is_some_and(|s| s.running) {
            self.switch_to(args);
            return;
        }
        let Some(strategy) = Strategy::parse(&args).ok().and_then(|m| self.compose(&m)) else {
            return;
        };
        let ui = self.clone();
        glib::spawn_future_local(async move {
            if ui.apply(&strategy.to_string()).await {
                ui.set_enabled(true);
            }
        });
    }

    fn offer_undo(self: &Rc<Self>, title: &str, previous: String) {
        *self.undo.borrow_mut() = Some(previous);
        let toast = adw::Toast::builder()
            .title(title)
            .button_label(gettext("_Undo"))
            .action_name("win.undo-switch")
            .use_markup(false)
            .build();
        self.toasts.add_toast(toast);
    }

    /// Goes back to what the last switch replaced.
    pub fn undo_switch(self: &Rc<Self>) {
        let Some(previous) = self.undo.borrow_mut().take() else {
            return;
        };
        let ui = self.clone();
        glib::spawn_future_local(async move {
            if ui.apply(&previous).await {
                ui.toast(&gettext("Switched back"));
                ui.check_sites();
            }
        });
    }

    /// "Keep Method 3?" with a countdown that switches back at zero, like
    /// GNOME Settings does after a display change.
    fn ask_switch_back(
        self: &Rc<Self>,
        name: &str,
        back_to: &str,
        lost: &[String],
        previous: String,
    ) {
        let lost_names: Vec<String> = lost.iter().map(|s| Self::site_name(s)).collect();
        let back_to = back_to.to_owned();
        let body = move |seconds: u32| {
            gettext("{sites} didn’t open with it, so Sluis will switch back to {method} in {n} seconds.")
                .replace("{sites}", &list(&lost_names))
                .replace("{method}", &back_to)
                .replace("{n}", &seconds.to_string())
        };
        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Keep {name}?").replace("{name}", name))
            .body(body(SWITCH_BACK_SECONDS))
            .default_response("revert")
            .close_response("revert")
            .build();
        dialog.add_responses(&[
            ("keep", &gettext("_Keep Anyway")),
            ("revert", &gettext("_Switch Back")),
        ]);
        dialog.set_response_appearance("revert", adw::ResponseAppearance::Suggested);

        let remaining = Rc::new(Cell::new(SWITCH_BACK_SECONDS));
        let countdown = glib::timeout_add_seconds_local(
            1,
            glib::clone!(
                #[weak]
                dialog,
                #[strong]
                remaining,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    remaining.set(remaining.get().saturating_sub(1));
                    if remaining.get() == 0 {
                        dialog.close();
                        return glib::ControlFlow::Break;
                    }
                    dialog.set_body(&body(remaining.get()));
                    glib::ControlFlow::Continue
                }
            ),
        );
        let countdown = RefCell::new(Some(countdown));
        let ui = self.clone();
        dialog.connect_response(None, move |_, response| {
            if remaining.get() > 0
                && let Some(source) = countdown.borrow_mut().take()
            {
                source.remove();
            }
            if response == "revert" {
                let ui = ui.clone();
                let previous = previous.clone();
                glib::spawn_future_local(async move {
                    if ui.apply(&previous).await {
                        ui.toast(&gettext("Switched back"));
                        ui.check_sites();
                    }
                });
            }
        });
        dialog.present(Some(&self.window));
    }
}
