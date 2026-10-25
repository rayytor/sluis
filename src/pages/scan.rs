//! Unblock Sites: which sites don't open, looking for a way around them,
//! and the answer (design/screens/05–07). The helper does the looking; the
//! dialog can be closed meanwhile and opened again, and a notification
//! says when it's done.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use adw::prelude::*;
use futures_util::StreamExt;
use gettextrs::{gettext, ngettext};
use gtk::{gio, glib};
use sluis_core::{Strategy, describe, services, site};

use crate::config::RESOURCE_BASE;
use crate::helper::{ScanEvent, Tried};
use crate::methods::{SAME_SPEED, Standing, Store};
use crate::ui::{Ui, list};

pub struct ScanPage {
    dialog: adw::Dialog,
    toasts: adw::ToastOverlay,
    nav: adw::NavigationView,
    sites_page: adw::NavigationPage,
    progress_page: adw::NavigationPage,
    result_page: adw::NavigationPage,
    chosen: adw::PreferencesGroup,
    add_site: adw::EntryRow,
    suggested: adw::PreferencesGroup,
    cancel_button: gtk::Button,
    deep: gtk::CheckButton,
    progress_detail: gtk::Label,
    progress_bar: gtk::ProgressBar,
    progress_count: gtk::Label,
    found_group: adw::PreferencesGroup,
    tried_group: adw::PreferencesGroup,
    result_icon: gtk::Image,
    result_title: gtk::Label,
    result_description: gtk::Label,
    result_group: adw::PreferencesGroup,
    result_primary: gtk::Button,
    result_secondary: gtk::Button,
    rows: RefCell<Vec<(adw::PreferencesGroup, gtk::Widget)>>,
    scan: RefCell<Scan>,
    /// What the result page's two buttons do.
    next: Cell<(Next, Option<Next>)>,
    /// Bumped by every scan, so events of an older one are ignored.
    round: Cell<u64>,
}

/// What the dialog shows.
#[derive(Default)]
struct Scan {
    /// The sites typed or picked on the first page.
    chosen: Vec<String>,
    deep: bool,
    /// The sites being scanned.
    sites: Vec<String>,
    running: bool,
    started: Option<Instant>,
    done: u32,
    total: u32,
    diagnosis: Vec<(String, String)>,
    tried: Vec<Tried>,
    /// Methods tried in an earlier round of the same scan (Keep Looking).
    kept: Vec<Tried>,
    /// Methods already offered as the answer, in order; Keep Looking offers
    /// the next best one that isn't here.
    shown: Vec<String>,
    /// The method the result page offers now (what Turn It On turns on).
    showing: Option<String>,
    /// Whether the last round was stopped before it tried everything.
    stopped: bool,
    /// How long the last round took, once it's done.
    took: Option<u64>,
}

/// What to do from the result page.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Next {
    TurnOn,
    KeepLooking,
    Deep,
    Close,
}

impl ScanPage {
    pub fn new() -> Self {
        let b = gtk::Builder::from_resource(&format!("{RESOURCE_BASE}/ui/scan-dialog.ui"));
        Self {
            dialog: b.object("dialog").unwrap(),
            toasts: b.object("toasts").unwrap(),
            nav: b.object("nav").unwrap(),
            sites_page: b.object("sites_page").unwrap(),
            progress_page: b.object("progress_page").unwrap(),
            result_page: b.object("result_page").unwrap(),
            chosen: b.object("chosen").unwrap(),
            add_site: b.object("add_site").unwrap(),
            suggested: b.object("suggested").unwrap(),
            cancel_button: b.object("cancel_button").unwrap(),
            deep: b.object("deep").unwrap(),
            progress_detail: b.object("progress_detail").unwrap(),
            progress_bar: b.object("progress_bar").unwrap(),
            progress_count: b.object("progress_count").unwrap(),
            found_group: b.object("found_group").unwrap(),
            tried_group: b.object("tried_group").unwrap(),
            result_icon: b.object("result_icon").unwrap(),
            result_title: b.object("result_title").unwrap(),
            result_description: b.object("result_description").unwrap(),
            result_group: b.object("result_group").unwrap(),
            result_primary: b.object("result_primary").unwrap(),
            result_secondary: b.object("result_secondary").unwrap(),
            rows: RefCell::default(),
            scan: RefCell::default(),
            next: Cell::new((Next::Close, None)),
            round: Cell::new(0),
        }
    }

    fn add_row(&self, group: &adw::PreferencesGroup, row: &impl IsA<gtk::Widget>) {
        group.add(row);
        self.rows
            .borrow_mut()
            .push((group.clone(), row.clone().upcast()));
    }

    fn clear(&self, group: &adw::PreferencesGroup) {
        self.rows.borrow_mut().retain(|(g, row)| {
            if g == group {
                g.remove(row);
                false
            } else {
                true
            }
        });
    }
}

/// A row with a dim label and an icon at the end.
fn status_row(
    title: &str,
    subtitle: Option<&str>,
    text: &str,
    icon: Option<(&str, &str)>,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .use_markup(false)
        .build();
    if let Some(s) = subtitle {
        row.set_subtitle(s);
    }
    if !text.is_empty() {
        let label = gtk::Label::new(Some(text));
        label.add_css_class("dim-label");
        row.add_suffix(&label);
    }
    if let Some((icon, class)) = icon {
        let image = gtk::Image::builder()
            .icon_name(icon)
            .accessible_role(gtk::AccessibleRole::Presentation)
            .build();
        image.add_css_class(class);
        image.set_tooltip_text(Some(text));
        row.add_suffix(&image);
    }
    row
}

/// Best first: passed, fewer sites left blocked, fewer broken, faster (in
/// [`SAME_SPEED`] steps), then the method in use, simpler; the same order
/// as the Methods page (`Store::ranked`), so "Method N" match.
fn rank(tried: &mut [Tried], in_use: Option<&str>) {
    tried.sort_by_cached_key(|t| {
        let args = Strategy::parse(&t.strategy)
            .ok()
            .and_then(|s| s.method())
            .map_or_else(|| t.strategy.clone(), |m| m.to_string());
        (
            !t.passed,
            t.blocked.len(),
            t.broken.len(),
            t.ms / SAME_SPEED,
            Some(args.as_str()) != in_use,
            args.split_whitespace().count(),
            args.clone(),
        )
    });
}

/// The method to offer from `all` (best first): the best that opened
/// anything, or after Keep Looking, the best that works and hasn't been
/// offered yet.
fn answer(all: &[Tried], shown: &[String]) -> Option<Tried> {
    if shown.is_empty() {
        all.first().filter(|t| !t.opened.is_empty()).cloned()
    } else {
        all.iter()
            .find(|t| t.passed && !shown.contains(&t.strategy))
            .cloned()
    }
}

/// Writes what a scan found into the store: every method that opened
/// something, and every known method that opened nothing, so one that
/// stopped working (the internet provider changed its blocking) doesn't
/// keep saying it works. A stopped scan's failures don't count: the method
/// being tried when it stopped never got its chance.
fn record(store: &mut Store, tried: &[Tried], stopped: bool) {
    for t in tried {
        let Ok(method) = Strategy::parse(&t.strategy) else {
            continue;
        };
        let known = store.get(&method.to_string()).is_some();
        if t.opened.is_empty() && (stopped || !known) {
            continue;
        }
        let args = store.add(&method);
        let mut blocked = t.blocked.clone();
        blocked.extend(t.broken.iter().cloned());
        let ms = (t.ms > 0).then_some(t.ms);
        store.record(&args, t.opened.clone(), blocked, ms);
    }
}

/// Every method tried in this scan's rounds, each once (its latest try),
/// best first.
fn all_tried(scan: &Scan, in_use: Option<&str>) -> Vec<Tried> {
    let mut all: Vec<Tried> = Vec::new();
    for t in scan.kept.iter().chain(&scan.tried) {
        all.retain(|a| a.strategy != t.strategy);
        all.push(t.clone());
    }
    rank(&mut all, in_use);
    all
}

/// Whether to suggest the sites blocked in Türkiye: its time zone, or a
/// locale for the country (English speakers there count too).
fn in_turkiye() -> bool {
    if glib::TimeZone::local().identifier() == "Europe/Istanbul" {
        return true;
    }
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty())
        .is_some_and(|l| l.split('.').next().is_some_and(|l| l.ends_with("_TR")))
}

impl Ui {
    pub fn connect_scan(self: &Rc<Self>) {
        let page = &self.scan;
        page.add_site.connect_apply(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| ui.scan_add_site()
        ));
        page.add_site.connect_entry_activated(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| ui.scan_add_site()
        ));
        page.add_site
            .connect_changed(|entry| entry.remove_css_class("error"));
        page.deep.connect_active_notify(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |deep| ui.scan.scan.borrow_mut().deep = deep.is_active()
        ));
        page.cancel_button.connect_clicked(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| {
                ui.scan.dialog.close();
            }
        ));
        page.result_primary.connect_clicked(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| ui.scan_next(ui.scan.next.get().0)
        ));
        page.result_secondary.connect_clicked(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| {
                if let Some(next) = ui.scan.next.get().1 {
                    ui.scan_next(next);
                }
            }
        ));
    }

    /// Starts scanning the sites on the first page (win.start-scan);
    /// `deep` overrides the Quick/Deep choice.
    pub fn start_chosen_scan(self: &Rc<Self>, deep: Option<bool>) {
        if self.scan.scan.borrow().running {
            return;
        }
        let (sites, chosen_deep) = {
            let scan = self.scan.scan.borrow();
            (scan.chosen.clone(), scan.deep)
        };
        self.start_scan(sites, deep.unwrap_or(chosen_deep), Vec::new());
    }

    /// Start needs at least one site.
    fn set_can_start_scan(&self, can: bool) {
        if let Some(action) = self
            .window
            .lookup_action("start-scan")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(can);
        }
    }

    /// Opens the dialog: where a running or finished scan is, else the
    /// first page, with the sites that don't open already in it.
    pub fn show_scan(self: &Rc<Self>) {
        let page = &self.scan;
        let showing = page.nav.visible_page();
        let busy = page.scan.borrow().running || showing.as_ref() == Some(&page.result_page);
        if !busy {
            let blocked: Vec<String> = self
                .checks()
                .into_iter()
                .filter(|(_, c)| c.is_blocked())
                .map(|(s, _)| s)
                .collect();
            let chosen = if blocked.is_empty() {
                self.sites()
            } else {
                blocked
            };
            {
                let mut scan = page.scan.borrow_mut();
                scan.chosen = chosen;
                scan.deep = false;
            }
            page.deep.set_active(false);
            page.nav.replace(std::slice::from_ref(&page.sites_page));
            self.render_scan_sites();
        }
        page.dialog.present(Some(&self.window));
    }

    fn scan_add_site(self: &Rc<Self>) {
        let entry = &self.scan.add_site;
        let text = entry.text();
        if text.trim().is_empty() {
            return;
        }
        match site::normalize(&text) {
            Ok(domain) => {
                let mut scan = self.scan.scan.borrow_mut();
                if !scan.chosen.contains(&domain) {
                    scan.chosen.push(domain);
                }
                drop(scan);
                entry.set_text("");
                self.render_scan_sites();
            }
            Err(_) => {
                entry.add_css_class("error");
                self.scan.toasts.add_toast(
                    adw::Toast::builder()
                        .title(gettext("“{text}” isn’t a site name").replace("{text}", text.trim()))
                        .use_markup(false)
                        .build(),
                );
            }
        }
    }

    fn render_scan_sites(self: &Rc<Self>) {
        let page = &self.scan;
        page.clear(&page.chosen);
        page.clear(&page.suggested);
        let chosen = page.scan.borrow().chosen.clone();
        for site in &chosen {
            let row = adw::ActionRow::builder()
                .title(Self::site_name(site))
                .subtitle(site.as_str())
                .use_markup(false)
                .build();
            let remove = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .valign(gtk::Align::Center)
                .tooltip_text(gettext("Remove {site}").replace("{site}", &Self::site_name(site)))
                .build();
            remove.add_css_class("flat");
            remove.connect_clicked(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                #[strong]
                site,
                move |_| {
                    ui.scan.scan.borrow_mut().chosen.retain(|s| *s != site);
                    ui.render_scan_sites();
                }
            ));
            row.add_suffix(&remove);
            page.add_row(&page.chosen, &row);
        }
        // services not picked yet, one tick away
        page.suggested.set_title(&if in_turkiye() {
            gettext("Often Blocked in Türkiye")
        } else {
            gettext("Often Blocked")
        });
        let mut any = false;
        for service in services::SERVICES {
            let main = service.domains[0].to_owned();
            if chosen
                .iter()
                .any(|s| services::find(s).is_some_and(|f| f.name == service.name))
            {
                continue;
            }
            any = true;
            let check = gtk::CheckButton::builder()
                .valign(gtk::Align::Center)
                .build();
            let row = adw::ActionRow::builder()
                .title(service.name)
                .activatable_widget(&check)
                .build();
            row.add_prefix(&check);
            check.connect_toggled(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                move |check| {
                    let mut scan = ui.scan.scan.borrow_mut();
                    if check.is_active() {
                        if !scan.chosen.contains(&main) {
                            scan.chosen.push(main.clone());
                        }
                    } else {
                        scan.chosen.retain(|s| *s != main);
                    }
                    let empty = scan.chosen.is_empty();
                    drop(scan);
                    ui.set_can_start_scan(!empty);
                }
            ));
            page.add_row(&page.suggested, &row);
        }
        page.suggested.set_visible(any);
        self.set_can_start_scan(!chosen.is_empty());
    }

    /// Starts a scan of `sites` and shows its progress; `kept` are results
    /// of an earlier round to keep showing.
    fn start_scan(self: &Rc<Self>, sites: Vec<String>, deep: bool, kept: Vec<Tried>) {
        let Some(helper) = self.helper() else {
            return;
        };
        if sites.is_empty() {
            return;
        }
        let page = &self.scan;
        let round = page.round.get() + 1;
        page.round.set(round);
        {
            let mut scan = page.scan.borrow_mut();
            *scan = Scan {
                chosen: scan.chosen.clone(),
                deep,
                sites: sites.clone(),
                running: true,
                started: Some(Instant::now()),
                // a new search starts over; Keep Looking goes on from here
                shown: if kept.is_empty() {
                    Vec::new()
                } else {
                    scan.shown.clone()
                },
                kept,
                ..Scan::default()
            };
        }
        page.nav.replace(std::slice::from_ref(&page.progress_page));
        self.render_scan_progress();
        page.progress_detail.announce(
            &gettext("Looking for a way around the block"),
            gtk::AccessibleAnnouncementPriority::Medium,
        );

        // methods that worked here before go first
        let first: Vec<String> = {
            let store = self.methods.borrow();
            store
                .ranked(&sites, self.current_method().as_deref())
                .into_iter()
                .filter(|m| store.standing(&m.args, &sites) == Standing::Works)
                .take(3)
                .map(|m| m.args.clone())
                .collect()
        };
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let mut events = match helper.scan_events().await {
                Ok(events) => events,
                Err(e) => {
                    eprintln!("sluis-helper: {e}");
                    ui.scan_failed(&gettext("Couldn’t reach Zapret"));
                    return;
                }
            };
            let started = if deep {
                helper.start_thorough_scan(&sites).await
            } else {
                // every method, not just the first that passes: the answer
                // is the fastest of those that open the most sites
                helper.start_scan(&sites, &first, 0).await
            };
            if let Err(message) = started {
                ui.scan_failed(&message);
                return;
            }
            while let Some(event) = events.next().await {
                if ui.scan.round.get() != round {
                    return;
                }
                let finished = matches!(event, ScanEvent::Finished { .. });
                ui.scan_event(event);
                if finished {
                    return;
                }
            }
        });
    }

    fn scan_failed(self: &Rc<Self>, message: &str) {
        let page = &self.scan;
        page.scan.borrow_mut().running = false;
        page.nav.replace(std::slice::from_ref(&page.sites_page));
        self.render_scan_sites();
        page.toasts.add_toast(adw::Toast::new(message));
    }

    fn scan_event(self: &Rc<Self>, event: ScanEvent) {
        let page = &self.scan;
        match event {
            ScanEvent::Site { site, diagnosis } => {
                page.scan.borrow_mut().diagnosis.push((site, diagnosis));
            }
            ScanEvent::Tried(tried) => {
                let mut scan = page.scan.borrow_mut();
                // a Thorough Scan reports a method again as it opens more
                scan.tried.retain(|t| t.strategy != tried.strategy);
                scan.tried.push(tried);
            }
            ScanEvent::Progress { done, total } => {
                let mut scan = page.scan.borrow_mut();
                scan.done = done;
                scan.total = total;
            }
            ScanEvent::Finished { stopped, error } => {
                {
                    let mut scan = page.scan.borrow_mut();
                    scan.running = false;
                    scan.stopped = stopped;
                    scan.took = scan.started.map(|s| s.elapsed().as_secs());
                }
                if !error.is_empty() {
                    eprintln!("sluis-helper: scan failed: {error}");
                    self.scan_failed(&gettext("The scan couldn’t start"));
                    return;
                }
                self.record_scan();
                self.show_scan_result(stopped);
                return;
            }
        }
        self.render_scan_progress();
    }

    /// Remembers every method that opened something, with what it opened,
    /// so the Methods page ranks it.
    fn record_scan(&self) {
        let scan = self.scan.scan.borrow();
        let mut store = self.methods.borrow_mut();
        record(&mut store, &scan.tried, scan.stopped);
        store.save();
    }

    fn render_scan_progress(self: &Rc<Self>) {
        let page = &self.scan;
        let scan = page.scan.borrow();
        let elapsed = scan.started.map_or(0, |s| s.elapsed().as_secs());
        if scan.deep {
            page.progress_bar.pulse();
            page.progress_detail.set_label(&gettext(
                "This can take up to 30 minutes. You can close this window; Sluis tells you when it’s done.",
            ));
            page.progress_count.set_label(
                &ngettext("{n} way tried", "{n} ways tried", scan.done)
                    .replace("{n}", &scan.done.to_string()),
            );
        } else {
            let fraction = if scan.total == 0 {
                0.0
            } else {
                f64::from(scan.done) / f64::from(scan.total)
            };
            page.progress_bar.set_fraction(fraction);
            page.progress_detail.set_label(&if scan.done < 2 {
                gettext("This takes about a minute. You can keep using your computer.")
            } else {
                let left = elapsed * u64::from(scan.total - scan.done) / u64::from(scan.done);
                ngettext(
                    "About {n} second left. You can keep using your computer.",
                    "About {n} seconds left. You can keep using your computer.",
                    left.clamp(1, 600) as u32,
                )
                .replace("{n}", &left.clamp(1, 600).to_string())
            });
            page.progress_count.set_label(&if scan.total == 0 {
                String::new()
            } else {
                gettext("{done} of {total} ways tried")
                    .replace("{done}", &scan.done.to_string())
                    .replace("{total}", &scan.total.to_string())
            });
        }

        // what the diagnosis found
        page.clear(&page.found_group);
        let names = |kind: &[&str]| -> Vec<String> {
            scan.diagnosis
                .iter()
                .filter(|(_, d)| kind.contains(&d.as_str()))
                .map(|(s, _)| Self::site_name(s))
                .collect()
        };
        if scan.diagnosis.is_empty() && !scan.deep {
            let row = adw::ActionRow::builder()
                .title(gettext("Checking your sites…"))
                .build();
            row.add_suffix(&adw::Spinner::new());
            page.add_row(&page.found_group, &row);
        }
        let opens = names(&["opens"]);
        if !opens.is_empty() {
            page.add_row(
                &page.found_group,
                &status_row(
                    &ngettext(
                        "{sites} already opens",
                        "{sites} already open",
                        opens.len() as u32,
                    )
                    .replace("{sites}", &list(&opens)),
                    Some(&ngettext(
                        "Sluis isn’t needed for it",
                        "Sluis isn’t needed for them",
                        opens.len() as u32,
                    )),
                    "",
                    Some(("object-select-symbolic", "success")),
                ),
            );
        }
        let blocked = names(&["blocked", "dns-spoofed-blocked"]);
        if !blocked.is_empty() {
            page.add_row(
                &page.found_group,
                &status_row(
                    &ngettext(
                        "{sites} is blocked by your internet provider",
                        "{sites} are blocked by your internet provider",
                        blocked.len() as u32,
                    )
                    .replace("{sites}", &list(&blocked)),
                    Some(&gettext(
                        "Blocked on purpose, but Sluis can usually get around it",
                    )),
                    "",
                    Some(("dialog-information-symbolic", "accent")),
                ),
            );
        }
        let spoofed = names(&["dns-spoofed", "dns-spoofed-blocked"]);
        if !spoofed.is_empty() {
            page.add_row(
                &page.found_group,
                &status_row(
                    &gettext("Your internet provider gives a wrong address for {sites}")
                        .replace("{sites}", &list(&spoofed)),
                    Some(&gettext(
                        "Turning on secure DNS in your browser or system fixes this",
                    )),
                    "",
                    Some(("dialog-warning-symbolic", "warning")),
                ),
            );
        }
        let gone = names(&["unreachable", "not-found"]);
        if !gone.is_empty() {
            page.add_row(
                &page.found_group,
                &status_row(
                    &ngettext(
                        "{sites} doesn’t answer at all",
                        "{sites} don’t answer at all",
                        gone.len() as u32,
                    )
                    .replace("{sites}", &list(&gone)),
                    Some(&gettext(
                        "It may be down, or blocked in a way Sluis can’t get around",
                    )),
                    "",
                    Some(("window-close-symbolic", "error")),
                ),
            );
        }
        page.found_group
            .set_visible(!scan.deep || !scan.diagnosis.is_empty());

        // the latest methods first
        page.clear(&page.tried_group);
        for t in scan.tried.iter().rev().take(8) {
            let total = t.opened.len() + t.blocked.len();
            let (text, icon) = if t.passed {
                (
                    gettext("Opened every site"),
                    ("object-select-symbolic", "success"),
                )
            } else if !t.broken.is_empty() {
                (
                    gettext("Breaks {sites}").replace(
                        "{sites}",
                        &list(
                            &t.broken
                                .iter()
                                .map(|s| Self::site_name(s))
                                .collect::<Vec<_>>(),
                        ),
                    ),
                    ("window-close-symbolic", "dim-label"),
                )
            } else if t.opened.is_empty() {
                (
                    gettext("Didn’t work"),
                    ("window-close-symbolic", "dim-label"),
                )
            } else {
                (
                    gettext("Opened {n} of {total} sites")
                        .replace("{n}", &t.opened.len().to_string())
                        .replace("{total}", &total.to_string()),
                    ("window-close-symbolic", "dim-label"),
                )
            };
            let name = Strategy::parse(&t.strategy)
                .map(|s| describe(&s).to_string())
                .unwrap_or_else(|_| t.strategy.clone());
            let row = status_row(&name, Some(&text), "", Some(icon));
            row.set_tooltip_text(Some(&t.strategy));
            page.add_row(&page.tried_group, &row);
        }
        page.tried_group.set_visible(!scan.tried.is_empty());
    }

    fn show_scan_result(self: &Rc<Self>, stopped: bool) {
        let page = &self.scan;
        let (sites, deep, all, elapsed, diagnosis, shown) = {
            let scan = page.scan.borrow();
            (
                scan.sites.clone(),
                scan.deep,
                all_tried(&scan, self.current_method().as_deref()),
                // the scan's own time, not how long the result was looked at
                scan.took
                    .or_else(|| scan.started.map(|s| s.elapsed().as_secs()))
                    .unwrap_or(0),
                scan.diagnosis.clone(),
                scan.shown.clone(),
            )
        };
        let best = answer(&all, &shown);
        page.scan.borrow_mut().showing = None;
        let of = |kind: &[&str]| -> Vec<String> {
            diagnosis
                .iter()
                .filter(|(_, d)| kind.contains(&d.as_str()))
                .map(|(s, _)| s.clone())
                .collect()
        };
        let blocked = if deep {
            sites.clone()
        } else {
            of(&["blocked", "dns-spoofed-blocked"])
        };
        let opens = of(&["opens"]);
        let names = |sites: &[String]| -> Vec<String> {
            sites.iter().map(|s| Self::site_name(s)).collect()
        };

        if stopped && best.is_none() && shown.is_empty() {
            page.nav.replace(std::slice::from_ref(&page.sites_page));
            self.render_scan_sites();
            return;
        }
        page.clear(&page.result_group);
        let found = best.as_ref().is_some_and(|b| b.passed);
        let mut next = (Next::Close, None);
        let (icon, class, title, description) = if !deep && blocked.is_empty() {
            let gone = of(&["unreachable", "not-found"]);
            let spoofed = of(&["dns-spoofed"]);
            if !gone.is_empty() {
                (
                    "sluis-chained-symbolic",
                    "bad",
                    gettext("Can’t Reach {sites}").replace("{sites}", &list(&names(&gone))),
                    gettext(
                        "They don’t answer at all, even without the block. They may be down, or blocked in a way Sluis can’t get around.",
                    ),
                )
            } else if !spoofed.is_empty() {
                (
                    "sluis-chained-symbolic",
                    "bad",
                    gettext("Your Internet Provider Gives Wrong Addresses"),
                    gettext("{sites} open at their real address. Turn on secure DNS (DNS over HTTPS) in your browser or system settings; Sluis isn’t needed.")
                        .replace("{sites}", &list(&names(&spoofed))),
                )
            } else {
                (
                    "sluis-freed-symbolic",
                    "ok",
                    gettext("These Sites Already Open"),
                    gettext("Your internet provider doesn’t block {sites}, so Sluis isn’t needed for them.")
                        .replace("{sites}", &list(&names(&opens))),
                )
            }
        } else if best.is_none() && !shown.is_empty() {
            // Keep Looking found nothing new: offer the best one again
            let args = self.stored_args(&shown[0]);
            let in_use = self.status().is_some_and(|s| s.running)
                && self.current_method().as_deref() == Some(args.as_str());
            page.scan.borrow_mut().showing = Some(shown[0].clone());
            next = (
                if in_use { Next::Close } else { Next::TurnOn },
                Some(Next::Deep),
            );
            (
                "sluis-freed-symbolic",
                "ok",
                gettext("No Other Way Found"),
                ngettext(
                    "Sluis tried the {n} common way, and {name} is still the best. A deep scan tries everything; it can take up to 30 minutes.",
                    "Sluis tried all {n} common ways, and {name} is still the best. A deep scan tries everything; it can take up to 30 minutes.",
                    all.len() as u32,
                )
                .replace("{n}", &all.len().to_string())
                .replace("{name}", &self.method_name(&args)),
            )
        } else if let (true, Some(best)) = (found, &best) {
            let args = self.stored_args(&best.strategy);
            let name = self.method_name(&args);
            {
                let mut scan = page.scan.borrow_mut();
                scan.showing = Some(best.strategy.clone());
                scan.shown.push(best.strategy.clone());
            }
            let in_use = self.status().is_some_and(|s| s.running)
                && self.current_method().as_deref() == Some(args.as_str());
            next = (
                if in_use { Next::Close } else { Next::TurnOn },
                if deep { None } else { Some(Next::KeepLooking) },
            );
            for site in &best.opened {
                page.add_row(
                    &page.result_group,
                    &status_row(
                        &Self::site_name(site),
                        None,
                        &gettext("Opens"),
                        Some(("object-select-symbolic", "success")),
                    ),
                );
            }
            if !deep && !opens.is_empty() {
                page.add_row(
                    &page.result_group,
                    &status_row(
                        &ngettext(
                            "Your other site",
                            "Your {n} other sites",
                            opens.len() as u32,
                        )
                        .replace("{n}", &opens.len().to_string()),
                        None,
                        &ngettext("Still works", "Still work", opens.len() as u32),
                        Some(("object-select-symbolic", "success")),
                    ),
                );
            }
            let mut description = if in_use {
                gettext("{name}, the method Sluis already uses, opened every site, twice. It took {time}.")
            } else if deep {
                gettext("{name} opened every site. It took {time}.")
            } else {
                gettext("{name} opened every site, twice, and your other sites still work. It took {time}.")
            }
            .replace("{name}", &name)
            .replace("{time}", &duration(elapsed));
            // tested at their real address: the method alone won't open
            // them while the provider's DNS sends them elsewhere
            let spoofed: Vec<String> = of(&["dns-spoofed-blocked"])
                .into_iter()
                .filter(|s| best.opened.contains(s))
                .collect();
            if !spoofed.is_empty() {
                description.push(' ');
                description.push_str(
                    &gettext("Your internet provider also gives wrong addresses for {sites}: turn on secure DNS (DNS over HTTPS) in your browser or in your network settings too, or they won’t open.")
                        .replace("{sites}", &list(&names(&spoofed))),
                );
            }
            (
                "sluis-freed-symbolic",
                "ok",
                gettext("Found a Way Around the Block"),
                description,
            )
        } else if let (true, Some(best)) = (deep, &best) {
            // the best a Thorough Scan found opens some sites
            let args = self.stored_args(&best.strategy);
            page.scan.borrow_mut().showing = Some(best.strategy.clone());
            next = (Next::TurnOn, Some(Next::Close));
            for site in &best.opened {
                page.add_row(
                    &page.result_group,
                    &status_row(
                        &Self::site_name(site),
                        None,
                        &gettext("Opens"),
                        Some(("object-select-symbolic", "success")),
                    ),
                );
            }
            for site in &best.blocked {
                page.add_row(
                    &page.result_group,
                    &status_row(
                        &Self::site_name(site),
                        None,
                        &gettext("Still blocked"),
                        Some(("dialog-warning-symbolic", "warning")),
                    ),
                );
            }
            (
                "sluis-chained-symbolic",
                "bad",
                gettext("Found a Way Around for Some Sites"),
                gettext("{name} opens {opened}, but nothing Sluis knows opens {blocked}.")
                    .replace("{name}", &self.method_name(&args))
                    .replace("{opened}", &list(&names(&best.opened)))
                    .replace("{blocked}", &list(&names(&best.blocked))),
            )
        } else if deep {
            (
                "sluis-chained-symbolic",
                "bad",
                gettext("No Way Around Found"),
                gettext("Sluis tried everything it knows. {sites} may be blocked in a way Sluis can’t get around.")
                    .replace("{sites}", &list(&names(&blocked))),
            )
        } else {
            next = (Next::Deep, Some(Next::Close));
            (
                "sluis-chained-symbolic",
                "bad",
                gettext("No Quick Fix Found"),
                ngettext(
                    "None of the {n} common ways opened {sites}. A deep scan tries everything; it can take up to 30 minutes.",
                    "None of the {n} common ways opened {sites}. A deep scan tries everything; it can take up to 30 minutes.",
                    all.len() as u32,
                )
                .replace("{n}", &all.len().to_string())
                .replace("{sites}", &list(&names(&blocked))),
            )
        };
        page.result_group.set_visible(
            page.rows
                .borrow()
                .iter()
                .any(|(g, _)| *g == page.result_group),
        );
        page.result_icon.set_icon_name(Some(icon));
        page.result_icon.remove_css_class("ok");
        page.result_icon.remove_css_class("bad");
        page.result_icon.add_css_class(class);
        page.result_title.set_label(&title);
        page.result_title.announce(
            &format!("{title}. {description}"),
            gtk::AccessibleAnnouncementPriority::High,
        );
        page.result_description.set_label(&description);
        let label = |n: Next| match n {
            Next::TurnOn => gettext("_Turn It On"),
            Next::KeepLooking => gettext("_Keep Looking"),
            Next::Deep => gettext("Start _Deep Scan"),
            Next::Close => gettext("_Close"),
        };
        page.result_primary.set_label(&label(next.0));
        page.result_primary.set_visible(true);
        if next.0 == Next::Close {
            page.result_primary.remove_css_class("suggested-action");
        } else {
            page.result_primary.add_css_class("suggested-action");
        }
        page.next.set(next);
        page.result_secondary.set_visible(next.1.is_some());
        if let Some(n) = next.1 {
            page.result_secondary.set_label(&label(n));
        }
        page.nav.replace(std::slice::from_ref(&page.result_page));
        self.notify_scan_done(&title, &description);
    }

    /// The method's arguments as the store keeps them.
    fn stored_args(&self, strategy: &str) -> String {
        Strategy::parse(strategy)
            .ok()
            .and_then(|s| s.method())
            .map_or_else(|| strategy.to_owned(), |m| m.to_string())
    }

    fn scan_next(self: &Rc<Self>, next: Next) {
        let page = &self.scan;
        let (sites, showing, more, stopped) = {
            let scan = page.scan.borrow();
            let more = answer(
                &all_tried(&scan, self.current_method().as_deref()),
                &scan.shown,
            )
            .is_some();
            (scan.sites.clone(), scan.showing.clone(), more, scan.stopped)
        };
        match next {
            Next::TurnOn => {
                page.dialog.close();
                if let Some(strategy) = showing {
                    self.turn_on_with(self.stored_args(&strategy));
                }
            }
            // every method was tried already: show the next best at once;
            // only a scan that was stopped early has more to try
            Next::KeepLooking if more || !stopped => self.show_scan_result(false),
            Next::KeepLooking => {
                let kept = {
                    let scan = page.scan.borrow();
                    all_tried(&scan, None)
                };
                self.start_scan(sites, false, kept);
            }
            Next::Deep => {
                let blocked: Vec<String> = page
                    .scan
                    .borrow()
                    .diagnosis
                    .iter()
                    .filter(|(_, d)| d.contains("blocked"))
                    .map(|(s, _)| s.clone())
                    .collect();
                self.start_scan(
                    if blocked.is_empty() { sites } else { blocked },
                    true,
                    Vec::new(),
                );
            }
            Next::Close => {
                page.dialog.close();
            }
        }
    }

    /// Stops the running scan; the result page follows.
    pub fn stop_scan(self: &Rc<Self>) {
        let Some(helper) = self.helper() else {
            return;
        };
        let ui = self.clone();
        glib::spawn_future_local(async move {
            if let Err(message) = helper.stop_scan().await {
                ui.scan.toasts.add_toast(adw::Toast::new(&message));
            }
        });
    }

    /// Tells the user a scan finished when they aren't looking.
    fn notify_scan_done(&self, title: &str, body: &str) {
        if self.window.is_active() && self.scan.dialog.is_mapped() {
            return;
        }
        let Some(app) = self.window.application() else {
            return;
        };
        let notification = gio::Notification::new(title);
        notification.set_body(Some(body));
        notification.set_default_action("app.show-scan");
        app.send_notification(Some("scan"), &notification);
    }
}

/// "38 seconds", "4 minutes"
fn duration(seconds: u64) -> String {
    if seconds < 90 {
        ngettext("{n} second", "{n} seconds", seconds as u32).replace("{n}", &seconds.to_string())
    } else {
        let minutes = (seconds + 30) / 60;
        ngettext("{n} minute", "{n} minutes", minutes as u32).replace("{n}", &minutes.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTL8: &str = "--dpi-desync=fake --dpi-desync-ttl=8";
    const TTL9: &str = "--dpi-desync=fake --dpi-desync-ttl=9";
    const MD5: &str = "--dpi-desync=fake --dpi-desync-fooling=md5sig";

    fn tried(strategy: &str, opened: usize, ms: u32) -> Tried {
        let sites = ["discord.com", "roblox.com"];
        Tried {
            strategy: strategy.into(),
            opened: sites[..opened].iter().map(|s| s.to_string()).collect(),
            blocked: sites[opened..].iter().map(|s| s.to_string()).collect(),
            broken: Vec::new(),
            ms,
            passed: opened == sites.len(),
        }
    }

    fn scan(tried: Vec<Tried>) -> Scan {
        Scan {
            tried,
            ..Scan::default()
        }
    }

    #[test]
    fn the_fastest_beats_the_method_in_use() {
        let all = all_tried(
            &scan(vec![tried(TTL8, 2, 900), tried(TTL9, 2, 300)]),
            Some(TTL8),
        );
        assert_eq!(answer(&all, &[]).unwrap().strategy, TTL9);
    }

    #[test]
    fn the_method_in_use_wins_when_as_fast() {
        let all = all_tried(
            &scan(vec![tried(TTL9, 2, 310), tried(TTL8, 2, 320)]),
            Some(TTL8),
        );
        assert_eq!(answer(&all, &[]).unwrap().strategy, TTL8);
    }

    #[test]
    fn opening_every_site_beats_speed() {
        let all = all_tried(&scan(vec![tried(TTL9, 1, 100), tried(TTL8, 2, 900)]), None);
        assert_eq!(answer(&all, &[]).unwrap().strategy, TTL8);
    }

    #[test]
    fn keep_looking_offers_the_next_best_then_nothing() {
        let all = all_tried(
            &scan(vec![
                tried(TTL8, 2, 300),
                tried(TTL9, 2, 600),
                tried(MD5, 1, 100),
            ]),
            Some(TTL8),
        );
        let mut shown = vec![answer(&all, &[]).unwrap().strategy];
        assert_eq!(shown, [TTL8]);
        let next = answer(&all, &shown).unwrap();
        assert_eq!(next.strategy, TTL9);
        shown.push(next.strategy);
        // MD5 leaves a site blocked: not worth offering after a working one
        assert_eq!(answer(&all, &shown), None);
    }

    #[test]
    fn a_known_method_that_stopped_working_says_so() {
        let mut store = Store::default();
        record(&mut store, &[tried(TTL8, 2, 300)], false);
        let sites: Vec<String> = vec!["discord.com".into(), "roblox.com".into()];
        assert_eq!(store.standing(TTL8, &sites), Standing::Works);
        // the next scan: it opens nothing any more; MD5 never did
        record(&mut store, &[tried(TTL8, 0, 0), tried(MD5, 0, 0)], false);
        assert_eq!(
            store.standing(TTL8, &sites),
            Standing::Partly {
                opened: 0,
                total: 2
            }
        );
        assert!(store.get(MD5).is_none(), "dead methods aren't added");
    }

    #[test]
    fn a_stopped_scan_doesnt_count_failures() {
        let mut store = Store::default();
        record(&mut store, &[tried(TTL8, 2, 300)], false);
        record(&mut store, &[tried(TTL8, 0, 0)], true);
        let sites: Vec<String> = vec!["discord.com".into(), "roblox.com".into()];
        assert_eq!(store.standing(TTL8, &sites), Standing::Works);
    }

    #[test]
    fn a_method_tried_again_counts_once() {
        let mut s = scan(vec![tried(TTL8, 2, 300)]);
        s.kept = vec![tried(TTL8, 2, 900), tried(TTL9, 2, 400)];
        let all = all_tried(&s, None);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].strategy, TTL8);
        assert_eq!(all[0].ms, 300);
    }
}
