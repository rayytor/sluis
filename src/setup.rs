//! Getting zapret onto the computer and keeping it current: Get Started on
//! the welcome page, the update banner, and Preferences' version and
//! Uninstall rows. The helper does the work (crates/helper/src/install.rs).

use std::rc::Rc;

use adw::prelude::*;
use futures_util::StreamExt;
use gettextrs::gettext;
use gtk::glib;
use sluis_helper::install::newer;

use crate::config::{APP_ID, RESOURCE_BASE};
use crate::helper::Helper;
use crate::ui::Ui;

/// Switch rows in preferences-dialog.ui, each named after its GSettings key.
/// Every switch applies instantly; nothing here needs a Save button.
const SWITCHES: &[&str] = &[
    "start-at-login",
    "remember-networks",
    "notify-when-blocked",
    "update-methods",
    "unblock-calls",
    "ipv6",
    "show-technical-details",
];

fn step_text(step: &str) -> String {
    match step {
        "downloading" => gettext("Downloading Zapret…"),
        "installing" => gettext("Installing…"),
        "starting" => gettext("Starting Zapret…"),
        _ => gettext("Getting ready…"),
    }
}

/// Shows each step the helper reports in `label`, until aborted.
fn follow_steps(helper: &Helper, label: &gtk::Label) -> glib::JoinHandle<()> {
    let (helper, label) = (helper.clone(), label.clone());
    glib::spawn_future_local(async move {
        let Ok(mut steps) = helper.setup_steps().await else {
            return;
        };
        while let Some(step) = steps.next().await {
            label.set_label(&step_text(&step));
        }
    })
}

impl Ui {
    /// Get Started: installs zapret, then asks which sites don't open.
    pub fn get_started(self: &Rc<Self>) {
        let Some(helper) = self.helper() else {
            self.toast(&gettext(
                "Can’t reach Sluis’s helper. Reinstalling Sluis usually fixes this.",
            ));
            return;
        };
        self.welcome_start.set_visible(false);
        self.welcome_progress.set_visible(true);
        self.welcome_label.set_label(&step_text(""));
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let steps = follow_steps(&helper, &ui.welcome_label);
            let result = helper.install().await;
            steps.abort();
            ui.welcome_start.set_visible(true);
            ui.welcome_progress.set_visible(false);
            match result {
                Ok(_) => {
                    ui.refresh_status().await;
                    ui.show_scan();
                }
                Err(message) => ui.toast(&message),
            }
        });
    }

    /// Looks for a newer zapret release when this app installed zapret.
    pub fn check_for_update(self: &Rc<Self>) {
        let (Some(helper), Some(status)) = (self.helper(), self.status()) else {
            return;
        };
        if status.install_kind != "app" {
            return;
        }
        let ui = self.clone();
        glib::spawn_future_local(async move {
            if let Some(latest) = helper.latest_version().await
                && newer(&latest, &status.version)
            {
                *ui.update.borrow_mut() = Some(latest);
                ui.render_home();
            }
        });
    }

    pub fn update_available(&self) -> Option<String> {
        self.update.borrow().clone()
    }

    pub fn update_zapret(self: &Rc<Self>) {
        let Some(helper) = self.helper() else {
            return;
        };
        let ui = self.clone();
        glib::spawn_future_local(async move {
            ui.busy.set(true);
            ui.render();
            ui.toast(&gettext("Updating Zapret…"));
            let result = helper.update().await;
            ui.busy.set(false);
            match result {
                Ok(version) => {
                    *ui.update.borrow_mut() = None;
                    ui.toast(
                        &gettext("Zapret is updated to {version}").replace("{version}", &version),
                    );
                }
                Err(message) => ui.toast(&message),
            }
            ui.refresh_status().await;
            ui.check_sites();
        });
    }

    /// Asks first, listing exactly what goes away.
    fn confirm_uninstall(self: &Rc<Self>, parent: &adw::PreferencesDialog) {
        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Uninstall Zapret?"))
            .body(gettext(
                "Blocked sites will stop opening. This removes Zapret’s program and settings in /opt/zapret and its system service. Your sites and the methods Sluis found stay in this app.",
            ))
            .default_response("cancel")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[
            ("cancel", &gettext("_Cancel")),
            ("uninstall", &gettext("_Uninstall")),
        ]);
        dialog.set_response_appearance("uninstall", adw::ResponseAppearance::Destructive);
        let ui = self.clone();
        let preferences = parent.clone();
        dialog.connect_response(Some("uninstall"), move |_, _| {
            preferences.close();
            ui.uninstall_zapret();
        });
        dialog.present(Some(parent));
    }

    fn uninstall_zapret(self: &Rc<Self>) {
        let Some(helper) = self.helper() else {
            return;
        };
        let ui = self.clone();
        glib::spawn_future_local(async move {
            match helper.uninstall().await {
                Ok(()) => ui.toast(&gettext("Zapret is uninstalled")),
                Err(message) => ui.toast(&message),
            }
            ui.refresh_status().await;
        });
    }

    pub fn present_preferences(self: &Rc<Self>) {
        let builder =
            gtk::Builder::from_resource(&format!("{RESOURCE_BASE}/ui/preferences-dialog.ui"));
        let dialog: adw::PreferencesDialog = builder
            .object("dialog")
            .expect("dialog in preferences-dialog.ui");
        let settings = gtk::gio::Settings::new(APP_ID);
        for key in SWITCHES {
            let row: adw::SwitchRow = builder
                .object(*key)
                .unwrap_or_else(|| panic!("no switch row \"{key}\" in preferences-dialog.ui"));
            settings.bind(key, &row, "active").build();
        }

        let version: adw::ActionRow = builder.object("zapret_version").unwrap();
        let update: gtk::Button = builder.object("update_button").unwrap();
        let remove: adw::PreferencesGroup = builder.object("remove_group").unwrap();
        let uninstall: adw::ButtonRow = builder.object("uninstall").unwrap();
        let status = self.status().unwrap_or_default();
        version.set_subtitle(&match status.install_kind.as_str() {
            "app" => match self.update_available() {
                Some(latest) => gettext("{version} · Version {latest} is available")
                    .replace("{version}", &status.version)
                    .replace("{latest}", &latest),
                None => gettext("{version} · Up to date").replace("{version}", &status.version),
            },
            "system" => gettext("Comes with your system, which keeps it up to date"),
            "manual" => gettext("Installed by hand; update it the way it was installed"),
            _ => gettext("Not installed"),
        });
        update.set_visible(self.update_available().is_some());
        remove.set_visible(status.install_kind == "app");
        update.connect_clicked(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            #[weak]
            dialog,
            move |_| {
                dialog.close();
                ui.update_zapret();
            }
        ));
        uninstall.connect_activated(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            #[weak]
            dialog,
            move |_| ui.confirm_uninstall(&dialog)
        ));
        dialog.present(Some(&self.window));
    }
}
