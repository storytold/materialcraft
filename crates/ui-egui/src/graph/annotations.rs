//! The annotations of the active editor: backdrops, notes and markers on the framework node graph.

use craft_behavior::EditEvent;
use craft_behavior::canvas::CanvasView;
use craft_behavior::kurbo::Point;
use craft_ui::graph::{AnnotationEvent, AnnotationKey, Backdrop, Marker, Note};
use craft_ui::style::DataColor;
use egui::{Color32, Pos2, Rect, Vec2, pos2, vec2};
use materialcraft_engine::core::{CanvasAnnotations, Comment, Frame, Pin, linear_to_srgb};
use materialcraft_engine::graph_grid::snap_position;
use materialcraft_engine::{AnnotationSelection, FrameContents};
use serde_json::json;

use crate::MaterialCraftApp;

/// The least size a frame resize leaves, in graph units.
const FRAME_MIN: Vec2 = vec2(10.0, 10.0);
/// The height a note covers before its text is laid out, in graph units, so a comment with no text still takes a press.
const NOTE_HEIGHT: f32 = 16.0;
/// The side of a pin in graph units, for the published rects `ui.inspect` and scripted clicks read; its bottom middle
/// is the pin's tip, as the framework's marker.
// shortcut: the framework sizes the pin by its icon token and adds the label, which this cannot measure; publish the
// framework's marker rect once the node graph reports its parts' rects.
const PIN_SIDE: f32 = 18.0;
/// A move or resize shorter than this, in graph units, commits nothing.
const SLOP: f32 = 0.5;

/// The owned annotation items of the active editor and the selected key, each already placed where this pass shows
/// it: shifted by a move preview, resized by a resize preview.
pub(crate) struct AnnotationLayers {
    /// Largest first, so a small frame paints over a large one and takes a press inside both.
    frames: Vec<Frame>,
    comments: Vec<Comment>,
    pins: Vec<Pin>,
    /// The contents of the frame being moved and the snapped offset they show at.
    carried: Option<(FrameContents, Vec2)>,
    /// The selected annotation, at most one.
    pub(crate) selected: Option<AnnotationKey>,
}

impl AnnotationLayers {
    /// The frames as the node graph's backdrops, in paint order.
    pub(crate) fn backdrops(&self) -> Vec<Backdrop<'_>> {
        self.frames
            .iter()
            .map(|frame| {
                let backdrop = Backdrop::new(frame.id, frame_rect(frame), fill(frame.color)).description(&frame.description);
                if frame.title_visible { backdrop.title(&frame.title) } else { backdrop }
            })
            .collect()
    }

    /// The comments as the node graph's notes; each wraps to its comment's width.
    pub(crate) fn notes(&self) -> Vec<Note<'_>> {
        self.comments.iter().map(|comment| Note::new(comment.id, note_rect(comment), &comment.text)).collect()
    }

    /// The pins as the node graph's markers.
    pub(crate) fn markers(&self) -> Vec<Marker<'_>> {
        self.pins.iter().map(|pin| Marker::new(pin.id, pos2(pin.pos[0], pin.pos[1]), &pin.name)).collect()
    }

    /// The selected annotation, at most one.
    pub(crate) fn selected(&self) -> Option<AnnotationKey> {
        self.selected
    }

    /// The contents of the frame being moved and its snapped offset, for the hosts to translate their own nodes by.
    pub(crate) fn carried(&self) -> Option<(&FrameContents, Vec2)> {
        self.carried.as_ref().map(|(contents, by)| (contents, *by))
    }
}

/// The backdrops, notes and markers of the active editor's annotation target.
///
/// A host builds this before it shows the node graph and calls [`apply`] after, in the same pass: a resize reads the
/// rect this handed the framework (see `resized`).
pub(crate) fn layers(app: &MaterialCraftApp) -> AnnotationLayers {
    let target = &app.graph.annotation_target;
    let CanvasAnnotations { mut frames, mut comments, mut pins } = target.items(&app.session).unwrap_or_default();
    // Sorted on the stored rects, so a resize preview does not restack the frames under the pointer.
    frames.sort_by(|first, second| (second.rect[2] * second.rect[3]).total_cmp(&(first.rect[2] * first.rect[3])));

    // The contents are captured once a pass and never stored; the offset is snapped as `graph.frame.move` snaps it.
    let carried = match app.graph.moving {
        Some((AnnotationKey::Backdrop(id), by)) => frames
            .iter()
            .find(|frame| frame.id == id)
            .and_then(|frame| target.capture_frame(&app.session, frame).ok())
            .map(|contents| (contents, Vec2::from(snap_position([by.x, by.y], app.ui.prefs.snap_to_grid)))),
        _ => None,
    };
    let offset = |key: AnnotationKey| {
        if let Some((contents, by)) = &carried {
            let inside = match key {
                AnnotationKey::Backdrop(id) => contents.frames.contains(&id),
                AnnotationKey::Note(id) => contents.comments.contains(&id),
                AnnotationKey::Marker(id) => contents.pins.contains(&id),
                _ => false,
            };
            return if inside { *by } else { Vec2::ZERO };
        }
        // A comment or pin moves on its own, unsnapped, as its commit is.
        match app.graph.moving {
            Some((moved, by)) if moved == key => by,
            _ => Vec2::ZERO,
        }
    };

    for frame in &mut frames {
        let by = offset(AnnotationKey::Backdrop(frame.id));
        frame.rect[0] += by.x;
        frame.rect[1] += by.y;
        if let Some((id, rect)) = app.graph.resizing
            && id == frame.id
        {
            frame.rect = [rect.min.x, rect.min.y, rect.width(), rect.height()];
        }
    }
    for comment in &mut comments {
        let by = offset(AnnotationKey::Note(comment.id));
        comment.pos = [comment.pos[0] + by.x, comment.pos[1] + by.y];
    }
    for pin in &mut pins {
        let by = offset(AnnotationKey::Marker(pin.id));
        pin.pos = [pin.pos[0] + by.x, pin.pos[1] + by.y];
    }

    let selected = app.session.annotation_selection.map(|selection| match selection {
        AnnotationSelection::Frame(id) => AnnotationKey::Backdrop(id),
        AnnotationSelection::Comment(id) => AnnotationKey::Note(id),
        AnnotationSelection::Pin(id) => AnnotationKey::Marker(id),
    });
    AnnotationLayers { frames, comments, pins, carried, selected }
}

/// Applies the annotation events: select, preview and commit moves and resizes, then `finish_edit` once if anything
/// was committed.
///
/// The events are the framework's for this pass only: a rerun pass carries no press and a drag that did not move in
/// it proposes nothing, so a second pass applies nothing twice. A click with no drag raises no move at all, and a drag
/// released where it began arrives as a `Cancel` (`Layers::gesture` in the framework's `graph/annotation.rs`), which
/// only clears the preview: either way nothing is sent.
pub(crate) fn apply(app: &mut MaterialCraftApp, events: &[AnnotationEvent]) {
    let mut committed = false;
    for event in events {
        match event {
            AnnotationEvent::Selected(key) => select(app, *key),
            // A secondary click selects its annotation; the canvas menu opens through `canvas_events`.
            AnnotationEvent::ContextMenu { key, .. } => select(app, Some(*key)),
            AnnotationEvent::Move { key, edit, .. } => match edit {
                // Comments and pins preview through `moving` too, as production's drag did
                // (`annotation_input.rs:30-32`); only a backdrop key carries contents.
                EditEvent::Update { value, .. } => app.graph.moving = Some((*key, *value)),
                EditEvent::Commit { after, .. } | EditEvent::Set { after, .. } => {
                    app.graph.moving = None;
                    committed |= commit_move(app, *key, *after);
                }
                EditEvent::Cancel { .. } => app.graph.moving = None,
                _ => {}
            },
            AnnotationEvent::Resize { key: AnnotationKey::Backdrop(id), edit, .. } => match edit {
                EditEvent::Update { value, .. } => app.graph.resizing = resized(app, *id, *value).map(|rect| (*id, rect)),
                EditEvent::Commit { after, .. } | EditEvent::Set { after, .. } => {
                    let rect = resized(app, *id, *after);
                    app.graph.resizing = None;
                    committed |= rect.is_some_and(|rect| commit_resize(app, *id, rect));
                }
                EditEvent::Cancel { .. } => app.graph.resizing = None,
                _ => {}
            },
            _ => {}
        }
    }
    if committed {
        app.session.finish_edit();
    }
}

/// The frames', comments' and pins' rects in screen points, for `GraphView::publish`.
pub(crate) fn screen_rects(layers: &AnnotationLayers, view: &CanvasView) -> (Vec<(u64, Rect)>, Vec<(u64, Rect)>, Vec<(u64, Rect)>) {
    let point = |p: Pos2| Point::new(f64::from(p.x), f64::from(p.y));
    let screen = |rect: Rect| Rect::from_two_pos(view.to_screen(point(rect.min)), view.to_screen(point(rect.max)));
    (
        layers.frames.iter().map(|frame| (frame.id, screen(frame_rect(frame)))).collect(),
        layers.comments.iter().map(|comment| (comment.id, screen(note_rect(comment)))).collect(),
        layers.pins.iter().map(|pin| (pin.id, screen(pin_rect(pin)))).collect(),
    )
}

/// Selects `key`, or clears the selection for `None`. Selecting a pin also leaves the explorer's inspected folder and
/// graph, so the properties show the pin.
fn select(app: &mut MaterialCraftApp, key: Option<AnnotationKey>) {
    let params = match key {
        Some(AnnotationKey::Backdrop(id)) => json!({"kind": "frame", "id": id}),
        Some(AnnotationKey::Note(id)) => json!({"kind": "comment", "id": id}),
        Some(AnnotationKey::Marker(id)) => {
            app.inspected_folder = None;
            app.inspected_graph = None;
            json!({"kind": "pin", "id": id})
        }
        Some(_) => return,
        None => json!({}),
    };
    app.run("graph.annotation.select", params);
}

/// Sends the move of `key` by `by`, in graph units; returns whether a command ran. A frame's offset is snapped by the
/// engine with the same rule as the preview.
fn commit_move(app: &mut MaterialCraftApp, key: AnnotationKey, by: Vec2) -> bool {
    if by.length() <= SLOP {
        return false;
    }
    let items = app.graph.annotation_target.items(&app.session).unwrap_or_default();
    let at = |pos: [f32; 2]| [pos[0] + by.x, pos[1] + by.y];
    let (command, params) = match key {
        AnnotationKey::Backdrop(id) => ("graph.frame.move", json!({"id": id, "by": [by.x, by.y], "snapToGrid": app.ui.prefs.snap_to_grid})),
        AnnotationKey::Note(id) => match items.comments.iter().find(|comment| comment.id == id) {
            Some(comment) => ("graph.comment.edit", json!({"id": id, "at": at(comment.pos)})),
            None => return false,
        },
        AnnotationKey::Marker(id) => match items.pins.iter().find(|pin| pin.id == id) {
            Some(pin) => ("graph.pin.edit", json!({"id": id, "at": at(pin.pos)})),
            None => return false,
        },
        _ => return false,
    };
    app.run(command, params);
    true
}

/// Sends frame `id`'s new rect; returns whether a command ran.
fn commit_resize(app: &mut MaterialCraftApp, id: u64, rect: Rect) -> bool {
    let Some(frame) = find_frame(app, id) else { return false };
    if (rect.size() - frame_rect(&frame).size()).length() <= SLOP {
        return false;
    }
    app.run("graph.frame.edit", json!({"id": id, "rect": [rect.min.x, rect.min.y, rect.width(), rect.height()]}));
    true
}

/// The rect of frame `id` resized by the drag the framework's `rect` reports, no smaller than [`FRAME_MIN`].
///
/// The framework adds the drag since the press to the rect it was handed this pass, which is the resize preview
/// while one runs, so the drag is `rect`'s size less the handed size, and it applies to the stored rect.
// PLAN: graph-w5's `Layers::gesture` resizes `self.backdrops`' rect, which is the host's preview mid-drag; it should
// keep the rect from the press (Begin) so hosts need not undo the sum; this inverts it, exact unless the framework's
// `backdrop_min` clamped the reported size.
fn resized(app: &MaterialCraftApp, id: u64, rect: Rect) -> Option<Rect> {
    let stored = frame_rect(&find_frame(app, id)?);
    let handed = app.graph.resizing.filter(|(held, _)| *held == id).map_or(stored, |(_, preview)| preview);
    Some(Rect::from_min_size(stored.min, (stored.size() + rect.size() - handed.size()).max(FRAME_MIN)))
}

fn find_frame(app: &MaterialCraftApp, id: u64) -> Option<Frame> {
    app.graph.annotation_target.items(&app.session).ok()?.frames.into_iter().find(|frame| frame.id == id)
}

fn frame_rect(frame: &Frame) -> Rect {
    Rect::from_min_size(pos2(frame.rect[0], frame.rect[1]), vec2(frame.rect[2], frame.rect[3]))
}

/// A comment's rect: its stored width, which its text wraps to; the framework grows the height to the text.
fn note_rect(comment: &Comment) -> Rect {
    Rect::from_min_size(pos2(comment.pos[0], comment.pos[1]), vec2(comment.size[0], NOTE_HEIGHT))
}

fn pin_rect(pin: &Pin) -> Rect {
    Rect::from_min_size(pos2(pin.pos[0] - PIN_SIDE / 2.0, pin.pos[1] - PIN_SIDE), Vec2::splat(PIN_SIDE))
}

/// A frame's stored linear colour as the sRGB fill it is shown with; alpha is the fill opacity.
fn fill(color: [f32; 4]) -> DataColor {
    let channel = |value: f32| (linear_to_srgb(value) * 255.0).round() as u8;
    DataColor::new(Color32::from_rgba_unmultiplied(channel(color[0]), channel(color[1]), channel(color[2]), (color[3] * 255.0).round() as u8))
}
