use crate::theme::{self, Hint};
use egui::containers::{Popup, PopupCloseBehavior};
use egui::{Color32, Mesh, Pos2, Rect, Sense, Shape, Stroke, Vec2, pos2, vec2};
use klad_core::color::parse_hex;
use std::hash::Hash;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsva {
    pub h: f32,
    pub s: f32,
    pub v: f32,
    pub a: f32,
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let sector = h.floor();
    let f = h - sector;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match sector as u8 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

impl Hsva {
    pub fn from_rgba([r, g, b, a]: [f32; 4], keep_hue: f32) -> Self {
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if d <= 1e-6 {
            keep_hue
        } else if max == r {
            ((g - b) / d).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / d + 2.0) / 6.0
        } else {
            ((r - g) / d + 4.0) / 6.0
        };
        let s = if max <= 1e-6 { 0.0 } else { d / max };
        Self { h, s, v: max, a }
    }

    pub fn rgba(&self) -> [f32; 4] {
        let [r, g, b] = hsv_to_rgb(self.h, self.s, self.v);
        [r, g, b, self.a]
    }
}

pub fn color32([r, g, b, a]: [f32; 4]) -> Color32 {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(byte(r), byte(g), byte(b), byte(a))
}

pub fn hex(rgba: [f32; 4]) -> String {
    let [r, g, b, a] = rgba.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

pub fn from_srgb(rgb: [u8; 3]) -> [f32; 4] {
    let [r, g, b] = rgb.map(|v| v as f32 / 255.0);
    [r, g, b, 1.0]
}

pub fn to_srgb([r, g, b, _]: [f32; 4]) -> [u8; 3] {
    [r, g, b].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

pub fn rgba_of(text: &str) -> Option<[f32; 4]> {
    let trimmed = text.trim();
    let with_hash = if trimmed.starts_with('#') {
        trimmed.to_string()
    } else {
        format!("#{trimmed}")
    };
    parse_hex(&with_hash).ok()
}

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Ring,
    Square,
}

fn square(center: Pos2, inner: f32) -> Rect {
    Rect::from_center_size(center, Vec2::splat(inner * std::f32::consts::SQRT_2 - 8.0))
}

pub fn picker(ui: &mut egui::Ui, color: &mut Hsva, size: f32) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, "colour wheel"));
    let center = rect.center();
    let outer = size / 2.0 - 1.0;
    let inner = outer - (size * 0.09).max(10.0);
    let sv = square(center, inner);
    let id = response.id.with("part");
    let mut changed = false;
    if let Some(pos) = response.interact_pointer_pos() {
        if response.drag_started() || response.clicked() || ui.data(|d| d.get_temp::<u8>(id)).is_none() {
            let d = (pos - center).length();
            let part = if d >= inner - 2.0 && d <= outer + 4.0 {
                Some(Part::Ring)
            } else if sv.expand(4.0).contains(pos) {
                Some(Part::Square)
            } else {
                None
            };
            ui.data_mut(|data| match part {
                Some(Part::Ring) => data.insert_temp(id, 1u8),
                Some(Part::Square) => data.insert_temp(id, 2u8),
                None => data.insert_temp(id, 0u8),
            });
        }
        match ui.data(|d| d.get_temp::<u8>(id)) {
            Some(1) => {
                let d = pos - center;
                color.h = (d.y.atan2(d.x) / std::f32::consts::TAU).rem_euclid(1.0);
                changed = true;
            }
            Some(2) => {
                color.s = ((pos.x - sv.left()) / sv.width()).clamp(0.0, 1.0);
                color.v = 1.0 - ((pos.y - sv.top()) / sv.height()).clamp(0.0, 1.0);
                changed = true;
            }
            _ => {}
        }
    }
    if response.drag_stopped() || (!response.dragged() && !response.is_pointer_button_down_on()) {
        ui.data_mut(|d| d.remove::<u8>(id));
    }
    let painter = ui.painter_at(rect.expand(2.0));
    let mut ring = Mesh::default();
    let segments = 96;
    (0..=segments).for_each(|i| {
        let t = i as f32 / segments as f32;
        let angle = t * std::f32::consts::TAU;
        let dir = vec2(angle.cos(), angle.sin());
        let [r, g, b] = hsv_to_rgb(t, 1.0, 1.0);
        let c = color32([r, g, b, 1.0]);
        ring.colored_vertex(center + dir * inner, c);
        ring.colored_vertex(center + dir * outer, c);
        if i > 0 {
            let k = (i as u32) * 2;
            ring.add_triangle(k - 2, k - 1, k);
            ring.add_triangle(k - 1, k + 1, k);
        }
    });
    painter.add(Shape::mesh(ring));
    let mut field = Mesh::default();
    let cells = 16;
    (0..=cells).for_each(|j| {
        (0..=cells).for_each(|i| {
            let (s, v) = (i as f32 / cells as f32, 1.0 - j as f32 / cells as f32);
            let [r, g, b] = hsv_to_rgb(color.h, s, v);
            field.colored_vertex(
                pos2(sv.left() + sv.width() * s, sv.top() + sv.height() * (1.0 - v)),
                color32([r, g, b, 1.0]),
            );
            if i > 0 && j > 0 {
                let at = |x: u32, y: u32| y * (cells + 1) + x;
                field.add_triangle(at(i - 1, j - 1), at(i, j - 1), at(i, j));
                field.add_triangle(at(i - 1, j - 1), at(i, j), at(i - 1, j));
            }
        });
    });
    painter.add(Shape::mesh(field));
    let dark = Stroke::new(2.5, Color32::from_black_alpha(200));
    let light = Stroke::new(1.5, Color32::WHITE);
    let hue_at = center
        + vec2(
            (color.h * std::f32::consts::TAU).cos(),
            (color.h * std::f32::consts::TAU).sin(),
        ) * (inner + outer)
            / 2.0;
    painter.circle_stroke(hue_at, (outer - inner) / 2.0, dark);
    painter.circle_stroke(hue_at, (outer - inner) / 2.0, light);
    let sv_at = pos2(
        sv.left() + sv.width() * color.s,
        sv.top() + sv.height() * (1.0 - color.v),
    );
    painter.circle_stroke(sv_at, 5.0, dark);
    painter.circle_stroke(sv_at, 5.0, light);
    if changed {
        response.mark_changed();
    }
    response.hint("the ring sets the hue; the square inside sets how strong and how bright")
}

pub fn swatch(ui: &mut egui::Ui, rgba: [f32; 4], selected: bool, size: f32) -> egui::Response {
    swatch_with(ui, rgba, selected, size, Sense::click())
}

pub fn swatch_with(ui: &mut egui::Ui, rgba: [f32; 4], selected: bool, size: f32, sense: Sense) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, hex(rgba)));
    paint_swatch(ui, rect, rgba, selected);
    response
}

fn paint_swatch(ui: &egui::Ui, rect: Rect, rgba: [f32; 4], selected: bool) {
    let painter = ui.painter_at(rect);
    let half = rect.width() / 2.0;
    painter.rect_filled(rect, 2.0, Color32::from_gray(200));
    painter.rect_filled(
        Rect::from_min_size(rect.min, Vec2::splat(half)),
        0.0,
        Color32::from_gray(140),
    );
    painter.rect_filled(
        Rect::from_min_size(rect.min + Vec2::splat(half), Vec2::splat(half)),
        0.0,
        Color32::from_gray(140),
    );
    painter.rect_filled(rect, 2.0, color32(rgba));
    let outline = if selected {
        Stroke::new(2.0, Color32::WHITE)
    } else {
        Stroke::new(1.0, Color32::from_black_alpha(160))
    };
    painter.rect_stroke(rect, 2.0, outline, egui::StrokeKind::Inside);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    Dragging([f32; 4]),
    Chosen([f32; 4]),
    Remove,
}

impl Pick {
    pub fn chosen(self) -> Option<[f32; 4]> {
        match self {
            Pick::Chosen(rgba) => Some(rgba),
            _ => None,
        }
    }
}

pub struct Square {
    id: egui::Id,
    rgba: [f32; 4],
    what: String,
    current: Option<[f32; 4]>,
    alpha: bool,
    removable: bool,
}

impl Square {
    pub fn new(ui: &egui::Ui, salt: impl Hash + std::fmt::Debug, rgba: [f32; 4], what: impl Into<String>) -> Self {
        Self {
            id: ui.make_persistent_id(salt),
            rgba,
            what: what.into(),
            current: None,
            alpha: false,
            removable: false,
        }
    }

    pub fn current(mut self, rgba: [f32; 4]) -> Self {
        self.current = Some(rgba);
        self
    }

    pub fn alpha(mut self) -> Self {
        self.alpha = true;
        self
    }

    pub fn removable(mut self) -> Self {
        self.removable = true;
        self
    }

    pub fn pick(self, ui: &mut egui::Ui) -> Option<[f32; 4]> {
        self.show(ui).and_then(Pick::chosen)
    }

    pub fn show(self, ui: &mut egui::Ui) -> Option<Pick> {
        let live_id = self.id.with("live");
        let text_id = self.id.with("hex");
        let open = Popup::is_id_open(ui.ctx(), self.id);
        let stored = ui.data(|d| d.get_temp::<(Hsva, [f32; 4])>(live_id));
        let mut live = match stored {
            Some((hsva, base)) if open && hex(base) == hex(self.rgba) => hsva,
            _ => Hsva::from_rgba(self.rgba, stored.map_or(0.0, |(hsva, _)| hsva.h)),
        };
        if !open && stored.is_some() {
            ui.data_mut(|d| d.remove::<(Hsva, [f32; 4])>(live_id));
        }
        let shown = if open { live.rgba() } else { self.rgba };
        let response = swatch(ui, shown, open, theme::TARGET);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &self.what));
        let response = response.hint(format!("{}, {}; click to pick a colour", self.what, hex(self.rgba)));
        let mut out = None;
        Popup::from_toggle_button_response(&response)
            .id(self.id)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.label(egui::RichText::new(&self.what).color(theme::WEAK));
                let wheel = picker(ui, &mut live, 150.0);
                if wheel.drag_stopped() || (wheel.clicked() && wheel.changed()) {
                    out = Some(Pick::Chosen(live.rgba()));
                } else if wheel.changed() {
                    out = Some(Pick::Dragging(live.rgba()));
                }
                if self.alpha {
                    let slider = ui
                        .add(egui::Slider::new(&mut live.a, 0.0..=1.0).text("opacity"))
                        .hint("how see-through this colour is");
                    if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
                        out = Some(Pick::Chosen(live.rgba()));
                    } else if slider.changed() {
                        out = Some(Pick::Dragging(live.rgba()));
                    }
                }
                ui.horizontal(|ui| {
                    let label = ui.label("hex");
                    let mut text = ui
                        .data(|d| d.get_temp::<String>(text_id))
                        .unwrap_or_else(|| hex(live.rgba()));
                    let field = ui
                        .add(egui::TextEdit::singleline(&mut text).desired_width(90.0))
                        .labelled_by(label.id)
                        .hint("type a colour as #rrggbb; Return keeps it");
                    match field.has_focus() {
                        true => ui.data_mut(|d| {
                            d.insert_temp(text_id, text.clone());
                        }),
                        false => ui.data_mut(|d| d.remove::<String>(text_id)),
                    }
                    if field.lost_focus()
                        && let Some(rgba) = rgba_of(&text)
                    {
                        let rgba = if self.alpha {
                            rgba
                        } else {
                            [rgba[0], rgba[1], rgba[2], live.a]
                        };
                        live = Hsva::from_rgba(rgba, live.h);
                        out = Some(Pick::Chosen(rgba));
                    }
                });
                ui.horizontal(|ui| {
                    if let Some(current) = self.current {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(theme::TARGET), Sense::hover());
                        paint_swatch(ui, rect, current, false);
                        if ui
                            .button("use the current colour")
                            .hint("use the colour you paint with")
                            .clicked()
                        {
                            let rgba = if self.alpha {
                                current
                            } else {
                                [current[0], current[1], current[2], live.a]
                            };
                            out = Some(Pick::Chosen(rgba));
                            Popup::close_id(ui.ctx(), self.id);
                        }
                    }
                    if self.removable && ui.button("remove").hint("take this colour out").clicked() {
                        out = Some(Pick::Remove);
                        Popup::close_id(ui.ctx(), self.id);
                    }
                });
            });
        let base = match out {
            Some(Pick::Dragging(rgba) | Pick::Chosen(rgba)) => rgba,
            _ => self.rgba,
        };
        if Popup::is_id_open(ui.ctx(), self.id) {
            let keep = match out {
                Some(Pick::Chosen(rgba)) => Hsva::from_rgba(rgba, live.h),
                _ => live,
            };
            ui.data_mut(|d| d.insert_temp(live_id, (keep, base)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips_and_keeps_the_hue_of_greys() {
        [
            [1.0, 0.0, 0.0, 1.0],
            [0.2, 0.6, 0.4, 0.5],
            [0.95, 0.9, 0.1, 1.0],
            [0.1, 0.2, 0.9, 1.0],
        ]
        .iter()
        .for_each(|rgba| {
            let back = Hsva::from_rgba(*rgba, 0.0).rgba();
            assert!(
                back.iter().zip(rgba).all(|(a, b)| (a - b).abs() < 1e-5),
                "{rgba:?} {back:?}"
            );
        });
        assert_eq!(Hsva::from_rgba([0.5, 0.5, 0.5, 1.0], 0.3).h, 0.3);
        assert_eq!(hex([1.0, 0.5, 0.0, 1.0]), "#ff8000");
        assert_eq!(hex([0.0, 0.0, 0.0, 0.5]), "#00000080");
        assert_eq!(rgba_of("ff8000"), Some([1.0, 128.0 / 255.0, 0.0, 1.0]));
        assert!(rgba_of("nope").is_none());
    }
}
