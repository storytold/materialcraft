//! The eframe web runner: canvas lookup, backend selection, the loading screen's startup steps,
//! browser shortcuts the app takes over, and startup errors shown in the page.

use materialcraft_ui_egui::MaterialCraftApp;
use wasm_bindgen::JsCast as _;
use wasm_bindgen::closure::Closure;

const CANVAS_ID: &str = "materialcraft_canvas";
const LOADING_ID: &str = "materialcraft_loading";
const STATUS_ID: &str = "materialcraft_status";
const PROGRESS_ID: &str = "materialcraft_progress";
/// Where `index.html` copies the SBS library's `resources/`, relative to the page
const LIBRARY_URL: &str = "sbs";
/// How long startup waits for the library files; a response that stalls would otherwise keep the
/// loading screen up for good. Two minutes fits the 14 MB of bitmaps on a slow connection
const LIBRARY_DOWNLOAD_TIMEOUT_MS: i32 = 120_000;

pub fn start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    capture_app_shortcuts();
    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            log::error!("no document");
            return;
        };

        let result = match document.get_element_by_id(CANVAS_ID).and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok()) {
            Some(canvas) => run(canvas).await,
            None => Err(format!("the page has no <canvas id=\"{CANVAS_ID}\">")),
        };
        if let Some(el) = document.get_element_by_id(LOADING_ID) {
            match result {
                Ok(()) => el.remove(),
                Err(e) => {
                    log::error!("MaterialCraft failed to start: {e}");
                    el.set_text_content(Some(&format!(
                        "MaterialCraft failed to start: {e}. It needs a browser with WebGPU (current Chrome, Edge or Safari) or at least WebGL2."
                    )));
                }
            }
        }
    });
}

async fn run(canvas: web_sys::HtmlCanvasElement) -> Result<(), String> {
    // The WebAssembly has downloaded and compiled when this runs
    step(0.3, "Downloading the material library…").await;
    download_library().await;
    step(0.5, "Preparing a new package…").await;
    let session = materialcraft_engine::new_session();
    step(0.8, "Starting the renderer…").await;
    let mut options = eframe::WebOptions::default();
    if query().contains("webgl")
        && let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup
    {
        create.instance_descriptor.backends = eframe::wgpu::Backends::GL;
    }
    eframe::WebRunner::new()
        .start(
            canvas,
            options,
            Box::new(|cc| {
                materialcraft_ui_egui::framework::install_headless(&cc.egui_ctx);
                let mut app = MaterialCraftApp::new(cc, session);

                // One thread cooks in the browser, so keep previews small
                app.ui.prefs.preview_size = app.ui.prefs.preview_size.clamp(128, 256);
                Ok(Box::new(app))
            }),
        )
        .await
        .map_err(|e| e.as_string().unwrap_or_else(|| format!("{e:?}")))
}

/// Fetch the SBS library files that ship beside the wasm (`index.html` copies them to
/// [`LIBRARY_URL`]) before the first cook needs one. A file that fails to download, or is still
/// downloading after [`LIBRARY_DOWNLOAD_TIMEOUT_MS`], is logged and reported by the nodes that use
/// it; it doesn't stop startup.
async fn download_library() {
    use materialcraft_formats::sbs::library;
    let Some(window) = web_sys::window() else { return };

    // One deadline covers every request and body; aborting rejects whatever is still pending
    let controller = web_sys::AbortController::new().ok();
    let init = web_sys::RequestInit::new();
    init.set_signal(controller.as_ref().map(|c| c.signal()).as_ref());
    let abort = controller.map(|c| Closure::<dyn FnMut()>::new(move || c.abort()));
    let timer = abort
        .as_ref()
        .and_then(|abort| window.set_timeout_with_callback_and_timeout_and_arguments_0(abort.as_ref().unchecked_ref(), LIBRARY_DOWNLOAD_TIMEOUT_MS).ok());

    // Start every fetch before awaiting its response
    let requests: Vec<_> = library::external_assets()
        .iter()
        .map(|relative| {
            let url = String::from(js_sys::encode_uri(&format!("{LIBRARY_URL}/{relative}")));
            (*relative, wasm_bindgen_futures::JsFuture::from(window.fetch_with_str_and_init(&url, &init)))
        })
        .collect();
    for (relative, request) in requests {
        if let Err(e) = fetch_bytes(request).await.and_then(|bytes| library::install(relative, bytes)) {
            log::warn!("couldn't download sbs://{relative}: {e}");
        }
    }

    // The closure must outlive the timer, so cancel the timer before dropping it
    if let Some(timer) = timer {
        window.clear_timeout_with_handle(timer);
    }
    drop(abort);
}

async fn fetch_bytes(request: wasm_bindgen_futures::JsFuture) -> Result<Vec<u8>, String> {
    let response: web_sys::Response = request.await.and_then(|r| r.dyn_into()).map_err(|e| format!("{e:?}"))?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }

    let buffer = response.array_buffer().map_err(|e| format!("{e:?}"))?;
    let buffer = wasm_bindgen_futures::JsFuture::from(buffer).await.map_err(|e| format!("{e:?}"))?;

    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

/// Show a startup step on the loading screen and let the browser paint it before the next
/// (blocking) step runs.
async fn step(progress: f32, status: &str) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
    if let Some(el) = document.get_element_by_id(STATUS_ID) {
        el.set_text_content(Some(status));
    }
    if let Some(el) = document.get_element_by_id(PROGRESS_ID) {
        let _ = el.set_attribute("value", &progress.to_string());
    }

    // A timer lets background tabs resume even without animation frames
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 20);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// The app's shortcuts (Ctrl+S, Ctrl+O, Ctrl+D, F5…) would also trigger the browser's own
/// actions; take them first. Copy, cut and paste stay with the browser, which turns them into
/// the clipboard events egui reads. Shortcuts the browser reserves (Ctrl+N, Ctrl+T, Ctrl+W)
/// never reach the page.
fn capture_app_shortcuts() {
    let Some(window) = web_sys::window() else { return };
    let on_key = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(|e: web_sys::KeyboardEvent| {
        let ctrl = e.ctrl_key() || e.meta_key();
        let k = e.key().to_ascii_lowercase();
        let taken = (ctrl && ["s", "o", "d", "a", "z", "y", "e"].contains(&k.as_str())) || k == "f5" || (k == " " && !ctrl);
        if taken {
            e.prevent_default();
        }
    });
    let options = web_sys::AddEventListenerOptions::new();
    options.set_capture(true);
    if let Err(e) = window.add_event_listener_with_callback_and_add_event_listener_options("keydown", on_key.as_ref().unchecked_ref(), &options) {
        log::warn!("couldn't take over the browser's shortcuts: {e:?}");
    }

    // The listener lives as long as the page
    on_key.forget();
}

fn query() -> String {
    web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default()
}
