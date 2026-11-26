//! File ▸ New ▸ Graph: template cards by category on the left; the graph's name, size and the
//! package it goes in on the right

use craft_ui::contract::{Bind, Entry, FieldLayout, Props as _, Show as _, ShowContainer as _, Sizing};
use craft_ui::form::Field;
use craft_ui::layout_components::Row;
use craft_ui::patterns::form_dialogs::{CardCx, CardSource, DetailsCx, DocumentCard, DocumentSettings, DocumentTabs, NewDocumentDialog, PresetCategory};
use craft_ui::pickers::Select;
use craft_ui::runtime::env::Label;
use craft_ui::shell::ThumbKey;
use craft_ui::text::link_toggle;
use egui::Ui;
use materialcraft_engine::core::GraphKind;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::template_card::{FUNCTION_TEMPLATE, TemplateCard};

/// The sides a new graph can have, in pixels
const SIDES: [u32; 6] = [256, 512, 1024, 2048, 4096, 8192];
/// The categories the dialog lists: every template, or the templates of one kind of graph
const CATEGORIES: [(Option<GraphKind>, &str); 3] =
    [(None, "All templates"), (Some(GraphKind::Material), "Material graph"), (Some(GraphKind::Function), "Function graph")];

/// Open the dialog for a kind of graph, aimed at the active package (or a new one when none is open)
pub(crate) fn open(app: &mut MaterialCraftApp, kind: GraphKind) {
    app.ui.new_graph_kind = kind;
    app.ui.new_graph_category = Some(kind);
    if kind == GraphKind::Function {
        app.ui.new_graph_template = FUNCTION_TEMPLATE.into();
    } else if app.ui.new_graph_template == FUNCTION_TEMPLATE {
        app.ui.new_graph_template = "pbr".into();
    }
    app.ui.new_graph_package = app.session.has_package().then_some(app.session.document_id);
    app.ui.windows.new_graph = true;
}

/// Pick a card: its kind decides the graph's kind
fn choose(app: &mut MaterialCraftApp, card: &TemplateCard) {
    app.ui.new_graph_kind = card.kind;
    app.ui.new_graph_template = card.id.to_string();
}

/// The id of a category's tab: the one mapping between `new_graph_category` and the dialog's tab binding
fn category_id(kind: Option<GraphKind>) -> &'static str {
    match kind {
        None => "all",
        Some(GraphKind::Material) => "material",
        Some(GraphKind::Function) => "function",
    }
}

/// The New graph dialog over the `new_graph_*` fields: each pass reads them into a `NewGraph` and writes the dialog's
/// edits back, so the saved fields and the other writers of them (Home, the menus) stay as they are
pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let templates = app.thumbs.templates(ui.ctx());
    let source = crate::thumbs::TemplateSource;
    let title = if app.ui.new_graph_kind == GraphKind::Function { tr("New function graph") } else { tr("New graph") };
    // PLAN: the dialog's card grid has no groups, so "All templates" lists the cards without the "Material"/"Function" heading lines; each card's line names its kind
    let categories: Vec<PresetCategory<TemplateCard>> = CATEGORIES
        .iter()
        .map(|&(kind, label)| {
            let cards = crate::template_card::cards().into_iter().filter(|card| kind.is_none_or(|kind| card.kind == kind)).collect();
            PresetCategory::new(category_id(kind), tr(label), cards)
        })
        .collect();
    let mut graph = NewGraph::of(app);
    let mut tab = category_id(app.ui.new_graph_category).to_owned();
    let mut open = app.ui.windows.new_graph;

    let output = NewDocumentDialog::over(Bind::bound(&mut graph))
        .title(title)
        .tabs(DocumentTabs::Presets)
        .categories(categories)
        .tab(Bind::bound(&mut tab))
        .opened(Bind::bound(&mut open))
        .thumbnails(&templates, &source, |id| ThumbKey::Template(id.to_owned()))
        .show(ui);

    app.ui.windows.new_graph = open;
    if let Some(&(kind, _)) = CATEGORIES.iter().find(|(kind, _)| category_id(*kind) == tab) {
        app.ui.new_graph_category = kind;
    }
    graph.store(app);
    if let Some(result) = output.result {
        result.options.store(app);
        // The framework Create path does not check DocumentSettings::fits; keep a closed package from being used.
        let created = if result.options.fits() {
            run(app)
        } else {
            app.toast(tr("The package chosen for the graph is no longer open"), true);
            false
        };
        if created {
            result.remember(ui.ctx());
        } else {
            app.ui.windows.new_graph = true;
        }
    }
}

/// Create the graph. Returns true when it was created
fn run(app: &mut MaterialCraftApp) -> bool {
    let target = app.ui.new_graph_package;
    let kind = if app.ui.new_graph_kind == GraphKind::Function { "function" } else { "material" };
    let height = if app.ui.new_graph_linked { app.ui.new_graph_size } else { app.ui.new_graph_height };
    let parameters = json!({
        "kind": kind,
        "template": app.ui.new_graph_template,
        "name": app.ui.new_graph_name,
        "size": app.ui.new_graph_size,
        "height": height,
        "document": target,
        "newPackage": target.is_none(),
    });
    let created = app.run("file.new", parameters).is_some();
    if created {
        app.ui.frame_all = true;
    }
    created
}

/// What the New graph dialog edits: the `new_graph_*` fields of the app's state, with the open packages the graph can
/// go in
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct NewGraph {
    kind: GraphKind,
    template: String,
    name: String,
    width: u32,
    height: u32,
    linked: bool,
    /// The package the graph goes in; `None` is a new package
    package: Option<u64>,
    /// The open packages as `(id, label)`, in the session's order
    packages: Vec<(u64, String)>,
}

impl NewGraph {
    /// The dialog's settings as the app holds them now
    fn of(app: &MaterialCraftApp) -> Self {
        let packages = app
            .session
            .open_packages()
            .into_iter()
            .map(|(id, package, path, _)| {
                let label = match path {
                    Some(path) => format!("{} — {path}", package.name),
                    None => format!("{} (#{id})", package.name),
                };
                (id, label)
            })
            .collect();
        NewGraph {
            kind: app.ui.new_graph_kind,
            template: app.ui.new_graph_template.clone(),
            name: app.ui.new_graph_name.clone(),
            width: app.ui.new_graph_size,
            height: app.ui.new_graph_height,
            linked: app.ui.new_graph_linked,
            package: app.ui.new_graph_package,
            packages,
        }
    }

    /// Writes the settings back to the app's fields; the chosen card goes through `choose`
    fn store(&self, app: &mut MaterialCraftApp) {
        if let Some(card) = crate::template_card::cards().iter().find(|card| card.id == self.template && card.kind == self.kind) {
            choose(app, card);
        }
        app.ui.new_graph_name.clone_from(&self.name);
        app.ui.new_graph_size = self.width;
        app.ui.new_graph_height = self.height;
        app.ui.new_graph_linked = self.linked;
        app.ui.new_graph_package = self.package;
    }
}

impl DocumentSettings for NewGraph {
    type Preset = TemplateCard;
    type Setup = ();
    type Memory = ();

    fn name(&self) -> &str {
        &self.name
    }

    fn set_name(&mut self, name: String) {
        self.name = name;
    }

    fn page(&self) -> (f64, f64) {
        (f64::from(self.width), f64::from(self.height))
    }

    fn same_document(&self, other: &Self) -> bool {
        self == other
    }

    /// The graph can be made while the package it goes in is still open
    fn fits(&self) -> bool {
        self.package.is_none_or(|target| self.packages.iter().any(|(id, _)| *id == target))
    }

    /// The name stays the app's: an empty one is the engine's to fill
    fn open(&mut self, _setup: &(), _untitled: &str) {}

    fn preset_card(card: &TemplateCard, cx: &CardCx<'_, Self>) -> DocumentCard<Self> {
        let settings = NewGraph { kind: card.kind, template: card.id.to_owned(), ..cx.values.clone() };
        DocumentCard::new(tr(card.label), tr(card.kind_label()), settings, CardSource::preset(None)).picture(card.id).tooltip(tr(card.description))
    }

    /// `DocumentTabs::Presets` shows no Recent tab
    fn recent_cards(_cx: &CardCx<'_, Self>) -> Vec<DocumentCard<Self>> {
        Vec::new()
    }

    /// `DocumentTabs::Presets` shows no Saved tab; a card is still made for the trait
    fn saved_card(name: &str, saved: &Self, _cx: &CardCx<'_, Self>) -> DocumentCard<Self> {
        DocumentCard::new(Label::from(name.to_owned()), String::new(), saved.clone(), CardSource::Recent)
    }

    fn apply_card(&mut self, card: &DocumentCard<Self>, _setup: &(), _memory: &mut ()) {
        self.kind = card.settings.kind;
        self.template.clone_from(&card.settings.template);
    }

    /// Size (a material graph only: two sides and the link that keeps them equal), then the package the graph goes in
    fn details(ui: &mut Ui, values: &mut Self, _cx: &mut DetailsCx<'_, Self>) {
        // PLAN: the name row is the dialog's own ("Name", without production's help mark); a taken name still gets a number from the engine
        if values.kind == GraphKind::Material {
            let sides: Vec<Entry<u32>> = SIDES.iter().map(|&side| Entry::new(side, format!("{side} px"))).collect();
            Field::new(tr("Size")).layout(FieldLayout::Above).id_salt("size").show(ui, |ui, _| {
                Row::new().show(ui, |ui, _| {
                    let _ = Select::new(tr("Width"), Bind::bound(&mut values.width), &sides).sizing(Sizing::FillWidth).show(ui);
                    let _ = link_toggle(tr("Keep the width and height the same"), Bind::bound(&mut values.linked)).show(ui);
                    if values.linked {
                        values.height = values.width;
                    }
                    let _ = Select::new(tr("Height"), Bind::bound(&mut values.height), &sides).enabled(!values.linked).sizing(Sizing::FillWidth).show(ui);
                });
            });
        }

        let mut destinations = vec![Entry::new(None, tr("New package"))];
        destinations.extend(values.packages.iter().map(|(id, label)| Entry::new(Some(*id), label.clone())));
        Field::new(tr("Create graph in")).layout(FieldLayout::Above).id_salt("package").show(ui, |ui, _| {
            // A package closed while the dialog is open is no entry: the placeholder asks for another
            let _ = Select::new(tr("Create graph in"), Bind::bound(&mut values.package), &destinations)
                .placeholder(tr("Choose a package"))
                .sizing(Sizing::FillWidth)
                .show(ui);
        });
    }
}
