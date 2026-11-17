use super::Editor;
use crate::home::Home;
use crate::messages::Tone;
use crate::settings::Settings;
use egui::accesskit::Role;
use egui::{Key, Modifiers, Pos2, Vec2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(super) fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    std::fs::read_dir(from).unwrap().for_each(|e| {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        match e.file_type().unwrap().is_dir() {
            true if e.file_name() == ".cache" => {}
            true => copy_dir(&e.path(), &target),
            false => {
                std::fs::copy(e.path(), target).unwrap();
            }
        }
    });
}

pub(super) fn released(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/formats/0.2.0")
        .join(name)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("klad-journey-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

pub(super) fn written(node: &egui_kittest::kittest::AccessKitNode<'_>) -> Option<String> {
    match node.role() {
        Role::Label => node.value(),
        _ => node.label(),
    }
    .filter(|text| !text.is_empty())
}

fn tone(tone: Tone) -> &'static str {
    match tone {
        Tone::Info => "info",
        Tone::Done => "done",
        Tone::Refused => "refused",
        Tone::Error => "error",
    }
}

pub(super) struct Journey {
    pub(super) h: Harness<'static, Home>,
    pub(super) folder: PathBuf,
    out: Option<PathBuf>,
    step: usize,
    heard: u64,
    lines: Vec<String>,
    misses: Vec<String>,
}

impl Journey {
    pub(super) fn begin(name: &str, size: Vec2, open: Option<&str>) -> Self {
        let folder = scratch(name);
        let out = std::env::var_os("KLAD_JOURNEY_OUT")
            .filter(|dir| !dir.is_empty())
            .map(|dir| PathBuf::from(dir).join(name));
        if let Some(out) = &out {
            let _ = std::fs::remove_dir_all(out);
            std::fs::create_dir_all(out).unwrap();
        }
        let gpu = out.as_ref().map(|_| {
            egui_kittest::wgpu::create_render_state(
                egui_kittest::wgpu::default_wgpu_setup(),
                egui_wgpu::RendererOptions::PREDICTABLE,
            )
        });
        let settings = Settings {
            drawings: Some(folder.display().to_string()),
            check_updates: false,
            ..Settings::default()
        };
        let mut home = Home::with(settings, None, gpu.clone());
        let opened = open.map(|sample| {
            let copy = folder.join(sample);
            copy_dir(&released(sample), &copy);
            copy
        });
        home.begin(&egui::Context::default(), opened.as_deref());
        let builder = Harness::builder().with_size(size).with_step_dt(1.0 / 30.0);
        let builder = match gpu {
            Some(gpu) => builder.renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(gpu)),
            None => builder,
        };
        let h = builder.build_ui_state(|ui, home: &mut Home| home.frame(ui), home);
        let mut journey = Self {
            h,
            folder,
            out,
            step: 0,
            heard: 0,
            lines: vec![format!("# {name}"), String::new()],
            misses: Vec::new(),
        };
        journey.h.run_steps(4);
        journey
    }

    pub(super) fn ed(&self) -> Option<&Editor> {
        self.h.state().editor.as_deref()
    }

    pub(super) fn ed_mut(&mut self) -> Option<&mut Editor> {
        self.h.state_mut().editor.as_deref_mut()
    }

    fn pass(&mut self) {
        self.h.run_steps(1);
        if self.out.is_some() {
            let _ = self.h.render();
        }
    }

    pub(super) fn settle(&mut self) {
        let started = Instant::now();
        let waiting = |ed: &Editor| !ed.canvas_current() || !ed.jobs.is_empty() || ed.preview.is_some();
        self.pass();
        while self.ed().is_some_and(waiting) && started.elapsed() < Duration::from_secs(60) {
            self.pass();
            std::thread::sleep(Duration::from_millis(5));
        }
        if let Some(ed) = self.ed_mut() {
            ed.redraw_canvas();
        }
        (0..6).for_each(|_| self.pass());
    }

    fn said(&mut self) -> Vec<String> {
        let heard = self.heard;
        let fresh: Vec<(u64, String)> = self
            .ed()
            .map(|ed| {
                ed.messages
                    .log()
                    .filter(|m| m.id > heard)
                    .map(|m| (m.id, format!("[{}] {}", tone(m.tone), m.text.replace('\n', " / "))))
                    .collect()
            })
            .unwrap_or_default();
        self.heard = fresh.iter().map(|(id, _)| *id).max().unwrap_or(heard);
        fresh.into_iter().map(|(_, text)| text).collect()
    }

    fn state(&self) -> String {
        let home = self.h.state();
        match (&home.editor, &home.start) {
            (Some(ed), _) => {
                let layer = ed.scene.active_layer().map_or_else(
                    || "no layer".to_string(),
                    |l| {
                        format!(
                            "{} ({}{})",
                            l.name,
                            l.kind_name(),
                            if l.pixel() { ", pixels" } else { "" }
                        )
                    },
                );
                let view = ed.view.map_or_else(String::new, |v| {
                    format!(
                        "zoom {:.2}, turned {:.0}{}",
                        v.zoom,
                        v.angle.to_degrees(),
                        if v.flip { ", flipped" } else { "" }
                    )
                });
                let picked = ed.picked.as_ref().map_or(0, |p| p.ids.len());
                let shape = ed.selection.clone().unwrap_or_else(|| "none".into());
                let doc = &ed.scene.bundle.doc;
                format!(
                    "editor on {}: tool {:?}, layer {layer}, frame {} of {}, {} marks here, {} in all, {} layers, strokes picked {picked}, shape picked {shape}, {view}, size {:.1}, colour #{}",
                    ed.scene.bundle.root.file_name().unwrap_or_default().to_string_lossy(),
                    ed.tool,
                    ed.scene.frame,
                    doc.timeline.frames,
                    ed.scene.active_marks().len(),
                    ed.scene.total_marks(),
                    doc.layers.len(),
                    ed.size,
                    ed.color[..3]
                        .iter()
                        .map(|c| format!("{:02x}", (c.clamp(0.0, 1.0) * 255.0).round() as u8))
                        .collect::<String>(),
                )
            }
            (None, Some(start)) => format!(
                "start window{}",
                start
                    .error
                    .as_ref()
                    .map_or_else(String::new, |e| format!(", error: {e}"))
            ),
            (None, None) => "nothing open".to_string(),
        }
    }

    fn screen(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        self.h
            .query_all(egui_kittest::kittest::By::new().predicate(|n| written(n).is_some()))
            .filter_map(|n| {
                let node = n.accesskit_node();
                let label = written(&node)?;
                let text = match node.role() {
                    Role::Label => label,
                    role => format!("{role:?} \"{label}\""),
                };
                seen.insert(text.clone()).then_some(text)
            })
            .take(400)
            .collect()
    }

    pub(super) fn act(&mut self, what: &str) {
        self.step += 1;
        self.lines.push(String::new());
        self.lines.push(format!("## {}. {what}", self.step));
    }

    pub(super) fn note(&mut self, text: impl AsRef<str>) {
        self.lines.push(format!("- {}", text.as_ref()));
    }

    pub(super) fn look(&mut self, name: &str) {
        self.settle();
        self.said()
            .into_iter()
            .for_each(|m| self.lines.push(format!("- said {m}")));
        let state = self.state();
        self.lines.push(format!("- now: {state}"));
        if let Some(out) = self.out.clone() {
            let screen = self.screen().join(" | ");
            self.lines.push(format!("- on screen: {screen}"));
            let file = format!("{:02}-{name}.png", self.step);
            match self.h.render() {
                Ok(image) => {
                    image.save(out.join(&file)).unwrap();
                    self.lines.push(format!("- picture: {file}"));
                }
                Err(e) => self.lines.push(format!("- no picture: {e}")),
            }
        }
    }

    pub(super) fn expect(&mut self, key: &str, ok: bool, what: &str) {
        self.lines
            .push(format!("- {} {what} `{key}`", if ok { "ok:" } else { "MISS:" }));
        if !ok {
            self.misses.push(key.to_string());
        }
    }

    pub(super) fn key(&mut self, key: Key) {
        self.note(format!("pressed {}", key.name()));
        self.h.key_press(key);
        self.h.run_steps(3);
    }

    pub(super) fn chord(&mut self, modifiers: Modifiers, key: Key) {
        self.note(format!(
            "pressed {}{}{}{}",
            if modifiers.command { "cmd " } else { "" },
            if modifiers.shift { "shift " } else { "" },
            if modifiers.alt { "alt " } else { "" },
            key.name()
        ));
        self.h.key_press_modifiers(modifiers, key);
        self.h.run_steps(3);
    }

    pub(super) fn shown(&self, label: &str) -> bool {
        self.h.query_all_by_label_contains(label).next().is_some()
    }

    pub(super) fn click(&mut self, label: &str) -> bool {
        let found = self.h.query_all_by_label(label).next().map(|n| n.click()).is_some();
        self.note(match found {
            true => format!("clicked \"{label}\""),
            false => format!("looked for \"{label}\" to click: not on screen"),
        });
        self.h.run_steps(3);
        found
    }

    pub(super) fn click_containing(&mut self, label: &str) -> bool {
        let found = self
            .h
            .query_all_by_label_contains(label)
            .find(|n| n.accesskit_node().role() != Role::Window)
            .map(|n| n.click())
            .is_some();
        self.note(match found {
            true => format!("clicked the control saying \"{label}\""),
            false => format!("looked for a control saying \"{label}\": not on screen"),
        });
        self.h.run_steps(3);
        found
    }

    pub(super) fn menu(&mut self, menu: &str, sub: &str, item: &str) -> bool {
        if let Some(n) = self.h.query_all_by_label(menu).next() {
            n.click();
        }
        self.h.run_steps(2);
        let middle = self
            .h
            .query_all_by_label_contains(sub)
            .find(|n| n.accesskit_node().role() != Role::Window)
            .map(|n| n.rect().center());
        if let Some(at) = middle {
            self.h.hover_at(at);
            self.h.run_steps(3);
        }
        let found = self
            .h
            .query_all_by_label(item)
            .find(|n| !n.accesskit_node().is_disabled())
            .map(|n| n.click())
            .is_some();
        self.note(match found {
            true => format!("chose {menu} > {sub} > {item}"),
            false => format!("looked for {menu} > {sub} > {item}: not there, or greyed out"),
        });
        self.h.run_steps(3);
        if !found {
            self.h.key_press(Key::Escape);
            self.h.run_steps(2);
        }
        found
    }

    pub(super) fn double_click(&mut self, label: &str) -> bool {
        let at = self.h.query_all_by_label(label).next().map(|n| n.rect().center());
        self.note(match at {
            Some(_) => format!("double clicked \"{label}\""),
            None => format!("looked for \"{label}\" to double click: not on screen"),
        });
        if let Some(at) = at {
            self.h.hover_at(at);
            self.h.run_steps(1);
            (0..2).for_each(|_| {
                self.press(at, egui::PointerButton::Primary, true, Modifiers::NONE);
                self.press(at, egui::PointerButton::Primary, false, Modifiers::NONE);
            });
            self.h.run_steps(3);
        }
        at.is_some()
    }

    pub(super) fn type_text(&mut self, text: &str) {
        self.note(format!("typed \"{text}\""));
        self.h.event(egui::Event::Text(text.to_string()));
        self.h.run_steps(2);
    }

    pub(super) fn type_into(&mut self, label: &str, text: &str) -> bool {
        let found = self
            .h
            .query_all_by_label(label)
            .find(|n| matches!(n.accesskit_node().role(), Role::TextInput | Role::MultilineTextInput))
            .map(|n| {
                n.focus();
                n.type_text(text);
            })
            .is_some();
        self.note(match found {
            true => format!("typed \"{text}\" into \"{label}\""),
            false => format!("looked for a field \"{label}\" to type in: not on screen"),
        });
        self.h.run_steps(3);
        found
    }

    pub(super) fn retype(&mut self, label: &str, text: &str) -> bool {
        let found = self
            .h
            .query_all_by_label(label)
            .find(|n| matches!(n.accesskit_node().role(), Role::TextInput | Role::MultilineTextInput))
            .map(|n| n.click())
            .is_some();
        self.h.run_steps(2);
        if found {
            self.h.key_press_modifiers(Modifiers::COMMAND, Key::A);
            self.h.run_steps(1);
            self.h.event(egui::Event::Text(text.to_string()));
            self.h.run_steps(1);
            self.h.key_press(Key::Enter);
        }
        self.note(match found {
            true => format!("clicked into \"{label}\", selected all, typed \"{text}\" and pressed Return"),
            false => format!("looked for a field \"{label}\" to type in: not on screen"),
        });
        self.h.run_steps(3);
        found
    }

    pub(super) fn click_inside(&mut self, label: &str, across: f32, down: f32) -> bool {
        let rect = self.h.query_all_by_label(label).next().map(|n| n.rect());
        self.note(match rect {
            Some(_) => format!(
                "clicked \"{label}\" at {:.0}% across, {:.0}% down",
                across * 100.0,
                down * 100.0
            ),
            None => format!("looked for \"{label}\" to click: not on screen"),
        });
        if let Some(rect) = rect {
            let at = rect.min + vec2(rect.width() * across, rect.height() * down);
            self.h.hover_at(at);
            self.h.run_steps(1);
            self.press(at, egui::PointerButton::Primary, true, Modifiers::NONE);
            self.press(at, egui::PointerButton::Primary, false, Modifiers::NONE);
            self.h.run_steps(3);
        }
        rect.is_some()
    }

    pub(super) fn at(&self, x: f32, y: f32) -> Pos2 {
        let ed = self.ed().expect("a drawing is open");
        let rect = ed.canvas_rect;
        rect.min
            + ed.view
                .expect("the canvas has a view")
                .to_screen(vec2(x, y), rect.size())
    }

    fn press(&mut self, at: Pos2, button: egui::PointerButton, pressed: bool, modifiers: Modifiers) {
        self.h.event(egui::Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers,
        });
        self.h.run_steps(1);
    }

    pub(super) fn drag_with(&mut self, points: &[(f32, f32)], modifiers: Modifiers) {
        let screen: Vec<Pos2> = points.iter().map(|(x, y)| self.at(*x, *y)).collect();
        self.note(format!(
            "dragged on the canvas from {:.0},{:.0} to {:.0},{:.0}{}",
            points[0].0,
            points[0].1,
            points[points.len() - 1].0,
            points[points.len() - 1].1,
            if modifiers.is_none() {
                ""
            } else {
                " with a modifier held"
            }
        ));
        self.h.event(egui::Event::ModifiersChanged(modifiers));
        self.h.hover_at(screen[0]);
        self.h.run_steps(1);
        self.press(screen[0], egui::PointerButton::Primary, true, modifiers);
        screen.windows(2).for_each(|pair| {
            (1..=6).for_each(|i| {
                self.h.hover_at(pair[0] + (pair[1] - pair[0]) * (i as f32 / 6.0));
                self.h.run_steps(1);
            });
        });
        self.press(screen[screen.len() - 1], egui::PointerButton::Primary, false, modifiers);
        self.h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
        self.h.run_steps(3);
    }

    pub(super) fn drag(&mut self, points: &[(f32, f32)]) {
        self.drag_with(points, Modifiers::NONE);
    }

    pub(super) fn tap_with(&mut self, x: f32, y: f32, button: egui::PointerButton, modifiers: Modifiers) {
        let at = self.at(x, y);
        self.note(format!(
            "{} the canvas at {x:.0},{y:.0}",
            match button {
                egui::PointerButton::Secondary => "right clicked",
                _ => "clicked",
            }
        ));
        self.h.event(egui::Event::ModifiersChanged(modifiers));
        self.h.hover_at(at);
        self.h.run_steps(1);
        self.press(at, button, true, modifiers);
        self.press(at, button, false, modifiers);
        self.h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
        self.h.run_steps(3);
    }

    pub(super) fn hover_with(&mut self, x: f32, y: f32, modifiers: Modifiers) {
        let at = self.at(x, y);
        self.note(format!(
            "moved the pointer to {x:.0},{y:.0}{}",
            if modifiers.is_none() {
                ""
            } else {
                " with a modifier held"
            }
        ));
        self.h.event(egui::Event::ModifiersChanged(modifiers));
        self.h.hover_at(at);
        self.h.run_steps(2);
    }

    pub(super) fn let_go(&mut self) {
        self.h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
        self.h.run_steps(2);
    }

    pub(super) fn tap(&mut self, x: f32, y: f32) {
        self.tap_with(x, y, egui::PointerButton::Primary, Modifiers::NONE);
    }

    fn cell(&self, layer: &str, frame: u32) -> Option<Pos2> {
        let ed = self.ed()?;
        let row = ed
            .layer_rows(super::layers::Where::Timeline)
            .iter()
            .position(|r| r.name == layer)?;
        ed.timeline.geom.map(|g| g.cell_centre(frame, row))
    }

    pub(super) fn drag_frames(&mut self, layer: &str, from: u32, to: u32) -> bool {
        let (Some(a), Some(b)) = (self.cell(layer, from), self.cell(layer, to)) else {
            self.note(format!(
                "looked for frames {from} to {to} of {layer} in the timeline: not in sight"
            ));
            return false;
        };
        self.note(match from == to {
            true => format!("clicked frame {from} of {layer} in the timeline"),
            false => format!("dragged over frames {from} to {to} of {layer} in the timeline"),
        });
        self.h.hover_at(a);
        self.h.run_steps(1);
        self.press(a, egui::PointerButton::Primary, true, Modifiers::NONE);
        (1..=6).for_each(|i| {
            self.h.hover_at(a + (b - a) * (i as f32 / 6.0));
            self.h.run_steps(1);
        });
        self.press(b, egui::PointerButton::Primary, false, Modifiers::NONE);
        self.h.run_steps(3);
        true
    }

    pub(super) fn twist(&mut self, x: f32, y: f32, degrees: f32, modifiers: Modifiers) {
        let at = self.at(x, y);
        self.note(format!(
            "twisted two fingers {degrees:.0} degrees over the canvas{}",
            if modifiers.is_none() { "" } else { " with shift held" }
        ));
        self.h.event(egui::Event::ModifiersChanged(modifiers));
        self.h.hover_at(at);
        self.h.event(egui::Event::Rotate(degrees.to_radians()));
        self.h.run_steps(2);
        self.h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
        self.h.run_steps(2);
    }

    pub(super) fn heard(&self) -> String {
        self.ed()
            .and_then(|ed| ed.messages.latest().map(|m| m.text.clone()))
            .unwrap_or_default()
    }

    pub(super) fn wheel(&mut self, x: f32, y: f32, delta: Vec2, modifiers: Modifiers) {
        let at = self.at(x, y);
        self.note(format!(
            "scrolled by {:.0},{:.0} over the canvas{}",
            delta.x,
            delta.y,
            if modifiers.command { " with cmd held" } else { "" }
        ));
        self.h.event(egui::Event::ModifiersChanged(modifiers));
        self.h.event(egui::Event::PointerMoved(at));
        self.h.run_steps(1);
        self.h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta,
            modifiers,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
        });
        self.h.run_steps(2);
        self.h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
        self.h.run_steps(4);
    }

    pub(super) fn pinch(&mut self, x: f32, y: f32, factor: f32) {
        let at = self.at(x, y);
        self.note(format!("pinched by {factor:.2} over the canvas"));
        self.h.event(egui::Event::PointerMoved(at));
        self.h.run_steps(1);
        self.h.event(egui::Event::Zoom(factor));
        self.h.run_steps(4);
    }

    pub(super) fn finish(mut self, known: &[&str]) {
        let found: BTreeSet<String> = self.misses.iter().cloned().collect();
        let known: BTreeSet<String> = known.iter().map(|k| k.to_string()).collect();
        let new: Vec<&String> = found.difference(&known).collect();
        let gone: Vec<&String> = known.difference(&found).collect();
        self.lines.push(String::new());
        self.lines.push(format!(
            "misses: {}",
            found.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
        if let Some(out) = &self.out {
            std::fs::write(out.join("steps.md"), self.lines.join("\n") + "\n").unwrap();
        }
        let _ = std::fs::remove_dir_all(&self.folder);
        assert!(
            new.is_empty() && gone.is_empty(),
            "the journey met problems it did not list: {new:?}; listed problems it no longer meets (take them off its list): {gone:?}"
        );
    }
}
