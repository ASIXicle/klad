use crate::export::{dither8, f16_pixels, png_of_f16};
use anyhow::{Result, bail};
use klad_core::doc::{Clip, Document};
use klad_core::rng::{FNV_OFFSET, fnv};
use serde_json::{Value, json};
use skia_safe::{AlphaType, Image};

#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub name: String,
    pub from: u32,
    pub to: u32,
    pub fps: u32,
    pub looping: bool,
    pub clip: bool,
}

pub fn animations(doc: &Document, spec: &str) -> Result<Vec<Animation>> {
    let t = &doc.timeline;
    let last = t.frames - 1;
    let of_clip = |c: &Clip| Animation {
        name: c.name.clone(),
        from: c.from,
        to: c.to.min(last),
        fps: t.fps_of(c),
        looping: c.looping,
        clip: true,
    };
    let whole = |name: &str, from: u32, to: u32| Animation {
        name: name.to_string(),
        from,
        to,
        fps: t.fps,
        looping: true,
        clip: false,
    };
    if spec == "all" {
        return Ok(match t.clips.is_empty() {
            true => vec![whole("default", 0, last)],
            false => t.clips.iter().map(of_clip).collect(),
        });
    }
    if let Some(c) = t.clip(spec) {
        return Ok(vec![of_clip(c)]);
    }
    let (from, to) = crate::sheet::range(doc, spec)?;
    Ok(vec![whole(spec, from, to)])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub picture: usize,
    pub frame: u32,
    pub frames: u32,
    pub ms: u32,
}

fn ms_at(frames: u32, fps: u32) -> u64 {
    (frames as u64 * 2000 + fps as u64) / (2 * fps as u64)
}

pub fn runs(anim: &Animation, picture: impl Fn(u32) -> usize) -> Vec<Run> {
    let grouped = (anim.from..=anim.to).fold(Vec::<Run>::new(), |mut out, f| {
        let p = picture(f);
        match out.last_mut() {
            Some(run) if run.picture == p => run.frames += 1,
            _ => out.push(Run {
                picture: p,
                frame: f,
                frames: 1,
                ms: 0,
            }),
        }
        out
    });
    grouped
        .into_iter()
        .map(|run| {
            let start = run.frame - anim.from;
            Run {
                ms: (ms_at(start + run.frames, anim.fps) - ms_at(start, anim.fps)) as u32,
                ..run
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Data {
    Rgba8(Vec<u8>),
    Rgba16(Vec<half::f16>),
    Grey16(Vec<u16>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub data: Data,
    pub linear: bool,
}

impl Pixels {
    pub fn of(image: &Image, depth: u8) -> Result<Self> {
        let data = match depth {
            8 => Data::Rgba8(dither8(image)?),
            16 => Data::Rgba16(f16_pixels(image, AlphaType::Unpremul)?),
            other => bail!("--depth takes 8 or 16, not {other}"),
        };
        Ok(Self {
            width: image.width() as u32,
            height: image.height() as u32,
            data,
            linear: false,
        })
    }

    pub fn of_map(set: &crate::maps::MapSet, map: crate::maps::Map) -> Self {
        let values = set.straight(map);
        let data = match map.wide() {
            true => Data::Grey16(
                values
                    .iter()
                    .flat_map(|p| [p[0], p[3]].map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16))
                    .collect(),
            ),
            false => Data::Rgba8(
                values
                    .iter()
                    .flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
                    .collect(),
            ),
        };
        Self {
            width: set.width as u32,
            height: set.height as u32,
            data,
            linear: true,
        }
    }

    pub fn blank(width: u32, height: u32, depth: u8) -> Self {
        let n = width as usize * height as usize * 4;
        let data = match depth {
            16 => Data::Rgba16(vec![half::f16::ZERO; n]),
            _ => Data::Rgba8(vec![0; n]),
        };
        Self {
            width,
            height,
            data,
            linear: false,
        }
    }

    pub fn blank_like(&self, width: u32, height: u32) -> Self {
        let n = width as usize * height as usize;
        let data = match &self.data {
            Data::Rgba8(_) => Data::Rgba8(vec![0; n * 4]),
            Data::Rgba16(_) => Data::Rgba16(vec![half::f16::ZERO; n * 4]),
            Data::Grey16(_) => Data::Grey16(vec![0; n * 2]),
        };
        Self {
            width,
            height,
            data,
            linear: self.linear,
        }
    }

    pub fn depth(&self) -> u8 {
        match self.data {
            Data::Rgba8(_) => 8,
            Data::Rgba16(_) | Data::Grey16(_) => 16,
        }
    }

    fn channels(&self) -> usize {
        match self.data {
            Data::Grey16(_) => 2,
            _ => 4,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        match &self.data {
            Data::Rgba8(d) => d,
            Data::Rgba16(d) => bytemuck::cast_slice(d),
            Data::Grey16(d) => bytemuck::cast_slice(d),
        }
    }

    pub fn alpha(&self, x: u32, y: u32) -> f32 {
        let c = self.channels();
        let at = (y as usize * self.width as usize + x as usize) * c + c - 1;
        match &self.data {
            Data::Rgba8(d) => d[at] as f32 / 255.0,
            Data::Rgba16(d) => d[at].to_f32(),
            Data::Grey16(d) => d[at] as f32 / 65535.0,
        }
    }

    fn covered(&self, x: u32, y: u32) -> bool {
        self.alpha(x, y) > 0.0
    }

    pub fn trim(&self) -> Rect {
        let rows: Vec<u32> = (0..self.height)
            .filter(|&y| (0..self.width).any(|x| self.covered(x, y)))
            .collect();
        let (Some(&top), Some(&bottom)) = (rows.first(), rows.last()) else {
            return Rect { x: 0, y: 0, w: 1, h: 1 };
        };
        let columns = |y: u32| (0..self.width).filter(move |&x| self.covered(x, y));
        let left = rows.iter().filter_map(|&y| columns(y).next()).min().unwrap_or(0);
        let right = rows.iter().filter_map(|&y| columns(y).next_back()).max().unwrap_or(0);
        Rect {
            x: left,
            y: top,
            w: right - left + 1,
            h: bottom - top + 1,
        }
    }

    pub fn hash(&self) -> u64 {
        let dims = [self.width, self.height].map(u32::to_le_bytes).concat();
        fnv(fnv(FNV_OFFSET, &dims), self.bytes())
    }

    pub fn blit(&mut self, from: &Pixels, src: Rect, x: u32, y: u32) {
        let c = self.channels();
        let (fw, tw) = (from.width as usize * c, self.width as usize * c);
        let row = src.w as usize * c;
        (0..src.h as usize).for_each(|r| {
            let a = (src.y as usize + r) * fw + src.x as usize * c;
            let b = (y as usize + r) * tw + x as usize * c;
            match (&mut self.data, &from.data) {
                (Data::Rgba8(to), Data::Rgba8(d)) => to[b..b + row].copy_from_slice(&d[a..a + row]),
                (Data::Rgba16(to), Data::Rgba16(d)) => to[b..b + row].copy_from_slice(&d[a..a + row]),
                (Data::Grey16(to), Data::Grey16(d)) => to[b..b + row].copy_from_slice(&d[a..a + row]),
                _ => {}
            }
        });
    }

    pub fn crop(&self, rect: Rect) -> Pixels {
        let mut out = self.blank_like(rect.w, rect.h);
        out.blit(self, rect, 0, 0);
        out
    }

    pub fn png(&mut self) -> Result<Vec<u8>> {
        let (w, h) = (self.width, self.height);
        Ok(match (&self.data, self.linear) {
            (Data::Rgba8(d), false) => crate::pngwrite::write(w, h, 8, d),
            (Data::Rgba8(d), true) => crate::pngwrite::data(w, h, 8, 4, d),
            (Data::Rgba16(d), _) => png_of_f16(w, h, d),
            (Data::Grey16(d), _) => {
                let wide: Vec<u8> = d.iter().flat_map(|v| v.to_be_bytes()).collect();
                crate::pngwrite::data(w, h, 16, 2, &wide)
            }
        })
    }
}

pub type Layout = (Vec<Placed>, Vec<(u32, u32)>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub page: usize,
    pub x: u32,
    pub y: u32,
}

fn shelves(sizes: &[(u32, u32)], p: u32, width: u32, max: u32) -> Layout {
    let mut pages = vec![(0u32, 0u32)];
    let (mut x, mut y, mut row) = (p, p, 0);
    let mut placed = Vec::with_capacity(sizes.len());
    for &(w, h) in sizes {
        if x + w + p > width {
            (x, y, row) = (p, y + row + p, 0);
        }
        if y + h + p > max {
            pages.push((0, 0));
            (x, y, row) = (p, p, 0);
        }
        let page = pages.len() - 1;
        placed.push(Placed { page, x, y });
        x += w + p;
        row = row.max(h);
        let size = &mut pages[page];
        *size = (size.0.max(x), size.1.max(y + h + p));
    }
    (placed, pages)
}

pub fn pack(sizes: &[(u32, u32)], padding: u32, max: u32) -> Result<Layout> {
    let p = padding;
    if let Some((i, (w, h))) = sizes
        .iter()
        .enumerate()
        .find(|(_, (w, h))| w + 2 * p > max || h + 2 * p > max)
    {
        bail!(
            "picture {i} is {w}x{h} after trimming and does not fit a {max} px page with {p} px padding; export at a smaller --scale or raise --max"
        );
    }
    let area: u64 = sizes.iter().map(|&(w, h)| (w + p) as u64 * (h + p) as u64).sum();
    let widest = sizes.iter().map(|s| s.0).max().unwrap_or(1) + 2 * p;
    let base = ((area as f64).sqrt().ceil() as u32 + p).max(widest);
    let tries: Vec<u32> = (0..=10)
        .map(|k| (base as f64 * (1.0 + k as f64 / 10.0)).ceil() as u32)
        .map(|w| w.min(max))
        .collect();
    let best = tries
        .iter()
        .map(|&w| shelves(sizes, p, w, max))
        .min_by_key(|(_, pages)| {
            let last = pages.last().copied().unwrap_or_default();
            (pages.len(), last.0.max(last.1), last.0 as u64 * last.1 as u64)
        })
        .unwrap_or_default();
    Ok(best)
}

pub struct Sheet<'a> {
    pub stem: &'a str,
    pub source: (u32, u32),
    pub trims: &'a [Rect],
    pub placed: &'a [Placed],
    pub pages: &'a [(u32, u32)],
    pub images: &'a [String],
    pub animations: &'a [(Animation, Vec<Run>)],
    pub scale: f32,
    pub depth: u8,
}

fn rect_json(r: Rect) -> Value {
    json!({ "x": r.x, "y": r.y, "w": r.w, "h": r.h })
}

pub fn aseprite(sheet: &Sheet) -> Value {
    let (sw, sh) = sheet.source;
    let mut frames: Vec<Value> = Vec::new();
    let mut tags: Vec<Value> = Vec::new();
    sheet.animations.iter().for_each(|(anim, runs)| {
        let first = frames.len();
        frames.extend(runs.iter().map(|run| {
            let (trim, at) = (sheet.trims[run.picture], sheet.placed[run.picture]);
            let prefix = if anim.clip { anim.name.as_str() } else { sheet.stem };
            let mut entry = json!({
                "filename": format!("{prefix} {}", run.frame),
                "frame": rect_json(Rect { x: at.x, y: at.y, ..trim }),
                "rotated": false,
                "trimmed": trim != Rect { x: 0, y: 0, w: sw, h: sh },
                "spriteSourceSize": rect_json(trim),
                "sourceSize": { "w": sw, "h": sh },
                "duration": run.ms,
            });
            if at.page > 0 {
                entry["image"] = json!(sheet.images[at.page]);
            }
            entry
        }));
        if anim.clip && frames.len() > first {
            let mut tag = json!({
                "name": anim.name,
                "from": first,
                "to": frames.len() - 1,
                "direction": "forward",
                "color": "#000000ff",
            });
            if !anim.looping {
                tag["repeat"] = json!("1");
            }
            tags.push(tag);
        }
    });
    let scale = format!("{:.3}", sheet.scale);
    let mut meta = json!({
        "app": "klad",
        "version": env!("CARGO_PKG_VERSION"),
        "image": sheet.images[0],
        "format": if sheet.depth == 16 { "RGBA16161616" } else { "RGBA8888" },
        "size": { "w": sheet.pages[0].0, "h": sheet.pages[0].1 },
        "scale": scale.trim_end_matches('0').trim_end_matches('.'),
        "frameTags": tags,
        "layers": [],
        "slices": [],
    });
    if sheet.pages.len() > 1 {
        meta["images"] = sheet
            .images
            .iter()
            .zip(sheet.pages)
            .map(|(image, (w, h))| json!({ "image": image, "size": { "w": w, "h": h } }))
            .collect();
    }
    json!({ "frames": frames, "meta": meta })
}

pub fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn number(v: f32) -> String {
    let text = format!("{v:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    match text {
        "-0" => "0".into(),
        t => t.into(),
    }
}

fn float(v: f32) -> String {
    let text = number(v);
    match text.contains('.') {
        true => text,
        false => format!("{text}.0"),
    }
}

fn rect2(r: [f32; 4]) -> String {
    format!("Rect2({})", r.map(number).join(", "))
}

pub struct Texture {
    pub path: String,
    pub uid: Option<String>,
}

pub struct Lighting {
    pub normal: Texture,
    pub specular: Option<Texture>,
}

pub struct SpriteFrames<'a> {
    pub uid: &'a str,
    pub textures: &'a [Texture],
    pub source: (u32, u32),
    pub trims: &'a [Rect],
    pub placed: &'a [Placed],
    pub animations: &'a [(Animation, Vec<Run>)],
    pub lighting: &'a [Lighting],
    pub shininess: f32,
    pub nearest: bool,
}

fn texture_line(t: &Texture, id: &str) -> String {
    let uid = t
        .uid
        .as_deref()
        .map_or(String::new(), |u| format!(" uid={}", quoted(u)));
    format!(
        "\n[ext_resource type=\"Texture2D\"{uid} path={} id=\"{id}\"]",
        quoted(&t.path)
    )
}

pub fn canvas_texture(
    id: &str,
    diffuse: &str,
    normal: Option<&str>,
    specular: Option<&str>,
    shininess: f32,
    nearest: bool,
) -> String {
    let mut out = format!("\n[sub_resource type=\"CanvasTexture\" id=\"{id}\"]\ndiffuse_texture = {diffuse}\n");
    if let Some(n) = normal {
        out += &format!("normal_texture = {n}\n");
    }
    if let Some(s) = specular {
        out += &format!(
            "specular_texture = {s}\nspecular_color = Color(1, 1, 1, 1)\nspecular_shininess = {}\n",
            number(shininess)
        );
    }
    if nearest {
        out += "texture_filter = 1\n";
    }
    out
}

pub fn sprite_frames(res: &SpriteFrames) -> String {
    let (sw, sh) = res.source;
    let lit = !res.lighting.is_empty();
    let mut out = format!("[gd_resource type=\"SpriteFrames\" format=3 uid={}]\n", quoted(res.uid));
    res.textures.iter().enumerate().for_each(|(i, t)| {
        out += &texture_line(t, &format!("{}_sheet", i + 1));
        if let Some(l) = res.lighting.get(i) {
            out += &texture_line(&l.normal, &format!("{}_normal", i + 1));
            if let Some(spec) = &l.specular {
                out += &texture_line(spec, &format!("{}_specular", i + 1));
            }
        }
    });
    out += "\n";
    res.lighting.iter().enumerate().for_each(|(i, l)| {
        out += &canvas_texture(
            &format!("CanvasTexture_{}", i + 1),
            &format!("ExtResource(\"{}_sheet\")", i + 1),
            Some(&format!("ExtResource(\"{}_normal\")", i + 1)),
            l.specular
                .as_ref()
                .map(|_| format!("ExtResource(\"{}_specular\")", i + 1))
                .as_deref(),
            res.shininess,
            res.nearest,
        );
    });
    res.trims
        .iter()
        .zip(res.placed)
        .enumerate()
        .for_each(|(i, (trim, at))| {
            let atlas = match lit {
                true => format!("SubResource(\"CanvasTexture_{}\")", at.page + 1),
                false => format!("ExtResource(\"{}_sheet\")", at.page + 1),
            };
            out += &format!(
                "\n[sub_resource type=\"AtlasTexture\" id=\"AtlasTexture_{i}\"]\natlas = {atlas}\nregion = {}\n",
                rect2([at.x, at.y, trim.w, trim.h].map(|v| v as f32))
            );
            let margin = [trim.x, trim.y, sw - trim.w, sh - trim.h];
            if margin != [0; 4] {
                out += &format!("margin = {}\n", rect2(margin.map(|v| v as f32)));
            }
        });
    let animations: Vec<String> = res
        .animations
        .iter()
        .map(|(anim, runs)| {
            let frames: Vec<String> = runs
                .iter()
                .map(|run| {
                    format!(
                        "{{\n\"duration\": {},\n\"texture\": SubResource(\"AtlasTexture_{}\")\n}}",
                        float(run.frames as f32),
                        run.picture
                    )
                })
                .collect();
            format!(
                "{{\n\"frames\": [{}],\n\"loop\": {},\n\"name\": &{},\n\"speed\": {}\n}}",
                frames.join(", "),
                anim.looping,
                quoted(&anim.name),
                float(anim.fps as f32)
            )
        })
        .collect();
    out += &format!("\n[resource]\nanimations = [{}]\n", animations.join(", "));
    out
}

pub struct TileSet<'a> {
    pub uid: &'a str,
    pub texture: &'a Texture,
    pub tile: [u32; 2],
    pub tiles: &'a [(u32, u32)],
    pub lighting: Option<&'a Lighting>,
    pub shininess: f32,
    pub nearest: bool,
}

pub fn tile_set(set: &TileSet) -> String {
    let [w, h] = set.tile;
    let tiles: String = set.tiles.iter().map(|(x, y)| format!("{x}:{y}/0 = 0\n")).collect();
    let mut ext = texture_line(set.texture, "1_tiles");
    let (canvas, texture) = match set.lighting {
        Some(l) => {
            ext += &texture_line(&l.normal, "1_normal");
            if let Some(spec) = &l.specular {
                ext += &texture_line(spec, "1_specular");
            }
            (
                canvas_texture(
                    "CanvasTexture_1",
                    "ExtResource(\"1_tiles\")",
                    Some("ExtResource(\"1_normal\")"),
                    l.specular.as_ref().map(|_| "ExtResource(\"1_specular\")"),
                    set.shininess,
                    set.nearest,
                ),
                "SubResource(\"CanvasTexture_1\")",
            )
        }
        None => (String::new(), "ExtResource(\"1_tiles\")"),
    };
    format!(
        "[gd_resource type=\"TileSet\" load_steps=3 format=3 uid={}]\n{ext}\n{canvas}\n[sub_resource type=\"TileSetAtlasSource\" id=\"TileSetAtlasSource_1\"]\ntexture = {texture}\ntexture_region_size = Vector2i({w}, {h})\n{tiles}\n[resource]\ntile_size = Vector2i({w}, {h})\nsources/0 = SubResource(\"TileSetAtlasSource_1\")\n",
        quoted(set.uid),
    )
}

pub struct Scene<'a> {
    pub uid: &'a str,
    pub node: &'a str,
    pub frames_path: &'a str,
    pub frames_uid: &'a str,
    pub animation: &'a str,
    pub origin: [f32; 2],
    pub pixel: bool,
    pub glow: Option<(&'a str, &'a str)>,
    pub shadows: Option<Shadows<'a>>,
    pub material: Option<Material<'a>>,
}

pub struct Shadows<'a> {
    pub shapes: &'a [Vec<Vec<[f32; 2]>>],
    pub pictures: &'a [(String, Vec<usize>)],
}

pub struct Material<'a> {
    pub shader: (&'a str, &'a str),
    pub params: &'a [(String, String)],
    pub textures: &'a [(String, Vec<Texture>)],
    pub pages: &'a [(String, Vec<usize>)],
}

pub fn gdscript(id: &str, source: &str) -> String {
    format!(
        "\n[sub_resource type=\"GDScript\" id=\"{id}\"]\nscript/source = {}\n",
        quoted(source)
    )
}

const GLOW: &str = "extends AnimatedSprite2D

func _ready():
\tvar body := get_parent() as AnimatedSprite2D
\tbody.frame_changed.connect(_follow)
\tbody.animation_changed.connect(_follow)
\t_follow()

func _follow():
\tvar body := get_parent() as AnimatedSprite2D
\tif animation != body.animation:
\t\tanimation = body.animation
\tframe = body.frame
";

const SHADOWS: &str = "extends Node2D

@export var shapes: Array = []
@export var pictures: Dictionary = {}

func _ready():
\tvar body := get_parent() as AnimatedSprite2D
\tbody.frame_changed.connect(_follow)
\tbody.animation_changed.connect(_follow)
\t_follow()

func _follow():
\tvar body := get_parent() as AnimatedSprite2D
\tvar list: PackedInt32Array = pictures.get(String(body.animation), PackedInt32Array())
\tvar shown: Array = []
\tif body.frame < list.size():
\t\tshown = shapes[list[body.frame]]
\tfor i in get_child_count():
\t\tvar occluder := get_child(i) as LightOccluder2D
\t\toccluder.occluder = shown[i] if i < shown.size() else null
";

const PAGES: &str = "extends Node

@export var pages: Dictionary = {}
@export var maps: Dictionary = {}

func _ready():
\tvar body := get_parent() as AnimatedSprite2D
\tbody.frame_changed.connect(_follow)
\tbody.animation_changed.connect(_follow)
\t_follow()

func _follow():
\tvar body := get_parent() as AnimatedSprite2D
\tvar list: PackedInt32Array = pages.get(String(body.animation), PackedInt32Array())
\tif body.frame >= list.size():
\t\treturn
\tvar page := list[body.frame]
\tfor name in maps:
\t\tbody.material.set_shader_parameter(name, maps[name][page])
";

fn ints(values: &[usize]) -> String {
    format!(
        "PackedInt32Array({})",
        values.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ")
    )
}

fn by_animation(list: &[(String, Vec<usize>)]) -> String {
    format!(
        "{{\n{}\n}}",
        list.iter()
            .map(|(name, values)| format!("{}: {}", quoted(name), ints(values)))
            .collect::<Vec<_>>()
            .join(",\n")
    )
}

pub fn node_name(stem: &str) -> String {
    let name: String = stem
        .chars()
        .map(|c| match c {
            '.' | ':' | '@' | '/' | '"' | '%' => '_',
            c => c,
        })
        .collect();
    match name.trim().is_empty() {
        true => "sprite".into(),
        false => name,
    }
}

pub fn scene(s: &Scene) -> String {
    let mut ext = format!(
        "\n[ext_resource type=\"SpriteFrames\" uid={} path={} id=\"1_frames\"]",
        quoted(s.frames_uid),
        quoted(s.frames_path)
    );
    if let Some((path, uid)) = s.glow {
        ext += &format!(
            "\n[ext_resource type=\"SpriteFrames\" uid={} path={} id=\"2_glow\"]",
            quoted(uid),
            quoted(path)
        );
    }
    let mut subs = String::new();
    let mut nodes = String::new();
    if let Some(m) = &s.material {
        ext += &format!(
            "\n[ext_resource type=\"Shader\" uid={} path={} id=\"3_shader\"]",
            quoted(m.shader.1),
            quoted(m.shader.0)
        );
        m.textures.iter().for_each(|(name, pages)| {
            pages.iter().enumerate().for_each(|(i, t)| {
                ext += &texture_line(t, &format!("{}_{name}", i + 4));
            });
        });
        let params: String = m
            .params
            .iter()
            .map(|(k, v)| format!("shader_parameter/{k} = {v}\n"))
            .chain(
                m.textures
                    .iter()
                    .map(|(name, _)| format!("shader_parameter/{name} = ExtResource(\"4_{name}\")\n")),
            )
            .collect();
        subs += &format!(
            "\n[sub_resource type=\"ShaderMaterial\" id=\"ShaderMaterial_maps\"]\nshader = ExtResource(\"3_shader\")\n{params}"
        );
        if m.textures.iter().any(|(_, pages)| pages.len() > 1) {
            subs += &gdscript("GDScript_pages", PAGES);
            let maps: Vec<String> = m
                .textures
                .iter()
                .map(|(name, pages)| {
                    let list: Vec<String> = (0..pages.len())
                        .map(|i| format!("ExtResource(\"{}_{name}\")", i + 4))
                        .collect();
                    format!("{}: [{}]", quoted(name), list.join(", "))
                })
                .collect();
            nodes += &format!(
                "\n[node name=\"pages\" type=\"Node\" parent=\".\"]\nscript = SubResource(\"GDScript_pages\")\npages = {}\nmaps = {{\n{}\n}}\n",
                by_animation(m.pages),
                maps.join(",\n")
            );
        }
    }
    let offset = format!("Vector2({}, {})", number(-s.origin[0]), number(-s.origin[1]));
    if s.glow.is_some() {
        subs += "\n[sub_resource type=\"CanvasItemMaterial\" id=\"CanvasItemMaterial_glow\"]\nlight_mode = 1\n";
        subs += &gdscript("GDScript_glow", GLOW);
        nodes += &format!(
            "\n[node name=\"glow\" type=\"AnimatedSprite2D\" parent=\".\"]\nmaterial = SubResource(\"CanvasItemMaterial_glow\")\nsprite_frames = ExtResource(\"2_glow\")\nanimation = &{}\ncentered = false\noffset = {offset}\nscript = SubResource(\"GDScript_glow\")\n",
            quoted(s.animation)
        );
    }
    if let Some(shadows) = &s.shadows {
        let most = shadows.shapes.iter().map(Vec::len).max().unwrap_or(0);
        let list: Vec<String> = shadows
            .shapes
            .iter()
            .enumerate()
            .map(|(p, rings)| {
                let ids: Vec<String> = rings
                    .iter()
                    .enumerate()
                    .map(|(k, ring)| {
                        let id = format!("OccluderPolygon2D_{p}_{k}");
                        let points: Vec<String> = ring
                            .iter()
                            .map(|v| format!("{}, {}", number(v[0] - s.origin[0]), number(v[1] - s.origin[1])))
                            .collect();
                        subs += &format!(
                            "\n[sub_resource type=\"OccluderPolygon2D\" id=\"{id}\"]\ncull_mode = 2\npolygon = PackedVector2Array({})\n",
                            points.join(", ")
                        );
                        format!("SubResource(\"{id}\")")
                    })
                    .collect();
                format!("[{}]", ids.join(", "))
            })
            .collect();
        subs += &gdscript("GDScript_shadows", SHADOWS);
        nodes += &format!(
            "\n[node name=\"shadows\" type=\"Node2D\" parent=\".\"]\nscript = SubResource(\"GDScript_shadows\")\nshapes = [{}]\npictures = {}\n",
            list.join(", "),
            by_animation(shadows.pictures)
        );
        (0..most).for_each(|k| {
            nodes += &format!(
                "\n[node name=\"occluder {}\" type=\"LightOccluder2D\" parent=\"shadows\"]\n",
                k + 1
            );
        });
    }
    let material = match s.material {
        Some(_) => "material = SubResource(\"ShaderMaterial_maps\")\n",
        None => "",
    };
    format!(
        "[gd_scene format=3 uid={}]\n{ext}\n{subs}\n[node name={} type=\"AnimatedSprite2D\"]\n{material}sprite_frames = ExtResource(\"1_frames\")\nanimation = &{}\nautoplay = {}\ncentered = false\noffset = {offset}\n",
        quoted(s.uid),
        quoted(s.node),
        quoted(s.animation),
        quoted(s.animation),
    ) + match s.pixel {
        true => "texture_filter = 1\n",
        false => "",
    } + &nodes
}

pub struct LitScene<'a> {
    pub uid: &'a str,
    pub node: &'a str,
    pub sprite: (&'a str, &'a str),
    pub origin: [f32; 2],
    pub size: [f32; 2],
    pub scale: f32,
    pub maps: &'a klad_core::maps::Maps,
}

fn colour(hex: &str) -> Result<String> {
    let [r, g, b, a] = klad_core::color::parse_hex(hex)?;
    Ok(format!(
        "Color({}, {}, {}, {})",
        number(r),
        number(g),
        number(b),
        number(a)
    ))
}

const POINTER: &str = "extends PointLight2D

func _process(_delta):
\tglobal_position = get_global_mouse_position()
";

pub fn lit_scene(s: &LitScene) -> Result<String> {
    let k = s.scale;
    let fade = (0..=4).map(|i| 1.0 - i as f32 / 4.0).map(|t| t * t);
    let colours: Vec<String> = fade.map(|v| format!("{0}, {0}, {0}, 1", number(v))).collect();
    let mut nodes = format!(
        "\n[node name={} type=\"Node2D\"]\n\n[node name=\"ambient\" type=\"CanvasModulate\" parent=\".\"]\ncolor = {}\n\n[node name={} parent=\".\" instance=ExtResource(\"1_sprite\")]\nposition = Vector2({}, {})\n",
        quoted(&format!("{}_lit", s.node)),
        colour(&s.maps.ambient)?,
        quoted(s.node),
        number(s.origin[0] * k),
        number(s.origin[1] * k)
    );
    let mut lights = s.maps.lights.clone();
    if !lights.iter().any(|l| l.kind == klad_core::maps::LightKind::Point) {
        let longest = s.size[0].max(s.size[1]);
        lights.insert(
            0,
            klad_core::maps::Light {
                at: [s.size[0] * 0.25, s.size[1] * 0.25],
                height: (longest * 0.15).round(),
                range: longest,
                ..klad_core::maps::Light::default()
            },
        );
    }
    let mut follows = true;
    lights.iter().enumerate().try_for_each(|(i, l)| {
        match l.kind {
            klad_core::maps::LightKind::Point => {
                let pointer = std::mem::take(&mut follows);
                nodes += &format!(
                    "\n[node name={} type=\"PointLight2D\" parent=\".\"]\nposition = Vector2({}, {})\ncolor = {}\nenergy = {}\nshadow_enabled = true\ntexture = SubResource(\"GradientTexture2D_fade\")\ntexture_scale = {}\nheight = {}\n{}",
                    quoted(&format!("light {}{}", i + 1, if pointer { " (follows the mouse)" } else { "" })),
                    number(l.at[0] * k),
                    number(l.at[1] * k),
                    colour(&l.color)?,
                    number(l.energy),
                    number(l.range * k / 128.0),
                    number(l.height * k),
                    if pointer { "script = SubResource(\"GDScript_pointer\")\n" } else { "" }
                );
            }
            klad_core::maps::LightKind::Directional => {
                let toward = [-l.at[0], -l.at[1]];
                let rotation = libm::atan2f(-toward[0], toward[1]);
                nodes += &format!(
                    "\n[node name={} type=\"DirectionalLight2D\" parent=\".\"]\nrotation = {}\ncolor = {}\nenergy = {}\nheight = {}\n",
                    quoted(&format!("light {}", i + 1)),
                    number(rotation),
                    colour(&l.color)?,
                    number(l.energy),
                    number(l.height)
                );
            }
        }
        Ok::<_, anyhow::Error>(())
    })?;
    let view = [1152.0, 648.0];
    let zoom = ((view[0] / (s.size[0] * k)).min(view[1] / (s.size[1] * k)) * 0.9).clamp(0.05, 64.0);
    nodes += &format!(
        "\n[node name=\"Camera2D\" type=\"Camera2D\" parent=\".\"]\nposition = Vector2({}, {})\nzoom = Vector2({}, {})\n",
        number(s.size[0] * k / 2.0),
        number(s.size[1] * k / 2.0),
        number(zoom),
        number(zoom)
    );
    Ok(format!(
        "[gd_scene format=3 uid={}]\n\n[ext_resource type=\"PackedScene\" uid={} path={} id=\"1_sprite\"]\n\n[sub_resource type=\"Gradient\" id=\"Gradient_fade\"]\noffsets = PackedFloat32Array(0, 0.25, 0.5, 0.75, 1)\ncolors = PackedColorArray({})\n\n[sub_resource type=\"GradientTexture2D\" id=\"GradientTexture2D_fade\"]\ngradient = SubResource(\"Gradient_fade\")\nwidth = 256\nheight = 256\nfill = 1\nfill_from = Vector2(0.5, 0.5)\nfill_to = Vector2(1, 0.5)\n{}{nodes}",
        quoted(s.uid),
        quoted(s.sprite.1),
        quoted(s.sprite.0),
        colours.join(", "),
        gdscript("GDScript_pointer", POINTER)
    ))
}

pub fn uid_text(mut id: u64) -> String {
    id &= 0x7fff_ffff_ffff_ffff;
    let mut digits = Vec::new();
    loop {
        let c = (id % 34) as u8;
        digits.push(if c < 25 { b'a' + c } else { b'0' + c - 25 });
        id /= 34;
        if id == 0 {
            break;
        }
    }
    digits.reverse();
    format!("uid://{}", String::from_utf8_lossy(&digits))
}

pub fn uid_for(seed: &str) -> String {
    uid_text(fnv(FNV_OFFSET, seed.as_bytes()))
}

fn uid_in(text: &str) -> Option<String> {
    let at = text.find("uid=\"uid://")? + 5;
    let end = at + text[at..].find('"')?;
    Some(text[at..end].to_string())
}

pub fn header_uid(text: &str) -> Option<String> {
    uid_in(text.lines().next()?)
}

pub fn import_uid(text: &str) -> Option<String> {
    text.lines().find(|l| l.starts_with("uid=")).and_then(uid_in)
}

#[cfg(test)]
mod tests {
    use super::*;
    use klad_core::doc::Clip;

    fn anim(from: u32, to: u32, fps: u32) -> Animation {
        Animation {
            name: "run".into(),
            from,
            to,
            fps,
            looping: true,
            clip: true,
        }
    }

    #[test]
    fn runs_merge_held_frames_and_their_milliseconds_add_up_to_the_clip() {
        let pictures = [0, 0, 1, 2, 2, 2, 0, 3, 3, 3, 3, 3];
        let run = runs(&anim(0, 11, 12), |f| pictures[f as usize]);
        assert_eq!(
            run.iter().map(|r| (r.picture, r.frame, r.frames)).collect::<Vec<_>>(),
            [(0, 0, 2), (1, 2, 1), (2, 3, 3), (0, 6, 1), (3, 7, 5)]
        );
        assert_eq!(run.iter().map(|r| r.ms).collect::<Vec<_>>(), [167, 83, 250, 83, 417]);
        assert_eq!(run.iter().map(|r| r.ms).sum::<u32>(), 1000);
        let at24 = runs(&anim(2, 8, 24), |f| pictures[f as usize]);
        assert_eq!(at24.iter().map(|r| r.ms).sum::<u32>(), 292, "7 frames at 24 fps");
        assert_eq!(at24[0].frame, 2);
    }

    #[test]
    fn animations_come_from_clips_or_the_whole_timeline() {
        let mut doc = Document::new(8, 8);
        doc.timeline.frames = 20;
        let names = |d: &Document, spec: &str| {
            animations(d, spec)
                .unwrap()
                .iter()
                .map(|a| (a.name.clone(), a.from, a.to, a.fps, a.looping))
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&doc, "all"), [("default".into(), 0, 19, 12, true)]);
        doc.timeline.clips = vec![
            Clip::new("idle", 0, 5, true),
            Clip {
                fps: Some(24),
                ..Clip::new("jump", 6, 13, false)
            },
        ];
        assert_eq!(
            names(&doc, "all"),
            [("idle".into(), 0, 5, 12, true), ("jump".into(), 6, 13, 24, false)]
        );
        assert_eq!(names(&doc, "jump"), [("jump".into(), 6, 13, 24, false)]);
        assert_eq!(names(&doc, "2-4"), [("2-4".into(), 2, 4, 12, true)]);
        assert!(animations(&doc, "walk").is_err());
    }

    #[test]
    fn packing_keeps_order_and_padding_and_starts_pages_under_the_limit() {
        let sizes = [(30, 20), (30, 20), (50, 10), (10, 40)];
        let (placed, pages) = pack(&sizes, 2, 128).unwrap();
        assert_eq!(pages.len(), 1);
        let boxes: Vec<[u32; 4]> = placed
            .iter()
            .zip(&sizes)
            .map(|(p, (w, h))| [p.x, p.y, p.x + w, p.y + h])
            .collect();
        boxes.iter().enumerate().for_each(|(i, a)| {
            assert!(a[0] >= 2 && a[1] >= 2 && a[2] + 2 <= pages[0].0 && a[3] + 2 <= pages[0].1);
            boxes[i + 1..].iter().for_each(|b| {
                let apart = a[2] + 2 <= b[0] || b[2] + 2 <= a[0] || a[3] + 2 <= b[1] || b[3] + 2 <= a[1];
                assert!(apart, "{a:?} {b:?}");
            });
        });
        assert_eq!((placed[0].x, placed[0].y, placed[1].x), (2, 2, 34));
        let (placed, pages) = pack(&[(60, 60); 5], 2, 128).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(placed.iter().map(|p| p.page).collect::<Vec<_>>(), [0, 0, 0, 0, 1]);
        assert_eq!(pages, [(126, 126), (64, 64)]);
        assert!(pack(&[(200, 10)], 2, 128).is_err());
    }

    #[test]
    fn trimming_finds_the_pixels_and_empty_pictures_keep_one_pixel() {
        let mut p = Pixels::blank(16, 8, 8);
        assert_eq!(p.trim(), Rect { x: 0, y: 0, w: 1, h: 1 });
        if let Data::Rgba8(d) = &mut p.data {
            [(3, 2), (9, 5), (5, 6)]
                .iter()
                .for_each(|&(x, y)| d[(y * 16 + x) * 4 + 3] = 255);
        }
        let trim = p.trim();
        assert_eq!(trim, Rect { x: 3, y: 2, w: 7, h: 5 });
        let cut = p.crop(trim);
        assert_eq!((cut.width, cut.height), (7, 5));
        assert_ne!(cut.hash(), p.hash());
        assert_eq!(cut.crop(cut.trim()), cut);
    }

    #[test]
    fn aseprite_json_lists_exposures_with_durations_and_tags_index_them() {
        let trims = [
            Rect {
                x: 4,
                y: 2,
                w: 10,
                h: 12,
            },
            Rect {
                x: 0,
                y: 0,
                w: 32,
                h: 32,
            },
        ];
        let placed = [Placed { page: 0, x: 2, y: 2 }, Placed { page: 1, x: 2, y: 2 }];
        let idle = Animation {
            looping: false,
            ..anim(0, 3, 10)
        };
        let anims = vec![
            (
                Animation {
                    name: "idle".into(),
                    ..idle
                },
                vec![
                    Run {
                        picture: 0,
                        frame: 0,
                        frames: 3,
                        ms: 300,
                    },
                    Run {
                        picture: 1,
                        frame: 3,
                        frames: 1,
                        ms: 100,
                    },
                ],
            ),
            (
                anim(4, 4, 10),
                vec![Run {
                    picture: 0,
                    frame: 4,
                    frames: 1,
                    ms: 100,
                }],
            ),
        ];
        let images = ["ball.png".to_string(), "ball-2.png".to_string()];
        let json = aseprite(&Sheet {
            stem: "ball",
            source: (32, 32),
            trims: &trims,
            placed: &placed,
            pages: &[(16, 18), (36, 36)],
            images: &images,
            animations: &anims,
            scale: 0.5,
            depth: 8,
        });
        let frames = json["frames"].as_array().unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0]["filename"], "idle 0");
        assert_eq!(frames[0]["frame"], json!({"x": 2, "y": 2, "w": 10, "h": 12}));
        assert_eq!(frames[0]["spriteSourceSize"], json!({"x": 4, "y": 2, "w": 10, "h": 12}));
        assert_eq!(
            (frames[0]["trimmed"].clone(), frames[1]["trimmed"].clone()),
            (json!(true), json!(false))
        );
        assert_eq!(frames[1]["image"], "ball-2.png");
        assert_eq!(frames.iter().map(|f| f["duration"].as_u64().unwrap()).sum::<u64>(), 500);
        let tags = json["meta"]["frameTags"].as_array().unwrap();
        assert_eq!(
            tags[0],
            json!({"name":"idle","from":0,"to":1,"direction":"forward","color":"#000000ff","repeat":"1"})
        );
        assert_eq!((tags[1]["from"].clone(), tags[1]["to"].clone()), (json!(2), json!(2)));
        assert_eq!(json["meta"]["scale"], "0.5");
        assert_eq!(json["meta"]["images"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn godot_text_has_atlas_frames_durations_and_an_offset_scene() {
        let trims = [
            Rect {
                x: 4,
                y: 2,
                w: 10,
                h: 12,
            },
            Rect {
                x: 0,
                y: 0,
                w: 32,
                h: 32,
            },
        ];
        let placed = [Placed { page: 0, x: 2, y: 2 }, Placed { page: 0, x: 14, y: 2 }];
        let anims = vec![(
            Animation {
                name: "say \"hi\"".into(),
                looping: false,
                ..anim(0, 3, 12)
            },
            vec![
                Run {
                    picture: 0,
                    frame: 0,
                    frames: 3,
                    ms: 250,
                },
                Run {
                    picture: 1,
                    frame: 3,
                    frames: 1,
                    ms: 83,
                },
            ],
        )];
        let textures = [Texture {
            path: "ball.png".into(),
            uid: Some("uid://abc".into()),
        }];
        let tres = sprite_frames(&SpriteFrames {
            uid: "uid://frames",
            textures: &textures,
            source: (32, 32),
            trims: &trims,
            placed: &placed,
            animations: &anims,
            lighting: &[],
            shininess: 0.0,
            nearest: false,
        });
        assert!(tres.starts_with("[gd_resource type=\"SpriteFrames\" format=3 uid=\"uid://frames\"]\n"));
        assert!(tres.contains("[ext_resource type=\"Texture2D\" uid=\"uid://abc\" path=\"ball.png\" id=\"1_sheet\"]"));
        assert!(tres.contains("region = Rect2(2, 2, 10, 12)\nmargin = Rect2(4, 2, 22, 20)\n"));
        assert!(!tres.contains("margin = Rect2(0, 0, 0, 0)"));
        assert!(tres.contains("\"duration\": 3.0,\n\"texture\": SubResource(\"AtlasTexture_0\")"));
        assert!(tres.contains("\"loop\": false,\n\"name\": &\"say \\\"hi\\\"\",\n\"speed\": 12.0\n}]"));
        assert_eq!(header_uid(&tres).as_deref(), Some("uid://frames"));
        let tscn = scene(&Scene {
            uid: "uid://scene",
            node: &node_name("ball.v2"),
            frames_path: "ball.tres",
            frames_uid: "uid://frames",
            animation: "idle",
            origin: [16.0, 30.5],
            pixel: false,
            glow: None,
            shadows: None,
            material: None,
        });
        assert!(tscn.contains("[node name=\"ball_v2\" type=\"AnimatedSprite2D\"]"));
        assert!(tscn.contains("offset = Vector2(-16, -30.5)\n"));
        assert_eq!(header_uid(&tscn).as_deref(), Some("uid://scene"));
        let import = "[remap]\n\nimporter=\"texture\"\ntype=\"CompressedTexture2D\"\nuid=\"uid://c6xh0mqe8vbs6\"\n";
        assert_eq!(import_uid(import).as_deref(), Some("uid://c6xh0mqe8vbs6"));
    }

    #[test]
    fn uids_use_godots_alphabet_and_differ_by_path() {
        let a = uid_for("/game/art/ball.tres");
        let b = uid_for("/game/art/ball.tscn");
        assert_ne!(a, b);
        assert_eq!(a, uid_for("/game/art/ball.tres"));
        assert!(a.starts_with("uid://"));
        assert!(a[6..].chars().all(|c| matches!(c, 'a'..='y' | '0'..='8')), "{a}");
        assert_eq!(uid_text(0), "uid://a");
        assert_eq!(uid_text(34), "uid://ba");
        assert_eq!(uid_text(33), "uid://8");
    }
}
