//! Image annotation editors, ported from the Swift `ImageAnnotationView`
//! (freehand / arrow markup / crop).
//!
//! The Swift editors lean on PencilKit and UIKit. The portable core here is
//! the geometry — aspect-fit, arrow path math, crop-rect resize/clamp — all
//! unit-tested below. The editors themselves render as SVG over an `<img>`
//! and capture strokes with pointer events, so they run unchanged in the
//! Dioxus webview on iOS and Android. The caller flattens: strokes live in
//! normalized image coordinates (0…1), resolution-independent, exactly like
//! Swift's `ArrowMarkup`.
//!
//! Image bytes in/out are the launcher's job (it owns the `<img>` source
//! and the canvas flatten); these components report annotations only.

use dioxus::prelude::*;

/// Aspect-fit an image inside `bounds` — the displayed rect the annotation
/// layer overlays exactly. Port of Swift's `ImageAnnotationView.fittedSize`.
pub fn fitted_size(image_w: f64, image_h: f64, bounds_w: f64, bounds_h: f64) -> (f64, f64) {
    if image_w <= 0.0 || image_h <= 0.0 || bounds_w <= 0.0 || bounds_h <= 0.0 {
        return (bounds_w, bounds_h);
    }
    let scale = (bounds_w / image_w).min(bounds_h / image_h);
    (image_w * scale, image_h * scale)
}

/// Stroke width scales with the rendered width, like Swift's
/// `ArrowMarkup.lineWidth(for:)`.
pub fn line_width_for(rendered_width: f64) -> f64 {
    (2.5f64).max(rendered_width * 0.009)
}

/// Arrow head length scales with the rendered width, like Swift's
/// `ArrowMarkup.headLength(for:)`.
pub fn head_length_for(rendered_width: f64) -> f64 {
    (12.0f64).max(rendered_width * 0.038)
}

/// Arrow path as SVG subpaths: the shaft plus two head strokes. Port of
/// Swift's `ArrowGeometry.path(from:to:headLength:)`; each inner vec is one
/// continuous polyline.
pub fn arrow_subpaths(
    start: (f64, f64),
    end: (f64, f64),
    head_length: f64,
) -> Vec<Vec<(f64, f64)>> {
    let angle = (end.1 - start.1).atan2(end.0 - start.0);
    let spread = std::f64::consts::PI / 7.0;
    let left = (
        end.0 - head_length * (angle - spread).cos(),
        end.1 - head_length * (angle - spread).sin(),
    );
    let right = (
        end.0 - head_length * (angle + spread).cos(),
        end.1 - head_length * (angle + spread).sin(),
    );
    vec![vec![start, end], vec![end, left], vec![end, right]]
}

/// One arrow in normalized image coordinates (0…1), resolution-independent.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrow {
    pub start: (f64, f64),
    pub end: (f64, f64),
    /// `#rrggbb` hex.
    pub color: String,
}

impl Arrow {
    /// Pixel length of the shaft at a given rendered size; drags shorter
    /// than ~8px are discarded (Swift parity).
    pub fn pixel_length(&self, rendered_w: f64, rendered_h: f64) -> f64 {
        let dx = (self.end.0 - self.start.0) * rendered_w;
        let dy = (self.end.1 - self.start.1) * rendered_h;
        dx.hypot(dy)
    }

    /// Clamp both endpoints into the unit square.
    pub fn clamped(mut self) -> Self {
        self.start.0 = self.start.0.clamp(0.0, 1.0);
        self.start.1 = self.start.1.clamp(0.0, 1.0);
        self.end.0 = self.end.0.clamp(0.0, 1.0);
        self.end.1 = self.end.1.clamp(0.0, 1.0);
        self
    }
}

/// Adjustable crop rectangle in normalized image coordinates.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct CropRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl CropRect {
    pub fn full() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        }
    }

    /// Move, clamped so the rect stays inside the unit square.
    pub fn moved(mut self, dx: f64, dy: f64) -> Self {
        self.x = (self.x + dx).clamp(0.0, 1.0 - self.w);
        self.y = (self.y + dy).clamp(0.0, 1.0 - self.h);
        self
    }

    /// Resize by dragging a corner, with a minimum size (Swift parity:
    /// 44pt handles against the fitted rect).
    pub fn resized(mut self, corner: Corner, to: (f64, f64), min_size: f64) -> Self {
        let x = to.0.clamp(0.0, 1.0);
        let y = to.1.clamp(0.0, 1.0);
        match corner {
            Corner::TopLeft => {
                let nx = x.min(self.x + self.w - min_size);
                let ny = y.min(self.y + self.h - min_size);
                self.w = self.x + self.w - nx;
                self.h = self.y + self.h - ny;
                self.x = nx;
                self.y = ny;
            }
            Corner::TopRight => {
                let ny = y.min(self.y + self.h - min_size);
                self.w = (x - self.x).max(min_size).min(1.0 - self.x);
                self.h = self.y + self.h - ny;
                self.y = ny;
            }
            Corner::BottomLeft => {
                let nx = x.min(self.x + self.w - min_size);
                self.w = self.x + self.w - nx;
                self.x = nx;
                self.h = (y - self.y).max(min_size).min(1.0 - self.y);
            }
            Corner::BottomRight => {
                self.w = (x - self.x).max(min_size).min(1.0 - self.x);
                self.h = (y - self.y).max(min_size).min(1.0 - self.y);
            }
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// One freehand stroke: polyline in normalized image coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub points: Vec<(f64, f64)>,
    pub color: String,
    pub width_px: f64,
}

/// Annotation palette, mirroring Swift's `ArrowMarkup.palette`.
pub const PALETTE: [&str; 5] = ["#EF4444", "#EAB308", "#22C55E", "#3B82F6", "#FFFFFF"];

/// Render arrow subpaths as an SVG `d` attribute (shaft + head).
pub fn arrow_svg_d(start: (f64, f64), end: (f64, f64), head_length: f64) -> String {
    arrow_subpaths(start, end, head_length)
        .iter()
        .map(|sub| {
            sub.iter()
                .enumerate()
                .map(|(i, (x, y))| format!("{} {x:.1} {y:.1}", if i == 0 { 'M' } else { 'L' }))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Bounding rects of the annotation canvas and of the letterboxed `<img>`
/// inside it, as `[left, top, width, height]` CSS pixels.
///
/// The `<img>` uses `object-fit: contain`, so on non-matching aspect
/// ratios it letterboxes: strokes must normalize against the *image*
/// rect, not the whole canvas, or annotations drift on the bars.
///
/// JS: measures both rects, streams them over `dioxus.send`, and
/// re-sends on window resize/scroll (capture). A generation counter
/// retires the previous editor's listeners — only one annotation editor
/// is open at a time (it is a modal sheet), so the class selector is
/// unambiguous.
const ANNOT_RECT_JS: &str = r#"
(() => {
  window.__unpeelAnnotGen = (window.__unpeelAnnotGen || 0) + 1;
  const gen = window.__unpeelAnnotGen;
  const onEvt = () => {
    if (gen !== window.__unpeelAnnotGen) {
      window.removeEventListener('resize', onEvt);
      window.removeEventListener('scroll', onEvt, true);
      return;
    }
    const canvas = document.querySelector('.annotation-canvas');
    const img = canvas ? canvas.querySelector('.annotation-image') : null;
    if (!canvas || !img) { return; }
    const c = canvas.getBoundingClientRect();
    const r = img.getBoundingClientRect();
    dioxus.send({
      canvas: [c.left, c.top, c.width, c.height],
      img: [r.left, r.top, r.width, r.height],
    });
  };
  window.addEventListener('resize', onEvt);
  window.addEventListener('scroll', onEvt, true);
  onEvt();
})()
"#;

/// Canvas + image rects, `[left, top, width, height]` CSS px each.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct AnnotRects {
    pub canvas: [f64; 4],
    pub img: [f64; 4],
}

/// Start tracking the annotation rects: one JS eval on mount streams the
/// current rects and re-sends on resize/scroll. Pointer handlers read the
/// cached signal synchronously — measuring on pointerdown instead would
/// race the async eval round-trip (the first stroke's pointerdown would
/// normalize against a stale or missing rect).
pub fn track_annot_rects() -> Signal<Option<AnnotRects>> {
    let rects = use_signal(|| None::<AnnotRects>);
    use_effect(move || {
        let mut rects = rects;
        spawn(async move {
            let mut ev = dioxus::document::eval(ANNOT_RECT_JS);
            while let Ok(r) = ev.recv::<AnnotRects>().await {
                rects.set(Some(r));
            }
        });
    });
    // Retire this editor's window listeners on unmount (the generation
    // guard also retires them lazily on the next mount).
    use_drop(|| {
        spawn(async move {
            let _ = dioxus::document::eval(
                "window.__unpeelAnnotGen = (window.__unpeelAnnotGen || 0) + 1;",
            )
            .join::<()>()
            .await;
        });
    });
    rects
}

/// Pure mapping: client (viewport) coords → normalized image coords
/// against the letterboxed image rect. Clamped to the unit square so
/// presses on the letterbox bars pin to the image edge.
pub fn client_to_normalized(
    img_rect: (f64, f64, f64, f64),
    client_x: f64,
    client_y: f64,
) -> Option<(f64, f64)> {
    let (left, top, w, h) = img_rect;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some((
        ((client_x - left) / w).clamp(0.0, 1.0),
        ((client_y - top) / h).clamp(0.0, 1.0),
    ))
}

/// Normalize a pointer event into unit image coords using the cached
/// image rect. `None` until the first rect measurement lands.
pub fn normalize_pointer(rects: Option<AnnotRects>, ev: &PointerEvent) -> Option<(f64, f64)> {
    let [left, top, w, h] = rects?.img;
    let c = ev.client_coordinates();
    client_to_normalized((left, top, w, h), c.x, c.y)
}

/// Overlay geometry: `(left, top, width, height)` CSS px of the image
/// rect relative to the canvas origin, for positioning the SVG/crop
/// overlay exactly over the displayed (letterboxed) image.
pub fn overlay_geometry(rects: AnnotRects) -> (f64, f64, f64, f64) {
    let [cl, ct, _, _] = rects.canvas;
    let [l, t, w, h] = rects.img;
    (l - cl, t - ct, w, h)
}

/// Arrow-markup editor: drag start → end to draw an arrow over the image.
/// Reports normalized-coordinate arrows via `on_done`; `on_cancel`
/// discards. Colors from [`PALETTE`]; undo/clear included (Swift parity).
///
/// Pointer normalization reads the cached image rect (tracked from mount
/// via [`track_annot_rects`]) synchronously — the overlay is positioned
/// exactly over the letterboxed image, so strokes map 1:1 to image
/// coords.
#[component]
pub fn ArrowMarkupView(
    /// Image URL shown under the annotation layer.
    image_src: String,
    on_done: EventHandler<Vec<Arrow>>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut arrows = use_signal(Vec::<Arrow>::new);
    let mut current = use_signal(|| None::<Arrow>);
    let mut color = use_signal(|| PALETTE[0].to_string());
    let rects = track_annot_rects();
    let overlay_style = move || {
        rects
            .read()
            .as_ref()
            .map(|r| {
                let (l, t, w, h) = overlay_geometry(*r);
                format!("left:{l:.1}px;top:{t:.1}px;width:{w:.1}px;height:{h:.1}px;")
            })
            .unwrap_or_else(|| "display:none;".to_string())
    };

    rsx! {
        div { class: "annotation-editor",
            div { class: "annotation-toolbar",
                button { onclick: move |_| on_cancel.call(()), "Cancel" }
                div { class: "annotation-spacer" }
                button {
                    onclick: move |_| { arrows.write().pop(); },
                    "Undo"
                }
                button {
                    onclick: move |_| { arrows.write().clear(); },
                    "Clear"
                }
                button {
                    class: "annotation-done",
                    "data-testid": "annotation-done",
                    onclick: move |_| on_done.call(arrows.read().clone()),
                    "Done"
                }
            }
            div {
                class: "annotation-canvas",
                onpointerdown: move |ev| {
                    if let Some(p) = normalize_pointer(*rects.read(), &ev) {
                        current.set(Some(Arrow {
                            start: p,
                            end: p,
                            color: color.read().clone(),
                        }));
                    }
                },
                onpointermove: move |ev| {
                    if let Some(p) = normalize_pointer(*rects.read(), &ev) {
                        let pending = current.write().take();
                        if let Some(mut a) = pending {
                            a.end = p;
                            current.set(Some(a));
                        }
                    }
                },
                onpointerup: move |_| {
                    if let Some(a) = current.write().take() {
                        // ~8px minimum at a nominal 400px render width.
                        if a.pixel_length(400.0, 400.0) > 8.0 {
                            arrows.write().push(a.clamped());
                        }
                    }
                },
                img { src: "{image_src}", class: "annotation-image", draggable: "false" }
                svg {
                    class: "annotation-overlay",
                    style: "{overlay_style()}",
                    view_box: "0 0 100 100",
                    preserve_aspect_ratio: "none",
                    for arrow in arrows.read().iter() {
                        path {
                            d: "{arrow_svg_d((arrow.start.0 * 100.0, arrow.start.1 * 100.0), (arrow.end.0 * 100.0, arrow.end.1 * 100.0), 4.5)}",
                            stroke: "{arrow.color}",
                            stroke_width: "1.1",
                            fill: "none",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                        }
                    }
                    if let Some(a) = current.read().as_ref() {
                        path {
                            d: "{arrow_svg_d((a.start.0 * 100.0, a.start.1 * 100.0), (a.end.0 * 100.0, a.end.1 * 100.0), 4.5)}",
                            stroke: "{a.color}",
                            stroke_width: "1.1",
                            fill: "none",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                        }
                    }
                }
            }
            div { class: "annotation-palette",
                for swatch in PALETTE {
                    button {
                        class: if *color.read() == swatch { "swatch selected" } else { "swatch" },
                        style: "background: {swatch}",
                        aria_label: "Color",
                        onclick: move |_| color.set(swatch.to_string()),
                    }
                }
            }
        }
    }
}

/// Freehand editor: pointer strokes become SVG polylines. Reports the
/// stroke list via `on_done`.
#[component]
pub fn FreehandView(
    image_src: String,
    on_done: EventHandler<Vec<Stroke>>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut strokes = use_signal(Vec::<Stroke>::new);
    let mut current = use_signal(Vec::<(f64, f64)>::new);
    let mut color = use_signal(|| "#FFFFFF".to_string());
    let rects = track_annot_rects();
    let overlay_style = move || {
        rects
            .read()
            .as_ref()
            .map(|r| {
                let (l, t, w, h) = overlay_geometry(*r);
                format!("left:{l:.1}px;top:{t:.1}px;width:{w:.1}px;height:{h:.1}px;")
            })
            .unwrap_or_else(|| "display:none;".to_string())
    };

    let polyline = |pts: &[(f64, f64)]| -> String {
        pts.iter()
            .map(|(x, y)| format!("{:.1},{:.1}", x * 100.0, y * 100.0))
            .collect::<Vec<_>>()
            .join(" ")
    };

    rsx! {
        div { class: "annotation-editor",
            div { class: "annotation-toolbar",
                button { onclick: move |_| on_cancel.call(()), "Cancel" }
                div { class: "annotation-spacer" }
                button {
                    onclick: move |_| { strokes.write().pop(); },
                    "Undo"
                }
                button {
                    onclick: move |_| { strokes.write().clear(); },
                    "Clear"
                }
                button {
                    class: "annotation-done",
                    "data-testid": "annotation-done",
                    onclick: move |_| on_done.call(strokes.read().clone()),
                    "Done"
                }
            }
            div {
                class: "annotation-canvas",
                onpointerdown: move |ev| {
                    if let Some(p) = normalize_pointer(*rects.read(), &ev) {
                        current.set(vec![p]);
                    }
                },
                onpointermove: move |ev| {
                    if let Some(p) = normalize_pointer(*rects.read(), &ev) {
                        if !current.read().is_empty() {
                            current.write().push(p);
                        }
                    }
                },
                onpointerup: move |_| {
                    let pts: Vec<(f64, f64)> = std::mem::take(current.write().as_mut());
                    if pts.len() > 1 {
                        strokes.write().push(Stroke {
                            points: pts,
                            color: color.read().clone(),
                            width_px: 4.0,
                        });
                    }
                },
                img { src: "{image_src}", class: "annotation-image", draggable: "false" }
                svg {
                    class: "annotation-overlay",
                    style: "{overlay_style()}",
                    view_box: "0 0 100 100",
                    preserve_aspect_ratio: "none",
                    for stroke in strokes.read().iter() {
                        polyline {
                            points: "{polyline(&stroke.points)}",
                            stroke: "{stroke.color}",
                            stroke_width: "1.1",
                            fill: "none",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                        }
                    }
                    {
                        let pts = current.read();
                        if pts.len() > 1 {
                            let d = polyline(&pts);
                            let c = color.read().clone();
                            rsx! {
                                polyline {
                                    points: "{d}",
                                    stroke: "{c}",
                                    stroke_width: "1.1",
                                    fill: "none",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                }
            }
            div { class: "annotation-palette",
                for swatch in PALETTE {
                    button {
                        class: if *color.read() == swatch { "swatch selected" } else { "swatch" },
                        style: "background: {swatch}",
                        aria_label: "Color",
                        onclick: move |_| color.set(swatch.to_string()),
                    }
                }
            }
        }
    }
}

/// Crop editor: adjustable rectangle with four corner handles + drag to
/// move. Reports the normalized [`CropRect`] via `on_done`.
#[component]
pub fn CropView(
    image_src: String,
    on_done: EventHandler<CropRect>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut crop = use_signal(CropRect::full);
    let mut drag_corner = use_signal(|| None::<Corner>);
    let mut moving = use_signal(|| false);
    let mut last_point = use_signal(|| None::<(f64, f64)>);
    let rects = track_annot_rects();
    let overlay_style = move || {
        rects
            .read()
            .as_ref()
            .map(|r| {
                let (l, t, w, h) = overlay_geometry(*r);
                format!("left:{l:.1}px;top:{t:.1}px;width:{w:.1}px;height:{h:.1}px;")
            })
            .unwrap_or_else(|| "display:none;".to_string())
    };

    let corners = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomRight,
    ];
    let corner_pos = |c: &CropRect, corner: Corner| -> (f64, f64) {
        match corner {
            Corner::TopLeft => (c.x, c.y),
            Corner::TopRight => (c.x + c.w, c.y),
            Corner::BottomLeft => (c.x, c.y + c.h),
            Corner::BottomRight => (c.x + c.w, c.y + c.h),
        }
    };

    rsx! {
        div { class: "annotation-editor",
            div { class: "annotation-toolbar",
                button { onclick: move |_| on_cancel.call(()), "Cancel" }
                div { class: "annotation-spacer" }
                button {
                    onclick: move |_| crop.set(CropRect::full()),
                    "Reset"
                }
                button {
                    class: "annotation-done",
                    "data-testid": "annotation-done",
                    onclick: move |_| on_done.call(*crop.read()),
                    "Done"
                }
            }
            div {
                class: "annotation-canvas",
                onpointerdown: move |ev| {
                    last_point.set(normalize_pointer(*rects.read(), &ev));
                },
                onpointermove: move |ev| {
                    let Some(p) = normalize_pointer(*rects.read(), &ev) else { return };
                    if let Some(corner) = *drag_corner.read() {
                        let next = crop.read().resized(corner, p, 0.05);
                        crop.set(next);
                    } else if *moving.read() {
                        if let Some(last) = *last_point.read() {
                            let next = crop.read().moved(p.0 - last.0, p.1 - last.1);
                            crop.set(next);
                        }
                    }
                    last_point.set(Some(p));
                },
                onpointerup: move |_| {
                    drag_corner.set(None);
                    moving.set(false);
                    last_point.set(None);
                },
                img { src: "{image_src}", class: "annotation-image", draggable: "false" }
                div {
                    class: "annotation-overlay",
                    style: "{overlay_style()}",
                    {
                    let c = *crop.read();
                    rsx! {
                        div {
                            class: "crop-rect",
                            style: "left: {c.x * 100.0}%; top: {c.y * 100.0}%; width: {c.w * 100.0}%; height: {c.h * 100.0}%;",
                            onpointerdown: move |ev| {
                                ev.stop_propagation();
                                // Capture the drag origin here: the canvas
                                // handler is skipped (stop_propagation), so
                                // without this the first move would jump
                                // from a stale `last_point`.
                                last_point.set(normalize_pointer(*rects.read(), &ev));
                                moving.set(true);
                            },
                        }
                        for corner in corners {
                            {
                                let (px, py) = corner_pos(&c, corner);
                                rsx! {
                                    div {
                                        class: "crop-handle",
                                        style: "left: {px * 100.0}%; top: {py * 100.0}%;",
                                        onpointerdown: move |ev| {
                                            ev.stop_propagation();
                                            drag_corner.set(Some(corner));
                                        },
                                    }
                                }
                            }
                        }
                    }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Flatten: annotations → PNG bytes at native resolution.
//
// The editors report normalized-coordinate annotations; flattening is the
// launcher's job (it owns the image bytes and the upload). The geometry
// stays in Rust (reusing the tested `arrow_subpaths`), the pixel work
// happens in a `<canvas>` inside the webview, and the PNG comes back as
// bytes — no Host changes needed, and no CORS-tainted canvas because the
// image is fed in as a data URL from bytes the launcher already holds.
// ---------------------------------------------------------------------------

/// Line width / arrow-head size used by the editors, as fractions of the
/// image width. The SVG editors render strokes at 1.1 viewBox units and
/// arrow heads at 4.5 (viewBox is 0…100), so the flatten uses the same
/// fractions for a WYSIWYG match.
pub const FLAT_WIDTH_FRAC: f64 = 0.011;
pub const FLAT_HEAD_FRAC: f64 = 0.045;

/// One drawable path in normalized *output* coordinates (already
/// remapped through the crop, 0…1), line width as a fraction of the
/// output width.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FlatPath {
    pub points: Vec<(f64, f64)>,
    pub color: String,
    pub width_frac: f64,
}

/// Everything the JS flattener needs: source image bytes (base64) with
/// their MIME type, the optional crop in original-image normalized
/// coords, and the drawable paths in cropped-output normalized coords.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FlattenSpec {
    pub image_b64: String,
    pub mime: String,
    pub crop: Option<CropRect>,
    pub paths: Vec<FlatPath>,
}

/// Remap a normalized point from original-image space into
/// cropped-output space (clamped to the unit square).
pub fn remap_through_crop(crop: Option<CropRect>, p: (f64, f64)) -> (f64, f64) {
    match crop {
        None => p,
        Some(c) => {
            if c.w <= 0.0 || c.h <= 0.0 {
                return p;
            }
            (
                ((p.0 - c.x) / c.w).clamp(0.0, 1.0),
                ((p.1 - c.y) / c.h).clamp(0.0, 1.0),
            )
        }
    }
}

/// Build the flatten spec from the editors' outputs: arrows expand to
/// their shaft + head subpaths (the tested [`arrow_subpaths`]), strokes
/// pass through, everything is remapped through the crop.
pub fn flatten_spec(
    image_bytes: &[u8],
    mime: &str,
    arrows: &[Arrow],
    strokes: &[Stroke],
    crop: Option<CropRect>,
) -> FlattenSpec {
    use base64::Engine as _;
    let mut paths = Vec::new();
    for a in arrows {
        for sub in arrow_subpaths(a.start, a.end, FLAT_HEAD_FRAC) {
            let points: Vec<(f64, f64)> = sub
                .into_iter()
                .map(|p| remap_through_crop(crop, p))
                .collect();
            if points.len() > 1 {
                paths.push(FlatPath {
                    points,
                    color: a.color.clone(),
                    width_frac: FLAT_WIDTH_FRAC,
                });
            }
        }
    }
    for s in strokes {
        if s.points.len() > 1 {
            paths.push(FlatPath {
                points: s
                    .points
                    .iter()
                    .map(|&p| remap_through_crop(crop, p))
                    .collect(),
                color: s.color.clone(),
                width_frac: FLAT_WIDTH_FRAC,
            });
        }
    }
    FlattenSpec {
        image_b64: base64::engine::general_purpose::STANDARD.encode(image_bytes),
        mime: mime.to_string(),
        crop,
        paths,
    }
}

#[derive(Debug, serde::Deserialize)]
struct FlattenReply {
    ok: Option<String>,
    err: Option<String>,
}

/// JS: decode the spec image, draw it (cropped) at native resolution,
/// stroke the annotation paths on top, and send the PNG back as base64.
/// The spec is substituted for `__SPEC__` before eval.
const FLATTEN_JS_TEMPLATE: &str = r#"
(async () => {
  const spec = __SPEC__;
  try {
    const img = new Image();
    img.src = 'data:' + spec.mime + ';base64,' + spec.image_b64;
    await img.decode();
    const natW = img.naturalWidth, natH = img.naturalHeight;
    if (!natW || !natH) { dioxus.send({err: 'image has no pixels'}); return; }
    let sx = 0, sy = 0, sw = natW, sh = natH;
    if (spec.crop) {
      sx = Math.round(spec.crop.x * natW);
      sy = Math.round(spec.crop.y * natH);
      sw = Math.max(1, Math.round(spec.crop.w * natW));
      sh = Math.max(1, Math.round(spec.crop.h * natH));
      sx = Math.min(Math.max(0, sx), natW - 1);
      sy = Math.min(Math.max(0, sy), natH - 1);
      sw = Math.min(sw, natW - sx);
      sh = Math.min(sh, natH - sy);
    }
    const canvas = document.createElement('canvas');
    canvas.width = Math.round(sw);
    canvas.height = Math.round(sh);
    const ctx = canvas.getContext('2d');
    ctx.drawImage(img, sx, sy, sw, sh, 0, 0, canvas.width, canvas.height);
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    for (const p of spec.paths) {
      if (!p.points || p.points.length < 2) { continue; }
      ctx.strokeStyle = p.color;
      ctx.lineWidth = Math.max(1, p.width_frac * canvas.width);
      ctx.beginPath();
      ctx.moveTo(p.points[0][0] * canvas.width, p.points[0][1] * canvas.height);
      for (let i = 1; i < p.points.length; i++) {
        ctx.lineTo(p.points[i][0] * canvas.width, p.points[i][1] * canvas.height);
      }
      ctx.stroke();
    }
    canvas.toBlob((blob) => {
      if (!blob) { dioxus.send({err: 'toBlob failed'}); return; }
      const fr = new FileReader();
      fr.onload = () => dioxus.send({ok: String(fr.result).split(',')[1]});
      fr.onerror = () => dioxus.send({err: 'blob read failed'});
      fr.readAsDataURL(blob);
    }, 'image/png');
  } catch (e) {
    dioxus.send({err: String((e && e.message) || e)});
  }
})()
"#;

/// Flatten a [`FlattenSpec`] to PNG bytes at the image's native
/// resolution. Runs in the webview (needs a live document); the caller
/// uploads the bytes (e.g. via `HostClient::upload_artifact`).
pub async fn flatten_annotation_png(spec: &FlattenSpec) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    let json = serde_json::to_string(spec).map_err(|e| format!("flatten spec: {e}"))?;
    // The spec is substituted into a JS context: neutralize `</` so a
    // stray `</script>` sequence can never break out of the eval block.
    let json = json.replace("</", "<\\/");
    let js = FLATTEN_JS_TEMPLATE.replace("__SPEC__", &json);
    let mut ev = dioxus::document::eval(&js);
    let reply: FlattenReply = ev.recv().await.map_err(|e| format!("flatten eval: {e}"))?;
    if let Some(err) = reply.err {
        return Err(err);
    }
    let b64 = reply.ok.ok_or_else(|| "flatten: empty reply".to_string())?;
    base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("flatten png decode: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitted_size_aspect_fit() {
        // Wide image in a square: height-limited.
        let (w, h) = fitted_size(200.0, 100.0, 100.0, 100.0);
        assert!((w - 100.0).abs() < 1e-9);
        assert!((h - 50.0).abs() < 1e-9);
        // Tall image: width-limited.
        let (w, h) = fitted_size(100.0, 200.0, 100.0, 100.0);
        assert!((w - 50.0).abs() < 1e-9);
        assert!((h - 100.0).abs() < 1e-9);
    }

    #[test]
    fn fitted_size_degenerate_returns_bounds() {
        assert_eq!(fitted_size(0.0, 10.0, 50.0, 50.0), (50.0, 50.0));
        assert_eq!(fitted_size(10.0, 10.0, 0.0, 50.0), (0.0, 50.0));
    }

    #[test]
    fn arrow_path_has_shaft_and_head() {
        let subs = arrow_subpaths((0.0, 0.0), (10.0, 0.0), 3.0);
        assert_eq!(subs.len(), 3);
        // Shaft: start → end.
        assert_eq!(subs[0], vec![(0.0, 0.0), (10.0, 0.0)]);
        // Head strokes start at the tip and angle backwards.
        for head in &subs[1..] {
            assert_eq!(head[0], (10.0, 0.0));
            assert!(head[1].0 < 10.0);
        }
        // Symmetric about the shaft.
        assert!((subs[1][1].1 + subs[2][1].1).abs() < 1e-9);
        assert!((subs[1][1].0 - subs[2][1].0).abs() < 1e-9);
    }

    #[test]
    fn arrow_svg_d_renders_three_subpaths() {
        let d = arrow_svg_d((0.0, 0.0), (100.0, 0.0), 18.0);
        assert_eq!(d.matches('M').count(), 3);
        assert!(d.starts_with("M 0.0 0.0 L 100.0 0.0"));
    }

    #[test]
    fn arrow_clamped_to_unit_square() {
        let a = Arrow {
            start: (-0.2, 0.5),
            end: (1.4, 0.5),
            color: "#fff".to_string(),
        }
        .clamped();
        assert_eq!(a.start.0, 0.0);
        assert_eq!(a.end.0, 1.0);
    }

    #[test]
    fn stroke_width_scales() {
        assert_eq!(line_width_for(100.0), 2.5);
        assert!((line_width_for(1000.0) - 9.0).abs() < 1e-9);
        assert_eq!(head_length_for(100.0), 12.0);
        assert!((head_length_for(1000.0) - 38.0).abs() < 1e-9);
    }

    #[test]
    fn crop_move_clamps_inside() {
        let c = CropRect {
            x: 0.8,
            y: 0.8,
            w: 0.2,
            h: 0.2,
        }
        .moved(0.5, 0.5);
        assert_eq!((c.x, c.y), (0.8, 0.8));
        let c = CropRect::full().moved(-0.5, -0.5);
        assert_eq!((c.x, c.y), (0.0, 0.0));
    }

    #[test]
    fn crop_resize_corners() {
        let c = CropRect::full().resized(Corner::BottomRight, (0.5, 0.5), 0.05);
        assert!((c.w - 0.5).abs() < 1e-9);
        assert!((c.h - 0.5).abs() < 1e-9);
        // Minimum size holds.
        let c = CropRect::full().resized(Corner::BottomRight, (0.01, 0.01), 0.05);
        assert!((c.w - 0.05).abs() < 1e-9);
        // Top-left drag moves origin and shrinks.
        let c = CropRect::full().resized(Corner::TopLeft, (0.25, 0.25), 0.05);
        assert!((c.x - 0.25).abs() < 1e-9);
        assert!((c.w - 0.75).abs() < 1e-9);
    }

    // -- pointer mapping against the letterboxed image rect -------------

    #[test]
    fn normalize_wide_image_in_square_canvas() {
        // 2:1 image letterboxed in a 100x100 canvas: img rect (0,25,100,50).
        let p = client_to_normalized((0.0, 25.0, 100.0, 50.0), 50.0, 50.0).unwrap();
        assert!((p.0 - 0.5).abs() < 1e-9);
        assert!((p.1 - 0.5).abs() < 1e-9);
    }

    #[test]
    fn normalize_tall_image_in_square_canvas() {
        // 1:2 image letterboxed in a 100x100 canvas: img rect (25,0,50,100).
        let p = client_to_normalized((25.0, 0.0, 50.0, 100.0), 50.0, 50.0).unwrap();
        assert!((p.0 - 0.5).abs() < 1e-9);
        assert!((p.1 - 0.5).abs() < 1e-9);
    }

    #[test]
    fn normalize_letterbox_gutter_clamps_to_edge() {
        // Press in the top bar pins to the image's top edge, x still maps.
        let p = client_to_normalized((0.0, 25.0, 100.0, 50.0), 75.0, 0.0).unwrap();
        assert!((p.0 - 0.75).abs() < 1e-9);
        assert_eq!(p.1, 0.0);
        // Press in the bottom bar pins to the bottom edge.
        let p = client_to_normalized((0.0, 25.0, 100.0, 50.0), 25.0, 99.0).unwrap();
        assert!((p.0 - 0.25).abs() < 1e-9);
        assert_eq!(p.1, 1.0);
    }

    #[test]
    fn normalize_degenerate_rect_is_none() {
        assert!(client_to_normalized((0.0, 0.0, 0.0, 50.0), 10.0, 10.0).is_none());
        assert!(client_to_normalized((0.0, 0.0, 100.0, -1.0), 10.0, 10.0).is_none());
    }

    #[test]
    fn overlay_geometry_relative_to_canvas() {
        let r = AnnotRects {
            canvas: [10.0, 20.0, 100.0, 100.0],
            img: [10.0, 45.0, 100.0, 50.0],
        };
        assert_eq!(overlay_geometry(r), (0.0, 25.0, 100.0, 50.0));
    }

    // -- flatten spec ----------------------------------------------------

    #[test]
    fn flatten_spec_expands_arrows_and_passes_strokes() {
        let arrows = vec![Arrow {
            start: (0.1, 0.1),
            end: (0.9, 0.9),
            color: "#EF4444".to_string(),
        }];
        let strokes = vec![Stroke {
            points: vec![(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)],
            color: "#FFFFFF".to_string(),
            width_px: 4.0,
        }];
        let spec = flatten_spec(&[1, 2, 3], "image/png", &arrows, &strokes, None);
        // One arrow → shaft + 2 head subpaths; one stroke → one path.
        assert_eq!(spec.paths.len(), 4);
        assert!(spec
            .paths
            .iter()
            .all(|p| (p.width_frac - FLAT_WIDTH_FRAC).abs() < 1e-12));
        assert_eq!(spec.paths[0].color, "#EF4444");
        assert_eq!(spec.paths[3].color, "#FFFFFF");
        assert_eq!(spec.paths[3].points.len(), 3);
        assert!(spec.crop.is_none());
        // Round-trip of the image bytes through the spec's base64.
        use base64::Engine as _;
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(&spec.image_b64)
                .unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn flatten_spec_drops_degenerate_strokes() {
        let strokes = vec![Stroke {
            points: vec![(0.5, 0.5)],
            color: "#fff".to_string(),
            width_px: 4.0,
        }];
        let spec = flatten_spec(&[], "image/png", &[], &strokes, None);
        assert!(spec.paths.is_empty());
    }

    #[test]
    fn flatten_spec_remaps_through_crop() {
        let crop = CropRect {
            x: 0.25,
            y: 0.25,
            w: 0.5,
            h: 0.5,
        };
        let strokes = vec![Stroke {
            points: vec![(0.25, 0.25), (0.75, 0.75)],
            color: "#fff".to_string(),
            width_px: 4.0,
        }];
        let spec = flatten_spec(&[], "image/png", &[], &strokes, Some(crop));
        assert_eq!(spec.paths.len(), 1);
        assert!((spec.paths[0].points[0].0 - 0.0).abs() < 1e-9);
        assert!((spec.paths[0].points[1].0 - 1.0).abs() < 1e-9);
        // The crop itself is preserved for the drawImage source rect.
        assert_eq!(spec.crop, Some(crop));
    }

    #[test]
    fn remap_through_crop_identity_and_clamp() {
        assert_eq!(remap_through_crop(None, (0.3, 0.7)), (0.3, 0.7));
        assert_eq!(
            remap_through_crop(Some(CropRect::full()), (0.3, 0.7)),
            (0.3, 0.7)
        );
        let crop = CropRect {
            x: 0.5,
            y: 0.5,
            w: 0.5,
            h: 0.5,
        };
        // Point outside the crop clamps to the edge.
        assert_eq!(remap_through_crop(Some(crop), (0.0, 0.0)), (0.0, 0.0));
        let (x, y) = remap_through_crop(Some(crop), (0.75, 0.75));
        assert!((x - 0.5).abs() < 1e-9);
        assert!((y - 0.5).abs() < 1e-9);
    }
}
