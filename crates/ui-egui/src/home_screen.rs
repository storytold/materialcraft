//! Help ▸ Home screen, also shown at startup: start a graph from a template, open a package or a
//! recent one, and find the tutorials, documentation and community

use craft_ui::choice::Checkbox;
use craft_ui::contract::{Bind, Props as _, Show as _, ShowContainer as _};
use craft_ui::dialog::Dialog;
use craft_ui::display::{Shade, Text};
use craft_ui::runtime::commands::CommandRef;
use craft_ui::shell::{RecentFile, RecentSort, RecentView, StartAction, StartLayout, StartLink, StartPlace, StartScreen};
use craft_ui::style::TypeRole;
use egui::Ui;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::template_card::TemplateCard;

/// The links down the left side, under their headings: (heading, [(label, Help menu id)])
const LINKS: [(&str, &[(&str, &str)]); 2] = [
    ("Learn", &[("Tutorials", "ui.help.tutorials"), ("Documentation", "ui.help.docs"), ("What's new", "ui.help.news")]),
    ("Community", &[("Community (Discord)", "ui.help.discord"), ("Report a bug", "ui.help.bug"), ("All Craft apps", "ui.help.apps")]),
];

/// Home as a tool window over the start screen: it leaves the app usable behind it, and closes on every action it runs
pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let templates = app.thumbs.templates(ui.ctx());
    let source = crate::thumbs::TemplateSource;
    let cards: Vec<craft_ui::shell::Template> = crate::template_card::cards().iter().map(crate::template_card::template).collect();
    // The links run the Help menu's commands, so Home and the menu open a page the one way automation also reaches.
    // PLAN: the framework's Home rail has no group headings, so "Learn" leads the rail's links and "Community" its footer without their headings
    let [(_, learn), (_, community)] = LINKS;
    let link_rows = |rows: &[(&'static str, &'static str)]| -> Vec<StartLink> {
        rows.iter().map(|&(label, id)| StartLink::command(tr(label), None, CommandRef::new(id))).collect()
    };
    let (links, footer_links) = (link_rows(learn), link_rows(community));
    // Production lists no recent packages in the browser, where a path cannot be opened again.
    #[cfg(not(target_arch = "wasm32"))]
    let recent: Vec<RecentFile> = app.ui.recent.iter().map(|path| RecentFile::new(path, file_name(path))).collect();
    #[cfg(target_arch = "wasm32")]
    let recent: Vec<RecentFile> = Vec::new();
    let version = format!("{} {}", tr("Version"), materialcraft_engine::VERSION);
    let show_home = &mut app.ui.prefs.show_home;

    let mut open = app.ui.windows.home;
    // Production's fixed Home size, kept inside the window; the rail and the page scroll on their own
    let shown = Dialog::new(tr("Home"))
        .id_salt("home")
        .modeless()
        .resizable(true)
        .overlay(|overlay| overlay.body_scroll(false).size(egui::vec2(1000.0, 620.0)))
        .opened(Bind::bound(&mut open))
        .show(ui, |ui, _| {
            // Production shows the list newest first and saves neither choice.
            let (mut view, mut sort) = (RecentView::List, RecentSort::Opened);
            StartScreen::new(Bind::bound(&mut view), Bind::bound(&mut sort), &recent)
                .home_layout(tr("Welcome to MaterialCraft"), &footer_links)
                .layout(StartLayout::Greeting)
                .new_label(tr("New graph..."))
                .open_label(tr("Open package..."))
                .links(&links)
                .templates(&cards)
                .templates_title(tr("Start from a template"))
                .recent_title(tr("Recent packages"))
                .max_recent(recent.len())
                .thumbnails(&templates, &source)
                .section(StartPlace::BeforeRecent, |ui, _| {
                    Text::new(tr(
                        "Build tileable materials from noises, patterns and filters, see them live in 2D and 3D, and export the maps for any engine.",
                    ))
                    .shade(Shade::Muted)
                    .show(ui);
                    Text::new(version.clone()).role(TypeRole::Caption).shade(Shade::Muted).show(ui);
                })
                .section(StartPlace::AfterSections, |ui, _| {
                    let _ = Checkbox::new(Bind::bound(&mut *show_home)).label(tr("Show at startup")).show(ui);
                })
                .show(ui)
                .actions
        });
    app.ui.windows.home = open;

    // The body may run twice in a pass; the last run's actions are the pass's.
    for action in shown.inner.unwrap_or_default() {
        run_action(app, ui.ctx(), action);
    }
}

/// Run what the user did on the start screen. A recent package or a template waits while a file dialog is open, and
/// Home stays open then; the start screen reads no command state, so its cards look enabled.
fn run_action(app: &mut MaterialCraftApp, ctx: &egui::Context, action: StartAction) {
    let waits = crate::native_dialogs::pending(app);
    match action {
        StartAction::New => {
            app.ui.windows.home = false;
            crate::new_graph_dialog::open(app, materialcraft_engine::core::GraphKind::Material);
        }
        StartAction::Open => {
            app.ui.windows.home = false;
            crate::menus::invoke(app, ctx, "file.open");
        }
        #[cfg(not(target_arch = "wasm32"))]
        StartAction::OpenRecent(path) if !waits => {
            crate::windows::open_path(app, &path.to_string_lossy());
            app.ui.windows.home = false;
        }
        StartAction::UseTemplate(id) if !waits => {
            if let Some(card) = crate::template_card::cards().iter().find(|card| card.id == id) {
                begin_with(app, card);
            }
        }
        // The props Home sets offer no other action (no remove, pinned folders or more templates).
        _ => {}
    }
}

/// A card opens the New graph dialog with its template chosen
fn begin_with(app: &mut MaterialCraftApp, card: &TemplateCard) {
    app.ui.windows.home = false;
    crate::new_graph_dialog::open(app, card.kind);
    app.ui.new_graph_template = card.id.to_string();
}

/// The name a recent package is listed by: its file name, else the whole path
#[cfg(not(target_arch = "wasm32"))]
fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map_or_else(|| path.to_owned(), |name| name.to_string_lossy().into_owned())
}
