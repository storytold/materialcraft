use crate::UiState;
use crate::{
    appearance, commands, control, control_hooks, cook, docking, function, graph, i18n, now_ms, pattern, thumbs, toolbar, view2d, view3d, web_files, windows,
};
use materialcraft_engine::Session;
use serde_json::Value;
use std::sync::Arc;

/// One message of the outbox: `step` posts each to the window's notice center and empties the outbox.
pub struct Toast {
    /// The message.
    pub text: String,
    /// Whether it reports an error.
    pub error: bool,
}

/// How a window opens: the frame decision, where the settings live and the eframe-era file to import once.
pub struct StartOptions {
    /// Who draws the window frame; `None` when the app has no say (browser, tests).
    pub frame: Option<craft_window::policy::Decision>,
    /// Where the settings store lives.
    pub settings: craft_settings::Location,
    /// The old eframe storage file (or the browser's localStorage key), read once.
    pub legacy_settings: Option<std::path::PathBuf>,
}

// `Location` has no `Default`, so the derive the contract names cannot compile: written by hand.
impl Default for StartOptions {
    fn default() -> Self {
        StartOptions { frame: None, settings: craft_settings::Location::Auto, legacy_settings: None }
    }
}

pub struct MaterialCraftApp {
    pub session: Session,
    pub ui: UiState,
    /// The control port's requests, while the app was started with `--control`.
    pub control: Option<craft_automation::ControlChannel>,
    /// The control port's listening socket, held while the channel serves.
    #[cfg(not(target_arch = "wasm32"))]
    pub control_listener: Option<craft_automation::server::Listening>,
    /// Messages waiting for `step` to post them as notices.
    pub toasts: Vec<Toast>,
    /// Who draws the window frame.
    pub(crate) frame: Option<craft_window::policy::Decision>,
    /// The command states the runtime reads each frame.
    pub(crate) command_states: craft_tracked::Tracked<craft_ui::runtime::commands::CommandStates>,
    /// The app's commands.
    pub(crate) catalog: craft_ui::runtime::commands::AppCatalog,
    /// The user's shortcut changes over the catalog's defaults.
    pub(crate) overrides: Arc<craft_keymap::Overrides>,
    /// The menu bar the shell draws.
    pub(crate) menu: craft_tracked::Tracked<craft_menu::MenuBar>,
    /// The native menu bar (macOS).
    #[cfg(target_os = "macos")]
    pub(crate) native_menu: Option<craft_menu::NativeMenu>,
    /// The shared services, replaced as a whole on a language change.
    pub(crate) host: Arc<craft_host::Host>,
    /// The native file panels behind the Host's file service: they open on the main thread, once per pass.
    // PLAN: sh.A2 names no field for the panels `FileService::new` returns with the Host's files (sh.A1 "the Host gets files"); held here
    #[cfg(not(target_arch = "wasm32"))]
    file_panels: Option<craft_dialogs::service::FilePanels>,
    /// The open Shortcuts list.
    pub(crate) shortcuts: Option<craft_menu::shortcuts::DialogState>,
    /// The window's notices.
    pub(crate) notices: craft_ui::notices::NoticeCenter,
    /// The settings document.
    pub(crate) settings: craft_settings::SettingsStore<UiState>,
    /// The commands fired since the last frame.
    pub(crate) fired: Vec<craft_ui::runtime::commands::Fired>,
    /// Requests the control hooks leave for panels.
    pub(crate) hooks: control_hooks::Hooks,
    /// The file picks in flight and what each does.
    pub(crate) picks: std::collections::HashMap<craft_host::PickId, crate::native_dialogs::Then>,
    /// The unsaved-changes prompt, while it is open.
    pub(crate) unsaved: Option<crate::unsaved::Pending>,
    /// Autosave of a modified package.
    pub(crate) autosave: crate::autosave::State,
    /// Background cooking of the open graph
    pub cooker: cook::Cooker,
    /// Node output thumbnails and library previews
    pub thumbs: thumbs::Thumbs,
    /// The graph editor's gesture state
    pub graph: graph::GraphView,
    pub view3d: view3d::View3dState,
    pub view2d: view2d::View2dState,
    /// Embedded graph editors in the main graph panel
    pub pattern_editor: Option<pattern::PatternEditor>,
    pub function_editor: Option<function::FunctionEditor>,
    pub(crate) inspected_graph: Option<materialcraft_engine::GraphTarget>,
    pub(crate) inspected_folder: Option<(u64, String)>,
    pub(crate) expose_parameter_dialog: Option<crate::properties::ExposeParameterDialog>,
    pub(crate) graph_parents: Vec<graph::graph_parent::GraphParent>,
    pub fps: f32,
    /// Smoothed CPU time spent building the UI per frame (ms)
    pub ui_ms: f32,
    /// The last status message (bottom left)
    pub status: String,
    pub(crate) pending_load: Option<(String, Vec<u8>)>,
    pub(crate) quit_confirmed: bool,
    last_time: f64,
    /// Release checks and the update prompt (desktop only)
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) updates_rt: crate::updates_ui::Runtime,
    /// The material export jobs, queued through the host (desktop only)
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)] // P3(panels-w6): pn.A30
    pub(crate) material_export: crate::export::PublishJobs,
    /// The library's entries for the open session, rebuilt when its packages change
    pub(crate) library_entries: crate::library::EntriesCache,
    /// Whether a node's width and height are linked, per graph and node
    #[allow(dead_code)] // P3(panels-w9): pn.A30
    pub(crate) size_links: std::collections::HashMap<(usize, materialcraft_engine::core::NodeId), bool>,
    /// The name typed for a new preset
    #[allow(dead_code)] // P3(panels-w10): pn.A30
    pub(crate) preset_name: String,
    /// The Function JSON being edited: the node, the text and the text last applied
    #[allow(dead_code)] // P3(panels-w10): pn.A30
    pub(crate) function_json: Option<(materialcraft_engine::core::NodeId, String, String)>,
    /// The Levels editor's histograms, per cook result and output port
    // PLAN: pn.A30 lists panels-w6..w10 as readers, and no sheet names the one that reads `levels_histograms`
    #[allow(dead_code)]
    pub(crate) levels_histograms: std::collections::HashMap<(u64, usize), Arc<craft_imaging::ui::histogram::HistogramData>>,
    // owed by panels-w10: the property text drafts its files still keep, until its commits replace them (pn.A30).
    pub(crate) folder_property_draft: Option<crate::properties::FolderPropertyDraft>,
    pub(crate) graph_property_draft: Option<crate::properties::GraphPropertyDraft>,
    /// What Copy or Cut put on the clipboard this frame, for the system clipboard
    pub(crate) system_clipboard: Option<String>,
    /// The version this build names itself with: the Launcher's feed identity for a Nightly.
    pub build_version: String,
    /// The commit this build was made from, empty when the build could not tell.
    pub build_commit: String,
    /// The app binary's last step as the app leaves, run once at the end of `on_exit`. On macOS
    /// every quit ends the process inside AppKit's terminate, after `on_exit`, so code after
    /// `eframe::run_native` never runs there.
    pub at_exit: Option<Box<dyn FnOnce()>>,
}

impl MaterialCraftApp {
    /// Installs the framework runtime on `ctx`: the app's root environment, fonts and window frame. Returns the
    /// installed font stack.
    pub fn install_runtime(ctx: &egui::Context, frame: Option<craft_window::policy::Decision>) -> Result<craft_ui::fonts::Stack, craft_ui::InstallError> {
        let shell = craft_host::target::ShellInfo::default().backend(craft_host::target::Backend::from_env());
        // Runtime defaults are the production wheel modifiers and eframe clipboard; repeats are set in root_env.
        let mut spec = craft_ui::InstallSpec::new(shell, Arc::new(craft_window::system::SystemWindows)).app_id(crate::os_window::app_id());
        if let Some(decision) = frame {
            spec = spec.frame(decision);
        }
        craft_ui::install(ctx, spec)?;
        let fonts = appearance::ui_font_stack();
        fonts.install(ctx);
        Ok(fonts)
    }

    /// The app in an eframe window: installs the runtime with `options.frame`, keeps the decision, and opens the
    /// settings store at `options.settings`, importing `options.legacy_settings` once.
    pub fn new(cc: &eframe::CreationContext<'_>, session: Session, options: StartOptions) -> MaterialCraftApp {
        let installed = Self::install_runtime(&cc.egui_ctx, options.frame.clone());
        let legacy = options.legacy_settings.into_iter().map(crate::settings::eframe_legacy).collect();
        let (settings, report) = craft_settings::SettingsStore::open_with(craft_settings::OpenOptions::new(options.settings).legacy(legacy));
        log::info!("settings loaded from {:?}", report.source);
        let mut app = MaterialCraftApp::headless(session, settings.get().clone());
        app.settings = settings;
        app.frame = options.frame;
        app.ui.windows.home = app.ui.prefs.show_home;
        if let Err(error) = installed {
            app.toast(crate::i18n::trf("The interface could not start completely: {error}", &[("error", &error)]), true);
        }
        crate::app_icons::register();

        let host = Self::host_builder().jobs(craft_host::default_jobs());
        #[cfg(not(target_arch = "wasm32"))]
        let host = {
            let ctx = cc.egui_ctx.clone();
            let (files, panels) = craft_dialogs::service::FileService::new(craft_host::Wake::new(move || ctx.request_repaint()));
            app.file_panels = Some(panels);
            app.thumbs.enable_persistence(app.ui.prefs.library_cache);
            host.files(Arc::new(files))
        };
        app.host = Arc::new(host.build());

        #[cfg(target_os = "macos")]
        {
            app.native_menu = craft_menu::mac::install(&cc.egui_ctx, &app.menu);
        }
        app
    }

    /// The Host's services every app shares: the app's identity and the translation catalog in the current language.
    fn host_builder() -> craft_host::HostBuilder {
        craft_host::Host::builder(craft_host::AppInfo { id: "materialcraft", name: "MaterialCraft", version: materialcraft_engine::VERSION })
            .i18n(i18n::host_i18n())
            .lang(i18n::lang())
    }

    /// An app without eframe (offscreen snapshots and tests): no runtime and no `Context`; jobs run inline and the
    /// settings live in memory. A test installs the runtime on its own context with [`Self::install_runtime`].
    pub fn headless(session: Session, mut ui: UiState) -> MaterialCraftApp {
        ui.windows = crate::state::Windows::default();
        ui.frame_all = true;
        i18n::set_language(&ui.prefs.language);
        let catalog = commands::catalog(craft_keymap::Platform::current(), &session.library, session.library_generation());
        let mut app = MaterialCraftApp {
            session,
            ui,
            control: None,
            #[cfg(not(target_arch = "wasm32"))]
            control_listener: None,
            toasts: Vec::new(),
            frame: None,
            command_states: Default::default(),
            catalog,
            overrides: Default::default(),
            menu: Default::default(),
            #[cfg(target_os = "macos")]
            native_menu: None,
            host: Arc::new(Self::host_builder().jobs(Arc::new(craft_host::InlineJobs)).build()),
            #[cfg(not(target_arch = "wasm32"))]
            file_panels: None,
            shortcuts: None,
            notices: craft_ui::notices::NoticeCenter::new(),
            settings: craft_settings::SettingsStore::open(craft_settings::Location::Memory).0,
            fired: Vec::new(),
            hooks: Default::default(),
            picks: Default::default(),
            unsaved: None,
            autosave: Default::default(),
            cooker: cook::Cooker::default(),
            thumbs: thumbs::Thumbs::default(),
            graph: graph::GraphView::default(),
            view3d: view3d::View3dState::default(),
            view2d: view2d::View2dState::default(),
            function_editor: None,
            inspected_graph: None,
            inspected_folder: None,
            expose_parameter_dialog: None,
            graph_parents: Vec::new(),
            pattern_editor: None,
            fps: 0.0,
            ui_ms: 0.0,
            status: String::new(),
            pending_load: None,
            quit_confirmed: false,
            last_time: 0.0,
            #[cfg(not(target_arch = "wasm32"))]
            updates_rt: crate::updates_ui::Runtime::default(),
            #[cfg(not(target_arch = "wasm32"))]
            material_export: Default::default(),
            library_entries: Default::default(),
            size_links: Default::default(),
            preset_name: String::new(),
            function_json: None,
            levels_histograms: Default::default(),
            folder_property_draft: None,
            graph_property_draft: None,
            system_clipboard: None,
            build_version: materialcraft_engine::VERSION.to_owned(),
            build_commit: String::new(),
            at_exit: None,
        };
        let mut menu = std::mem::take(&mut app.menu);
        commands::menu_bar(&app, crate::shell::modality::Modality::Free, &mut menu);
        app.menu = menu;
        app.apply_prefs();

        // A session that has not touched its 3D view starts from the saved default
        if let Some(view) = &app.session.default_view3d
            && app.session.view3d == materialcraft_engine::View3d::default()
        {
            app.session.view3d = view.clone();
        }
        app
    }

    /// Push application preferences into the session
    pub fn apply_prefs(&mut self) {
        self.ui.prefs.library_cache = self.ui.prefs.library_cache.sanitized();
        let library_cache = self.ui.prefs.library_cache;
        if self.session.library_cache_prefs != library_cache {
            let _ = materialcraft_engine::execute(&mut self.session, "library.cache.configure", &serde_json::json!(library_cache));
        }

        self.ui.prefs.history = self.ui.prefs.history.sanitized();
        let history = self.ui.prefs.history;
        if self.session.history_prefs != history {
            let _ = materialcraft_engine::execute(&mut self.session, "session.history", &serde_json::json!(history));
        }

        self.ui.recent.truncate(self.ui.prefs.recent_limit.clamp(1, crate::state::Prefs::MAX_RECENT));
        self.session.view_outputs_on_open = self.ui.prefs.view_outputs_on_open;

        let camera = self.ui.prefs.camera.sanitized();
        if self.session.camera != camera {
            self.session.set_camera_prefs(camera);
        }

        // Bound saved camera values before applying them
        let default_view = self.ui.prefs.default_view3d.clone().map(|view| {
            let mut view = view.sanitized();
            view.target = self.session.camera.clamp_target(view.target);
            view
        });
        if self.session.default_view3d != default_view {
            self.session.default_view3d = default_view;
        }

        let preview_size = match self.ui.prefs.preview_size {
            0 => None,
            size => Some(size.clamp(64, 8192)),
        };
        if self.session.preview_size != preview_size {
            let _ = materialcraft_engine::execute(&mut self.session, "view.previewSize", &serde_json::json!({"size": preview_size.unwrap_or(0)}));
        }

        // A new `Arc` rebuilds the runtime's keymap, so only a change replaces it.
        if *self.overrides != self.ui.shortcut_overrides {
            self.overrides = Arc::new(self.ui.shortcut_overrides.clone());
        }
    }

    /// Run one command with a displayed snapshot and release the handoff before returning
    pub(crate) fn run_with_display_snapshot(
        &mut self,
        id: &str,
        params: Value,
        snapshot: materialcraft_engine::DisplaySnapshot,
    ) -> (Option<Value>, Option<materialcraft_engine::DisplaySnapshot>) {
        self.session.begin_display_handoff(snapshot);
        let result = self.run(id, params);
        let output = self.session.finish_display_handoff();
        (result, output)
    }

    /// Run an engine command, reporting errors as toasts. Returns the result on success
    pub fn run(&mut self, id: &str, mut params: Value) -> Option<Value> {
        if id.starts_with("graph.annotation.")
            || id.starts_with("graph.frame.")
            || id.starts_with("graph.comment.")
            || id.starts_with("graph.pin.")
            || id == "library.add"
        {
            if params.is_null() {
                params = serde_json::json!({});
            }
            if let (Some(parameters), Some(target)) = (params.as_object_mut(), self.graph.annotation_target.parameters().as_object()) {
                for (name, value) in target {
                    parameters.entry(name).or_insert_with(|| value.clone());
                }
            }
        }

        if matches!(id, "edit.undo" | "edit.redo") {
            if params.is_null() {
                params = serde_json::json!({});
            }
            if let Some(parameters) = params.as_object_mut() {
                parameters.entry("document").or_insert_with(|| Value::from(crate::history::document(self)));
            }
        }

        if matches!(
            id,
            "graph.node.add"
                | "graph.instance.add"
                | "graph.node.move"
                | "graph.frame.move"
                | "graph.wire.insertDot"
                | "edit.paste"
                | "edit.pasteWithoutLinks"
                | "edit.duplicate"
                | "edit.duplicateWithoutLinks"
        ) {
            if params.is_null() {
                params = serde_json::json!({});
            }

            if let Some(parameters) = params.as_object_mut() {
                parameters.entry("snapToGrid").or_insert(Value::Bool(self.ui.prefs.snap_to_grid));
            }
        }

        match cook::execute_guarded(&mut self.session, id, &params) {
            Ok(v) => {
                // The desktop sends no Paste event while the system clipboard is empty, so Copy and Cut fill it
                if matches!(id, "edit.copy" | "edit.cut")
                    && let Some(clip) = v.get("clip")
                {
                    self.system_clipboard = Some(clip.to_string());
                }

                if matches!(id, "graph.pin.add" | "library.add")
                    && let Some(materialcraft_engine::AnnotationSelection::Pin(pin)) = self.session.annotation_selection
                {
                    self.graph.pin_description_focus = Some(pin);
                    self.inspected_folder = None;
                    self.inspected_graph = None;
                    self.ui.dock.focus(docking::Panel::Properties);
                }

                self.sync_preferences_after_command(id);
                self.close_subgraph_after(id);
                if id == "graph.node.function.edit"
                    && let Ok(target) = serde_json::from_value::<materialcraft_engine::FunctionGraphTarget>(v["target"].clone())
                {
                    let target = match target {
                        materialcraft_engine::FunctionGraphTarget::Base { node, parameter } => Some((node, function::FunctionTarget::Base(parameter))),
                        materialcraft_engine::FunctionGraphTarget::Dynamic { node, parameter } => Some((node, function::FunctionTarget::Dynamic(parameter))),
                        _ => None,
                    };
                    if let Some((node, target)) = target {
                        crate::graph::subgraph_navigation::open_function(self, node, target);
                    }
                }

                if matches!(id, "view.3d.assign" | "view.3d.outputs") {
                    self.ui.dock.focus(docking::Panel::View3d);
                }

                Some(v)
            }
            Err(e) => {
                self.toast(e.to_string(), true);
                None
            }
        }
    }

    /// Commands that change a preference write it to the preferences, which the settings store saves
    pub(crate) fn sync_preferences_after_command(&mut self, id: &str) {
        let prefs = &mut self.ui.prefs;
        match id {
            "view.previewSize" => prefs.preview_size = self.session.preview_size.unwrap_or(0),
            "view.3d.saveDefault" => prefs.default_view3d.clone_from(&self.session.default_view3d),
            "library.cache.configure" => prefs.library_cache = self.session.library_cache_prefs,
            "session.history" => prefs.history = self.session.history_prefs,
            _ => {}
        }
    }

    pub(crate) fn close_subgraph_after(&mut self, command: &str) {
        if command.starts_with("file.new")
            || command.starts_with("package.")
            || matches!(command, "file.open" | "file.reload" | "graph.open" | "graph.delete" | "graph.duplicate" | "folder.remove" | "explorer.paste")
        {
            self.graph.inspected_node = None;
            self.inspected_graph = None;
            self.inspected_folder = None;
            self.graph_parents.clear();
            self.pattern_editor = None;
            self.function_editor = None;
            self.graph.search = None;
            self.graph.selected_wires = Default::default();
            self.graph.cancel_gesture();
        }
        self.reconcile_editors();
        if (materialcraft_engine::selection_commands::supports(command) || matches!(command, "edit.undo" | "edit.redo"))
            && let Some(editor) = &mut self.function_editor
            && matches!(editor.target, function::FunctionTarget::Graph)
        {
            editor.selection = self.session.selection.iter().filter_map(|id| u32::try_from(id.0).ok()).collect();
        }
    }

    pub(crate) fn reconcile_editors(&mut self) {
        if self
            .inspected_folder
            .as_ref()
            .is_some_and(|(document, path)| !self.session.package(*document).is_ok_and(|package| package.folder_paths().contains(path)))
        {
            self.inspected_folder = None;
        }
        if self.inspected_graph.is_some_and(|target| target.get(&self.session).is_err()) {
            self.inspected_graph = None;
        }
        if self.function_editor.as_ref().is_some_and(|editor| editor.current(self).is_none()) {
            self.function_editor = None;
        }
        if self.pattern_editor.as_ref().is_some_and(|editor| editor.current(self).is_none()) {
            self.pattern_editor = None;
        }
        crate::view3d::clear_stale_inspection(self);
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        let text = text.into();
        self.session.log(text.clone());
        self.toast_only(text, error);
    }

    pub(crate) fn toast_only(&mut self, text: String, error: bool) {
        self.status = text.clone();
        self.toasts.push(Toast { text, error });
    }
}

impl eframe::App for MaterialCraftApp {
    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(if cfg!(target_arch = "wasm32") { 5 } else { 30 })
    }

    /// eframe's periodic and final save: the settings document takes the app's `UiState`. The window geometry stays
    /// eframe's own.
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        if self.settings.get() != &self.ui {
            self.settings.update(|saved| saved.clone_from(&self.ui));
        }
        if let Err(error) = self.settings.flush() {
            log::warn!("could not save the settings: {error}");
        }
    }

    /// The one exit path macOS has: its quit ends the process before `run_native` returns.
    fn on_exit(&mut self) {
        if let Some(at_exit) = self.at_exit.take() {
            at_exit();
        }
    }

    /// Once per frame, before its passes: hands the runtime the app's root env, command states, overrides, catalog
    /// and Host, and takes the commands fired since the last frame for `step` to run.
    fn raw_input_hook(&mut self, ctx: &egui::Context, _raw: &mut egui::RawInput) {
        craft_ui::shell::embed::before_input(ctx, |output| {
            self.fired.extend(output.fired);
            let mut states = std::mem::take(&mut self.command_states);
            commands::states(self, ctx, &mut states);
            self.command_states = states;
            let mut input = craft_ui::runtime::frame::FrameInput::new(appearance::root_env(&self.ui.prefs));
            input.commands =
                craft_ui::runtime::commands::CommandInput::new().states(&self.command_states).overrides(self.overrides.clone()).catalog(self.catalog.clone());
            input.host = Some(self.host.clone());
            input
        });
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.step(ctx);
        craft_ui::shell::embed::logic(ctx, || crate::unsaved::close_requested(self));
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        #[cfg(not(target_arch = "wasm32"))]
        craft_ui::shell::embed::before_ui(ui.ctx(), frame);
        self.draw(ui);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(panels) = &mut self.file_panels {
            craft_ui::shell::WindowDialogs::of(ui.ctx(), &craft_ui::runtime::env::WindowKey::main()).with(|dialogs| panels.pump_with(dialogs, Some(&*frame)));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = frame;
    }
}

impl MaterialCraftApp {
    /// Per-frame logic before drawing, once per frame (eframe's `logic`, never a second pass): the control channel,
    /// picks and jobs, cooking, autosave, the fired commands, the clipboard and the notices.
    pub fn step(&mut self, ctx: &egui::Context) {
        i18n::set_language(&self.ui.prefs.language);
        let lang = i18n::lang();
        if self.host.lang() != lang {
            self.host = Arc::new(self.host.with_lang(lang));
        }
        let now = ctx.input(|i| i.time);
        let dt = (now - self.last_time) as f32;
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
        }
        self.last_time = now;

        control::poll(self, ctx);
        control_hooks::step(self, ctx);
        crate::native_dialogs::poll(self);
        #[cfg(not(target_arch = "wasm32"))]
        crate::export::poll(self);

        cook::drive(self, ctx);
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::autosave::tick(self, std::time::Instant::now());
            if let Some(due) = self.autosave.deadline() {
                ctx.request_repaint_after(due.saturating_duration_since(std::time::Instant::now()));
            }
            crate::updates_ui::tick(self, ctx);
        }
        self.thumbs.configure_cache(ctx, self.ui.prefs.library_cache, self.session.library_cache_clear_epoch);
        let busy = self.cooker.busy() || self.session.dirty;
        if let Some(error) = self.thumbs.frame(ctx, busy) {
            self.toast(error, true);
        }

        // The catalog's generation follows the library's (sh.A13), so a new node list rebuilds the node commands.
        if self.catalog.generation() != self.session.library_generation() {
            self.catalog = commands::catalog(craft_keymap::Platform::current(), &self.session.library, self.session.library_generation());
        }
        let fired = std::mem::take(&mut self.fired);
        commands::run_fired(self, ctx, &fired);
        #[cfg(target_os = "macos")]
        if let Some(mut native) = self.native_menu.take() {
            native.poll(ctx);
            for command in native.take_clicks(ctx) {
                commands::run(self, ctx, &command);
            }
            self.native_menu = Some(native);
        }
        let mut menu = std::mem::take(&mut self.menu);
        commands::menu_bar(self, crate::shell::modality::Modality::capture(self, ctx), &mut menu);
        self.menu = menu;

        self.reconcile_editors();
        self.publish_clipboard(ctx);
        for toast in std::mem::take(&mut self.toasts) {
            let tone = if toast.error { craft_theme::variant::Tone::Bad } else { craft_theme::variant::Tone::Neutral };
            self.notices.post(ctx, &craft_ui::notices::PostedNotice::new(toast.text).tone(tone), &[]);
        }

        // The close goes through `embed::logic`, where `unsaved::close_requested` lets it pass once the quit is confirmed.
        if self.quit_confirmed && !ctx.input(|input| input.viewport().close_requested()) {
            craft_ui::runtime::windows::WindowHost::of(ctx).act(egui::ViewportId::ROOT, craft_ui::runtime::windows::WindowAction::Close);
        }
    }

    /// Hand what Copy or Cut put on the clipboard to the system clipboard. Runs in `step` only: a measuring pass
    /// writes nothing, so an offer from a pass could be lost.
    fn publish_clipboard(&mut self, ctx: &egui::Context) {
        if let Some(text) = self.system_clipboard.take() {
            let item = craft_ui::runtime::commands::ClipItem::Text(text.into());
            if let Err(error) = craft_ui::runtime::commands::ClipboardHandle::of(ctx).offer(&[item]) {
                log::warn!("could not copy to the system clipboard: {error:?}");
            }
        }
    }

    /// Draw the whole window into `ui`, once per pass: the title bar with the menus, the toolbar, the dock, the status
    /// bar and the app's windows, under the drop veil. Files dropped in this pass are opened in this pass.
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        eprintln!("DIAG draw: start");
        crate::annotation::activate(self, crate::annotation::target(self));
        eprintln!("DIAG draw: activated");
        let t0 = now_ms();
        let show_toolbar = self.ui.prefs.show_toolbar;
        let veil = craft_ui::popover::DropVeil::new(i18n::tr("Drop files to open")).show_with_files(ui, |ui, _| {
            eprintln!("DIAG draw: body start {:?}", ui.max_rect());
            craft_ui::shell::AppShell::new(craft_ui::runtime::env::WindowKey::main())
                .title_bar(|ui, _, app: &mut MaterialCraftApp| {
                    eprintln!("DIAG draw: title {:?}", ui.max_rect());
                    title_bar(app, ui);
                    eprintln!("DIAG draw: title done");
                })
                .options_bar(show_toolbar, |ui, cx, app: &mut MaterialCraftApp| {
                    eprintln!("DIAG draw: toolbar {:?}", ui.max_rect());
                    toolbar::main_toolbar(app, ui, cx);
                    eprintln!("DIAG draw: toolbar done");
                })
                .status(true, |ui, cx, app: &mut MaterialCraftApp| {
                    eprintln!("DIAG draw: status {:?}", ui.max_rect());
                    toolbar::status_bar(app, ui, cx);
                    eprintln!("DIAG draw: status done");
                })
                .body(|ui, _, app: &mut MaterialCraftApp| {
                    eprintln!("DIAG draw: dock {:?}", ui.max_rect());
                    docking::show(app, ui);
                    eprintln!("DIAG draw: dock done");
                })
                .show(ui, &mut *self);
            eprintln!("DIAG draw: shell shown");
            windows::show(self, ui);
            eprintln!("DIAG draw: windows shown");
        });
        eprintln!("DIAG draw: veil done");
        if let Some(files) = veil.output.dropped
            && !crate::shell::modality::Modality::capture(self, ui.ctx()).is_blocked()
        {
            web_files::dropped(self, &files);
        }
        control::ui_pass(self, ui.ctx());
        control_hooks::after_draw(self);
        eprintln!("DIAG draw: end");
        let dt = (now_ms() - t0) as f32;
        self.ui_ms = if self.ui_ms == 0.0 { dt } else { self.ui_ms * 0.9 + dt * 0.1 };
    }
}

/// The title bar: the menus at its start; the window buttons where the app draws the frame. Its Close asks the root
/// window to close, so the unsaved-changes check runs as for the OS close button.
fn title_bar(app: &mut MaterialCraftApp, ui: &mut egui::Ui) {
    let menu = &app.menu;
    let output = craft_ui::shell::TitleBar::new().show_fitted(
        ui,
        |ui, _| {
            craft_menu::MenuBarUi::new(menu).show(ui);
        },
        |_, _| {},
    );
    if output.close_requested {
        craft_ui::runtime::windows::WindowHost::of(ui.ctx()).act(egui::ViewportId::ROOT, craft_ui::runtime::windows::WindowAction::Close);
    }
}

#[cfg(test)]
#[path = "material_craft_app.exit_hook_test.rs"]
mod exit_hook_tests;

#[cfg(test)]
#[path = "material_craft_app.test.rs"]
mod tests;
