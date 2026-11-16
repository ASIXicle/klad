use super::Editor;
use super::clips::resolve;
use crate::jobs::{Spec, Tag};
use crate::theme::Hint;
use anyhow::{Result, bail};
use klad_render::maps::Map;
use klad_render::shaders::Effect;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum What {
    #[default]
    Image,
    Frames,
    Sheet,
    Gif,
    Aseprite,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Form {
    pub what: What,
    pub layer: Option<String>,
    pub scale: f32,
    pub background: bool,
    pub deep: bool,
    pub dither: bool,
    pub variant: String,
    pub range: String,
    pub padding: u32,
    pub max: u32,
    pub out: String,
    pub maps: Vec<Map>,
    pub occluders: bool,
    pub shaders: Vec<Effect>,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            what: What::Image,
            layer: None,
            scale: 1.0,
            background: false,
            deep: false,
            dither: false,
            variant: String::new(),
            range: "all".into(),
            padding: 2,
            max: 4096,
            out: String::new(),
            maps: Vec::new(),
            occluders: false,
            shaders: Vec::new(),
        }
    }
}

fn listed<T>(items: &[T], name: impl Fn(&T) -> &'static str) -> String {
    items.iter().map(name).collect::<Vec<_>>().join(",")
}

pub(super) fn godot_extras(form: &Form) -> Vec<String> {
    let mut out = Vec::new();
    if form.occluders {
        out.push("--occluders".to_string());
    }
    if !form.shaders.is_empty() {
        out.push("--shaders".to_string());
        out.push(listed(&form.shaders, |e| e.name()));
    }
    out
}

fn toggled<T: PartialEq + Copy + Ord>(list: &mut Vec<T>, item: T, on: bool) {
    list.retain(|x| *x != item);
    if on {
        list.push(item);
        list.sort();
    }
}

pub(super) const EVERY: &str = "*";

pub(super) fn args(bundle: &Path, home: &Path, frame: u32, form: &Form) -> Vec<String> {
    let mut args = vec!["export".to_string(), bundle.to_string_lossy().to_string()];
    let mut push = |a: &str, b: Option<String>| {
        args.push(a.to_string());
        args.extend(b);
    };
    match form.what {
        What::Image => push("--frame", Some(frame.to_string())),
        What::Frames => push("--frames", Some(form.range.trim().to_string())),
        What::Sheet => push("--sheet", Some(form.range.trim().to_string())),
        What::Gif => push("--gif", Some(form.range.trim().to_string())),
        What::Aseprite => push("--aseprite", None),
    }
    if let Some(layer) = form
        .layer
        .as_ref()
        .filter(|_| !matches!(form.what, What::Frames | What::Aseprite))
    {
        push("--layer", Some(layer.clone()));
    }
    if form.scale != 1.0 && form.what != What::Aseprite {
        push("--scale", Some(form.scale.to_string()));
    }
    if form.deep && matches!(form.what, What::Image | What::Sheet) {
        push("--depth", Some("16".into()));
    }
    if form.background && matches!(form.what, What::Image | What::Gif) {
        push("--background", None);
    }
    if form.dither && form.what == What::Gif {
        push("--dither", None);
    }
    match form.variant.as_str() {
        "" => {}
        EVERY => push("--variants", None),
        name => push("--variant", Some(name.to_string())),
    }
    if form.what == What::Sheet {
        push("--padding", Some(form.padding.to_string()));
        push("--max", Some(form.max.to_string()));
    }
    if !form.maps.is_empty() && form.layer.is_none() && matches!(form.what, What::Image | What::Frames | What::Sheet) {
        push("--maps", Some(listed(&form.maps, |m| m.name())));
    }
    if !form.out.trim().is_empty() {
        push("-o", Some(resolve(home, &form.out).to_string_lossy().to_string()));
    }
    args
}

fn written(out: &str, err: &str) -> String {
    out.lines()
        .chain(err.lines())
        .map(str::trim)
        .filter(|l| {
            l.ends_with(".png")
                || l.ends_with(".json")
                || l.ends_with(".gif")
                || l.ends_with(".aseprite")
                || l.starts_with("note: ")
                || l.contains(" in all")
        })
        .take(3)
        .collect::<Vec<_>>()
        .join("; ")
}

impl Editor {
    pub(super) fn export(&mut self) -> Result<()> {
        let form = self.export.clone();
        if !matches!(form.what, What::Image | What::Aseprite) && form.range.trim().is_empty() {
            bail!("say which frames: all, a clip's name or a range like 0-11");
        }
        let args = args(&self.scene.bundle.root, &self.bundle_home(), self.scene.frame, &form);
        let name = match form.what {
            What::Image => format!("export frame {}", self.scene.frame),
            What::Frames => format!("export frames {}", form.range.trim()),
            What::Sheet => format!("export the sheet of {}", form.range.trim()),
            What::Gif => format!("export a GIF of {}", form.range.trim()),
            What::Aseprite => "export an Aseprite file".to_string(),
        };
        self.start_job(Spec {
            tag: Tag::Export,
            name,
            exe: PathBuf::new(),
            args,
            env: Vec::new(),
            reruns: 3,
            summary: written,
            failure: |_, err| super::tweens::failure(err),
        });
        Ok(())
    }

    pub(super) fn export_window(&mut self, ui: &mut egui::Ui) -> Result<()> {
        let mut run = false;
        let layers: Vec<(String, String)> = self
            .scene
            .bundle
            .doc
            .all_layers()
            .into_iter()
            .map(|l| (l.id.clone(), l.name.clone()))
            .collect();
        let clips: Vec<String> = self
            .scene
            .bundle
            .doc
            .timeline
            .clips
            .iter()
            .map(|c| c.name.clone())
            .collect();
        let doc = &self.scene.bundle.doc;
        let (pixel, most) = (doc.pixel_art(), klad_render::export::pixel_most(doc));
        let (width, height) = (doc.canvas.width, doc.canvas.height);
        let variants: Vec<String> = doc.variant_names().into_iter().map(String::from).collect();
        let form = &mut self.export;
        if form.variant != EVERY && !form.variant.is_empty() && !variants.contains(&form.variant) {
            form.variant.clear();
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut form.what, What::Image, "image")
                .hint("this frame as one PNG");
            ui.selectable_value(&mut form.what, What::Frames, "frames")
                .hint("numbered PNGs, one per frame");
            ui.selectable_value(&mut form.what, What::Sheet, "sprite sheet")
                .hint("sheet pages and Aseprite JSON");
            ui.selectable_value(&mut form.what, What::Gif, "GIF")
                .hint("an animated GIF: one for each clip, or one of the whole timeline");
            if pixel || form.what == What::Aseprite {
                ui.selectable_value(&mut form.what, What::Aseprite, "Aseprite")
                    .hint("an .aseprite file of the pixel layers: groups, every frame's drawing, the palette, and clips as tags");
            }
        });
        if form.what == What::Aseprite {
            ui.weak("pixel layers and groups only; frames that show the same drawings become one frame");
        }
        if !matches!(form.what, What::Frames | What::Aseprite) {
            let shown = form
                .layer
                .as_ref()
                .and_then(|id| layers.iter().find(|(l, _)| l == id))
                .map_or("the whole image".to_string(), |(_, n)| n.clone());
            egui::ComboBox::from_label("layer")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut form.layer, None, "the whole image");
                    layers.iter().for_each(|(id, name)| {
                        ui.selectable_value(&mut form.layer, Some(id.clone()), name);
                    });
                })
                .response
                .hint("export the whole image or one layer or group");
        }
        if !matches!(form.what, What::Image | What::Aseprite) {
            ui.horizontal(|ui| {
                let label = ui.label("frames");
                ui.add(
                    egui::TextEdit::singleline(&mut form.range)
                        .desired_width(90.0)
                        .hint_text("all, a clip or 0-11"),
                )
                .labelled_by(label.id)
                .hint("which frames: all, a clip's name or a range");
                clips.iter().for_each(|c| {
                    if ui.button(c).hint("export this clip's frames").clicked() {
                        form.range = c.clone();
                    }
                });
            });
        }
        match pixel {
            _ if form.what == What::Aseprite => {}
            true => {
                form.scale = form.scale.round().clamp(1.0, most);
                ui.add(
                    egui::Slider::new(&mut form.scale, 1.0..=most.min(32.0))
                        .step_by(1.0)
                        .fixed_decimals(0)
                        .text("times real size"),
                )
                .hint("pixel art is enlarged by whole numbers only, so every pixel stays a sharp square");
                ui.weak(format!(
                    "{} x {} px",
                    width * form.scale as u32,
                    height * form.scale as u32
                ));
            }
            false => {
                ui.add(
                    egui::Slider::new(&mut form.scale, 0.125..=4.0)
                        .logarithmic(true)
                        .text("scale"),
                )
                .hint("the size of the export against the canvas");
            }
        }
        if matches!(form.what, What::Image | What::Sheet) {
            ui.checkbox(&mut form.deep, "16 bits per channel")
                .hint("16-bit PNGs; 8-bit ones are dithered so gradients do not band");
        }
        if matches!(form.what, What::Image | What::Gif) {
            ui.checkbox(&mut form.background, "keep the background colour")
                .hint("off: a transparent picture");
        }
        if variants.len() > 1 {
            let chosen = match form.variant.as_str() {
                "" => "as shown".to_string(),
                EVERY => "every variant".to_string(),
                name => name.to_string(),
            };
            egui::ComboBox::from_label("palette variant")
                .selected_text(chosen)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut form.variant, String::new(), "as shown");
                    ui.selectable_value(&mut form.variant, EVERY.to_string(), "every variant");
                    variants.iter().for_each(|name| {
                        ui.selectable_value(&mut form.variant, name.clone(), name);
                    });
                })
                .response
                .hint("the colours to export in: what the drawing shows now, one variant, or a file for each (the variant's name is added to the file name)");
        }
        if form.what == What::Gif && !pixel {
            ui.checkbox(&mut form.dither, "dither")
                .hint("a GIF holds 255 colours: mix them in a fine pattern so soft gradients do not band; off keeps flat areas of one colour");
        }
        let lit = doc.maps.is_some();
        if matches!(form.what, What::Image | What::Frames | What::Sheet) {
            ui.horizontal_wrapped(|ui| {
                ui.label("light maps");
                Map::ALL.iter().for_each(|m| {
                    let mut on = form.maps.contains(m);
                    if ui
                        .add_enabled(form.layer.is_none(), egui::Checkbox::new(&mut on, m.name()))
                        .hint(format!(
                            "the {} map beside the picture, the same size, trim and layout, its file name ending in {}",
                            m.name(),
                            m.suffix()
                        ))
                        .on_disabled_hover_text("light maps are made for the whole image: export the whole image")
                        .changed()
                    {
                        toggled(&mut form.maps, *m, on);
                    }
                });
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Godot");
            ui.checkbox(&mut form.occluders, "shadows")
                .hint("the buttons below add an occluder traced from the silhouette of every picture, so lights cast its shadow");
            Effect::ALL.iter().for_each(|e| {
                let mut on = form.shaders.contains(e);
                if ui
                    .add_enabled(lit, egui::Checkbox::new(&mut on, format!("{} shader", e.name())))
                    .hint(match e {
                        Effect::Fog => "a sample shader on the sprite: fog towards a colour by the depth map",
                        Effect::Rim => "a sample shader on the sprite: a rim light from the height map",
                        Effect::Occlusion => "a sample shader on the sprite: darker creases from the occlusion map",
                    })
                    .on_disabled_hover_text("the shaders read the light maps: turn them on in Window > Light maps")
                    .changed()
                {
                    toggled(&mut form.shaders, *e, on);
                }
            });
        });
        if form.what == What::Sheet {
            ui.add(egui::Slider::new(&mut form.padding, 0..=16).text("padding px"))
                .hint("space between the cells of the sheet");
            ui.add(egui::Slider::new(&mut form.max, 512..=8192).text("page size px"))
                .hint("the largest sheet page; more frames go on more pages");
        }
        ui.horizontal(|ui| {
            let label = ui.label("to");
            ui.add(
                egui::TextEdit::singleline(&mut form.out)
                    .desired_width(220.0)
                    .hint_text("next to the drawing; a path relative to its folder"),
            )
            .labelled_by(label.id)
            .hint("the file or folder to write; empty writes next to the drawing");
        });
        ui.horizontal(|ui| {
            run = ui.button("export").hint("write it, as klad export does").clicked();
            if ui
                .button("export to Godot")
                .hint("the sheet, SpriteFrames and scene into the Godot folder of the clip row; with light maps on, lit by Godot's 2D lights, with a sample scene")
                .clicked()
            {
                self.export_godot();
            }
            if ui
                .button("as a parallax scene")
                .hint("each top-level layer a texture with its fog, on a Parallax2D that scrolls by its depth")
                .clicked()
            {
                self.export_godot_as("--godot-scene");
            }
            let rigged = self.scene.bundle.doc.all_layers().iter().any(|l| !l.bones.is_empty());
            if ui
                .add_enabled(rigged, egui::Button::new("as a rig"))
                .hint(
                    "the rest drawing on Polygon2Ds weighted to a Skeleton2D, and the poses of every clip; needs bones",
                )
                .clicked()
            {
                self.export_godot_as("--godot-rig");
            }
            let tiled = self.scene.bundle.doc.canvas.tile.is_some();
            if ui
                .add_enabled(tiled, egui::Button::new("as a tile set"))
                .hint("a Godot TileSet with a tile for every cell that is not empty; needs the drawing cut into tiles (View > Tileset: cut into tiles…)")
                .clicked()
            {
                self.export_godot_as("--tileset");
            }
        });
        if run {
            self.export()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_builds_the_cli_export() {
        let (bundle, home) = (Path::new("/w/art.klad"), Path::new("/w"));
        let image = Form {
            background: true,
            scale: 2.0,
            out: "out/hero.png".into(),
            ..Form::default()
        };
        assert_eq!(
            args(bundle, home, 3, &image).join(" "),
            format!(
                "export /w/art.klad --frame 3 --scale 2 --background -o {}",
                home.join("out/hero.png").display()
            )
        );
        let sheet = Form {
            what: What::Sheet,
            range: "walk".into(),
            layer: Some("l2".into()),
            deep: true,
            ..Form::default()
        };
        assert_eq!(
            args(bundle, home, 0, &sheet).join(" "),
            "export /w/art.klad --sheet walk --layer l2 --depth 16 --padding 2 --max 4096"
        );
        let frames = Form {
            what: What::Frames,
            range: "0-11".into(),
            layer: Some("l2".into()),
            scale: 0.5,
            ..Form::default()
        };
        assert_eq!(
            args(bundle, home, 0, &frames).join(" "),
            "export /w/art.klad --frames 0-11 --scale 0.5"
        );
        let gif = Form {
            what: What::Gif,
            range: "hop".into(),
            scale: 8.0,
            deep: true,
            background: true,
            dither: true,
            ..Form::default()
        };
        assert_eq!(
            args(bundle, home, 0, &gif).join(" "),
            "export /w/art.klad --gif hop --scale 8 --background --dither"
        );
        let file = Form {
            what: What::Aseprite,
            scale: 4.0,
            layer: Some("l2".into()),
            range: String::new(),
            variant: "blue".into(),
            ..Form::default()
        };
        assert_eq!(
            args(bundle, home, 0, &file).join(" "),
            "export /w/art.klad --aseprite --variant blue"
        );
        assert_eq!(
            written(
                "/w/art.aseprite\n",
                "16x12, 1 layers, 2 frames\nnote: left out, since an Aseprite file holds only pixel layers: notes\n"
            ),
            "/w/art.aseprite; note: left out, since an Aseprite file holds only pixel layers: notes"
        );
        let every = Form {
            what: What::Sheet,
            variant: EVERY.into(),
            ..Form::default()
        };
        assert_eq!(
            args(bundle, home, 0, &every).join(" "),
            "export /w/art.klad --sheet all --variants --padding 2 --max 4096"
        );
        let blue = Form {
            variant: "blue".into(),
            ..Form::default()
        };
        assert_eq!(
            args(bundle, home, 2, &blue).join(" "),
            "export /w/art.klad --frame 2 --variant blue"
        );
        let image = Form {
            dither: true,
            ..Form::default()
        };
        assert!(!args(bundle, home, 0, &image).contains(&"--dither".to_string()));
        assert_eq!(
            written(
                "/w/art-hop@8x.gif\n",
                "128x96  4 frames (2 pictures), 500 ms, loops  0.5 KB\n"
            ),
            "/w/art-hop@8x.gif"
        );
        let lit = Form {
            what: What::Sheet,
            maps: vec![Map::Normal, Map::Depth],
            ..Form::default()
        };
        assert_eq!(
            args(bundle, home, 0, &lit).join(" "),
            "export /w/art.klad --sheet all --padding 2 --max 4096 --maps normal,depth"
        );
        let one = Form {
            layer: Some("l2".into()),
            ..lit.clone()
        };
        assert!(
            !args(bundle, home, 0, &one).contains(&"--maps".to_string()),
            "maps go with the whole image"
        );
        let godot = Form {
            occluders: true,
            shaders: vec![Effect::Fog, Effect::Occlusion],
            ..Form::default()
        };
        assert_eq!(godot_extras(&godot).join(" "), "--occluders --shaders fog,occlusion");
        assert!(godot_extras(&Form::default()).is_empty());
    }
}
