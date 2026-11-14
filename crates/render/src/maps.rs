use crate::export::{self, Options, Target, f16_pixels};
use crate::{Frame, Rasters};
use anyhow::{Result, bail};
use half::f16;
use klad_brush::tiles::Heights;
use klad_brush::{Raster, TILE};
use klad_core::color::parse_hex;
use klad_core::doc::{Blend, Document, Kind, Layer, LayerContent};
use klad_core::maps::{Light, LightKind, Maps, Role, Surface, nearest, palette, unit};
use rayon::prelude::*;
use skia_safe::AlphaType;
use std::collections::BTreeMap;

const BODY_PX: f32 = 3.0;
const FAR: f32 = 1e20;
const NEAREST: f32 = 0.75;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Map {
    Normal,
    Height,
    Specular,
    Emission,
    Depth,
    Occlusion,
}

impl Map {
    pub const ALL: [Map; 6] = [
        Map::Normal,
        Map::Height,
        Map::Specular,
        Map::Emission,
        Map::Depth,
        Map::Occlusion,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Map::Normal => "normal",
            Map::Height => "height",
            Map::Specular => "specular",
            Map::Emission => "emission",
            Map::Depth => "depth",
            Map::Occlusion => "occlusion",
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Map::Normal => "_n",
            Map::Height => "_h",
            Map::Specular => "_s",
            Map::Emission => "_e",
            Map::Depth => "_d",
            Map::Occlusion => "_o",
        }
    }

    pub fn wide(self) -> bool {
        self == Map::Depth
    }

    pub fn parse(text: &str) -> Result<Map> {
        Map::ALL.into_iter().find(|m| m.name() == text).ok_or_else(|| {
            anyhow::anyhow!("no {text} map: the maps are normal, height, specular, emission, depth and occlusion")
        })
    }

    pub fn list(text: &str) -> Result<Vec<Map>> {
        let mut maps: Vec<Map> = match text.trim() {
            "all" => Map::ALL.to_vec(),
            "" => Vec::new(),
            list => list.split(',').map(|m| Map::parse(m.trim())).collect::<Result<_>>()?,
        };
        maps.sort();
        maps.dedup();
        Ok(maps)
    }
}

pub struct MapSet {
    pub width: usize,
    pub height: usize,
    pub cover: Vec<f32>,
    pub normal: Vec<[f32; 3]>,
    pub rise: Vec<f32>,
    pub specular: Vec<[f32; 3]>,
    pub emission: Vec<[f32; 4]>,
    pub depth: Vec<f32>,
    pub occlusion: Vec<f32>,
}

impl MapSet {
    pub fn straight(&self, map: Map) -> Vec<[f32; 4]> {
        let top = self.rise.iter().copied().fold(0.0f32, f32::max).max(1e-6);
        (0..self.width * self.height)
            .into_par_iter()
            .map(|i| {
                let a = self.cover[i].clamp(0.0, 1.0);
                match map {
                    Map::Normal => {
                        let [x, y, z] = klad_core::maps::encoded(self.normal[i]);
                        [x, y, z, a]
                    }
                    Map::Height => {
                        let v = (self.rise[i] / top).clamp(0.0, 1.0);
                        [v, v, v, a]
                    }
                    Map::Specular => {
                        let [r, g, b] = self.specular[i].map(|c| c.clamp(0.0, 1.0));
                        [r, g, b, a]
                    }
                    Map::Emission => {
                        let [r, g, b, e] = self.emission[i];
                        match e > 0.0 {
                            true => [r / e, g / e, b / e, e].map(|c| c.clamp(0.0, 1.0)),
                            false => [0.0; 4],
                        }
                    }
                    Map::Depth => {
                        let v = self.depth[i].clamp(0.0, 1.0);
                        [v, v, v, a]
                    }
                    Map::Occlusion => {
                        let v = self.occlusion[i].clamp(0.0, 1.0);
                        [v, v, v, a]
                    }
                }
            })
            .collect()
    }

    pub fn frame(&self, map: Map) -> Frame {
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let rgba = self.straight(map).iter().flat_map(|p| p.map(byte)).collect();
        Frame {
            width: self.width as u32,
            height: self.height as u32,
            rgba,
        }
    }

    pub fn invert_depth(&mut self) {
        self.depth
            .par_iter_mut()
            .zip(&self.cover)
            .for_each(|(d, a)| *d = if *a > 0.0 { 1.0 - *d } else { 1.0 });
    }

    fn enlarged(self, k: usize) -> MapSet {
        if k <= 1 {
            return self;
        }
        let (w, h) = (self.width * k, self.height * k);
        let from = |i: usize| ((i / w) / k) * self.width + (i % w) / k;
        fn grow<T: Copy + Send + Sync>(v: &[T], n: usize, from: &(dyn Fn(usize) -> usize + Sync)) -> Vec<T> {
            (0..n).into_par_iter().map(|i| v[from(i)]).collect()
        }
        let n = w * h;
        MapSet {
            width: w,
            height: h,
            cover: grow(&self.cover, n, &from),
            normal: grow(&self.normal, n, &from),
            rise: grow(&self.rise, n, &from).into_iter().map(|r| r * k as f32).collect(),
            specular: grow(&self.specular, n, &from),
            emission: grow(&self.emission, n, &from),
            depth: grow(&self.depth, n, &from),
            occlusion: grow(&self.occlusion, n, &from),
        }
    }
}

struct Owner<'a> {
    surface: Surface,
    layers: Vec<&'a Layer>,
    alpha: Vec<f32>,
    weight: Vec<f32>,
}

fn is_surface(layer: &Layer) -> bool {
    layer.visible
        && !layer.reference
        && !layer.clip
        && layer.map.is_none()
        && layer.blend == Blend::Normal
        && !matches!(layer.kind(), Kind::Adjust | Kind::Bones)
}

fn options(doc: &Document, scale: f32) -> Options {
    Options {
        scale,
        background: false,
        references: false,
        detail: export::detail_of(doc, scale),
    }
}

fn floats(image: &skia_safe::Image) -> Result<Vec<[f32; 4]>> {
    Ok(f16_pixels(image, AlphaType::Premul)?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p.map(|v| v.to_f32()))
        .collect())
}

fn alone(doc: &Document, frame: u32, rasters: &Rasters, layer: &Layer, scale: f32) -> Result<Vec<[f32; 4]>> {
    let mut shown = layer.clone();
    shown.map = None;
    shown.visible = true;
    floats(&export::render(
        doc,
        frame,
        rasters,
        &Target::Layer(&shown),
        options(doc, scale),
    )?)
}

fn painted<'a>(layer: &'a Layer, frame: u32, out: &mut Vec<&'a str>) {
    if !layer.visible || layer.reference || layer.map.is_some() {
        return;
    }
    match &layer.content {
        LayerContent::Paint { .. } => out.extend(layer.content_at(frame).map(|c| c.id.as_str())),
        LayerContent::Group { children } => children.iter().for_each(|c| painted(c, frame, out)),
        _ => {}
    }
}

fn height_at(heights: &Heights, x: i32, y: i32) -> f32 {
    if x < 0 || y < 0 || x as u32 >= heights.width || y as u32 >= heights.height {
        return 0.0;
    }
    let (x, y) = (x as usize, y as usize);
    heights
        .get(x / TILE, y / TILE)
        .map_or(0.0, |t| t[(y % TILE) * TILE + x % TILE].to_f32())
}

fn sampled(heights: &Heights, cx: f32, cy: f32) -> f32 {
    let (fx, fy) = (cx - 0.5, cy - 0.5);
    let (x, y) = (fx.floor(), fy.floor());
    let (ax, ay) = (fx - x, fy - y);
    let (x, y) = (x as i32, y as i32);
    let top = height_at(heights, x, y) * (1.0 - ax) + height_at(heights, x + 1, y) * ax;
    let bottom = height_at(heights, x, y + 1) * (1.0 - ax) + height_at(heights, x + 1, y + 1) * ax;
    top * (1.0 - ay) + bottom * ay
}

fn distance_1d(f: &[f32], out: &mut [f32]) {
    let n = f.len();
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f32; n + 1];
    let mut k = 0usize;
    z[0] = -FAR;
    z[1] = FAR;
    (1..n).for_each(|q| {
        let meet = |k: usize| {
            let p = v[k];
            ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32)
        };
        let mut s = meet(k);
        while s <= z[k] && k > 0 {
            k -= 1;
            s = meet(k);
        }
        if s <= z[k] {
            v[0] = q;
            z[0] = -FAR;
            z[1] = FAR;
            k = 0;
            return;
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = FAR;
    });
    let mut k = 0usize;
    (0..n).for_each(|q| {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        let d = q as f32 - p as f32;
        out[q] = (d * d + f[p]).min(FAR);
    });
}

fn transpose(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    (0..w * h).map(|i| src[(i % h) * w + i / h]).collect()
}

fn rows(grid: &mut [f32], w: usize) {
    grid.par_chunks_mut(w).for_each(|row| {
        let f = row.to_vec();
        distance_1d(&f, row);
    });
}

pub fn edge_distance(alpha: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut grid: Vec<f32> = alpha.iter().map(|a| if *a < 0.5 { 0.0 } else { FAR }).collect();
    if grid.iter().all(|v| *v >= FAR) {
        return vec![FAR; w * h];
    }
    rows(&mut grid, w);
    let mut turned = transpose(&grid, w, h);
    rows(&mut turned, h);
    transpose(&turned, h, w).iter().map(|d| libm::sqrtf(*d)).collect()
}

fn soften(grid: &[f32], w: usize, h: usize) -> Vec<f32> {
    let across: Vec<f32> = (0..w * h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let at = |x: usize| grid[y * w + x];
            (at(x.saturating_sub(1)) + 2.0 * at(x) + at((x + 1).min(w - 1))) * 0.25
        })
        .collect();
    (0..w * h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let at = |y: usize| across[y * w + x];
            (at(y.saturating_sub(1)) + 2.0 * at(y) + at((y + 1).min(h - 1))) * 0.25
        })
        .collect()
}

fn box_line(line: &[f32], reach: usize) -> Vec<f32> {
    let n = line.len();
    let sums: Vec<f32> = std::iter::once(0.0)
        .chain(line.iter().scan(0.0f32, |acc, v| {
            *acc += v;
            Some(*acc)
        }))
        .collect();
    (0..n)
        .map(|i| {
            let (a, b) = (i.saturating_sub(reach), (i + reach + 1).min(n));
            (sums[b] - sums[a]) / (b - a) as f32
        })
        .collect()
}

fn smooth(grid: &[f32], w: usize, h: usize, reach: usize) -> Vec<f32> {
    let across: Vec<f32> = grid.par_chunks(w).flat_map_iter(|row| box_line(row, reach)).collect();
    let turned = transpose(&across, w, h);
    let down: Vec<f32> = turned.par_chunks(h).flat_map_iter(|col| box_line(col, reach)).collect();
    transpose(&down, h, w)
}

fn normals(rise: &[f32], w: usize, h: usize, strength: f32) -> Vec<[f32; 3]> {
    (0..w * h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let at = |x: usize, y: usize| rise[y * w + x];
            let dx = (at((x + 1).min(w - 1), y) - at(x.saturating_sub(1), y)) * 0.5;
            let dy = (at(x, (y + 1).min(h - 1)) - at(x, y.saturating_sub(1))) * 0.5;
            unit([-strength * dx, strength * dy, 1.0])
        })
        .collect()
}

fn luminance(p: [f32; 4]) -> f32 {
    match p[3] > 0.0 {
        true => (0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]) / p[3],
        false => 0.0,
    }
}

fn top_of<'a>(doc: &'a Document, id: &str) -> Option<&'a Layer> {
    doc.layers.iter().find(|top| {
        let mut all = vec![*top];
        klad_core::doc::walk_children(top, &mut all);
        all.iter().any(|l| l.id == id)
    })
}

fn owner_of(doc: &Document, layer: &Layer) -> Option<String> {
    let top = top_of(doc, &layer.id)?;
    let unit = match top.id == layer.id && layer.clip {
        true => {
            let at = doc.layers.iter().position(|l| l.id == layer.id)?;
            doc.layers[..at].iter().rev().find(|l| !l.clip)?
        }
        false => top,
    };
    unit.surface.as_ref().map(|_| unit.id.clone())
}

pub fn map_layers(doc: &Document, role: Role) -> Vec<&Layer> {
    let mut all = Vec::new();
    doc.layers.iter().for_each(|l| {
        all.push(l);
        klad_core::doc::walk_children(l, &mut all);
    });
    all.into_iter()
        .filter(|l| l.map == Some(role) && l.visible && !l.reference)
        .collect()
}

pub fn settings(doc: &Document) -> Maps {
    doc.maps.clone().unwrap_or_default()
}

pub fn size(doc: &Document, scale: f32) -> (usize, usize) {
    (
        ((doc.canvas.width as f32 * scale).round() as usize).max(1),
        ((doc.canvas.height as f32 * scale).round() as usize).max(1),
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Traits {
    pub shine: f32,
    pub specular: [f32; 3],
    pub emissive: bool,
    pub z: f32,
}

impl Default for Traits {
    fn default() -> Self {
        Self {
            shine: 0.0,
            specular: [1.0; 3],
            emissive: false,
            z: 0.0,
        }
    }
}

fn gather(layers: &[Layer], above: Traits, out: &mut BTreeMap<String, Traits>) -> Result<()> {
    layers.iter().try_for_each(|layer| {
        let specular = match &layer.specular {
            Some(hex) => {
                let [r, g, b, _] = parse_hex(hex)?;
                [r, g, b]
            }
            None => above.specular,
        };
        let here = Traits {
            shine: layer.shine.unwrap_or(above.shine),
            specular,
            emissive: layer.emissive || above.emissive,
            z: above.z + layer.z.unwrap_or(0.0),
        };
        match &layer.content {
            LayerContent::Group { children } => gather(children, here, out),
            _ => {
                out.insert(layer.id.clone(), here);
                Ok(())
            }
        }
    })
}

pub fn traits(doc: &Document) -> Result<BTreeMap<String, Traits>> {
    let mut out = BTreeMap::new();
    gather(&doc.layers, Traits::default(), &mut out)?;
    Ok(out)
}

#[derive(Clone, Copy)]
enum Tint {
    Own,
    Flat([f32; 3]),
}

fn plainly(layers: &mut [Layer]) {
    layers.iter_mut().for_each(|l| {
        l.effects.clear();
        l.blend = Blend::Normal;
        if l.kind() == Kind::Adjust {
            l.visible = false;
        }
        if let LayerContent::Group { children } = &mut l.content {
            plainly(children);
        }
    });
}

fn recolour(raster: &mut Raster, colour: [f32; 3]) {
    let tiles: Vec<usize> = raster.occupied().map(|(i, _)| i).collect();
    tiles.into_iter().for_each(|i| {
        if let Some(tile) = raster.at_mut(i) {
            tile.par_chunks_mut(4).for_each(|px| {
                let a = px[3].to_f32();
                (0..3).for_each(|c| px[c] = f16::from_f32(colour[c] * a));
            });
        }
    });
}

fn tinted(
    doc: &Document,
    frame: u32,
    rasters: &Rasters,
    scale: f32,
    traits: &BTreeMap<String, Traits>,
    tint: impl Fn(&Traits) -> Tint,
) -> Result<Vec<[f32; 4]>> {
    let detail = export::detail_of(doc, scale);
    let mut plain = doc.clone();
    plain.atmosphere = None;
    plainly(&mut plain.layers);
    let mut out = rasters.clone();
    doc.all_layers()
        .into_iter()
        .filter(|l| matches!(l.kind(), Kind::Paint | Kind::Vector) && l.map.is_none())
        .try_for_each(|layer| {
            let (Some(cel), Some(t)) = (layer.content_at(frame), traits.get(&layer.id)) else {
                return Ok(());
            };
            let Tint::Flat(colour) = tint(t) else {
                return Ok(());
            };
            let mut raster = match (rasters.get(&cel.id), layer.kind()) {
                (Some(r), _) => r.clone(),
                (None, Kind::Vector) => crate::rasterize_nodes_at(
                    &klad_core::doc::nodes_at(&cel.nodes, frame),
                    doc.canvas.width,
                    doc.canvas.height,
                    detail,
                )?,
                _ => return Ok(()),
            };
            recolour(&mut raster, colour);
            out.insert(cel.id.clone(), raster);
            Ok::<_, anyhow::Error>(())
        })?;
    floats(&export::render(
        &plain,
        frame,
        &out,
        &Target::Frame,
        options(&plain, scale),
    )?)
}

fn occlusion(elevation: &[f32], cover: &[f32], w: usize, h: usize, reach: f32) -> Vec<f32> {
    let radii: Vec<usize> = [1.0, 2.0, 4.0, 8.0]
        .iter()
        .map(|k| ((reach * k).round() as usize).max(1))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let around: Vec<Vec<f32>> = radii.iter().map(|r| smooth(elevation, w, h, *r)).collect();
    (0..w * h)
        .into_par_iter()
        .map(|i| {
            if cover[i] < 0.5 {
                return 1.0;
            }
            let closed: f32 = radii
                .iter()
                .zip(&around)
                .map(|(r, a)| ((a[i] - elevation[i]) / *r as f32).clamp(0.0, 1.0))
                .sum::<f32>()
                / radii.len() as f32;
            (1.0 - 1.5 * closed).clamp(0.0, 1.0)
        })
        .collect()
}

pub fn render(doc: &Document, frame: u32, rasters: &Rasters, scale: f32) -> Result<MapSet> {
    export::check_scale(doc, scale)?;
    if doc.pixel_art() && scale > 1.0 {
        return Ok(render_at(doc, frame, rasters, 1.0)?.enlarged(scale.round() as usize));
    }
    render_at(doc, frame, rasters, scale)
}

fn render_at(doc: &Document, frame: u32, rasters: &Rasters, scale: f32) -> Result<MapSet> {
    let settings = settings(doc);
    let pixel = doc.pixel_art();
    let (w, h) = size(doc, scale);
    let n = w * h;
    let units: Vec<&Layer> = doc.layers.iter().filter(|l| is_surface(l)).collect();
    let mut root = Owner {
        surface: settings.surface.clone(),
        layers: Vec::new(),
        alpha: vec![0.0; n],
        weight: vec![0.0; n],
    };
    let mut owned: Vec<(String, Owner)> = Vec::new();
    let mut remaining = vec![1.0f32; n];
    let mut planes = vec![0.0f32; n];
    units.iter().rev().try_for_each(|layer| {
        let px = alone(doc, frame, rasters, layer, scale)?;
        let depth = layer.depth.unwrap_or(0.0);
        let owner = match &layer.surface {
            Some(surface) => {
                owned.push((
                    layer.id.clone(),
                    Owner {
                        surface: surface.clone(),
                        layers: Vec::new(),
                        alpha: vec![0.0; n],
                        weight: vec![0.0; n],
                    },
                ));
                &mut owned.last_mut().expect("just pushed").1
            }
            None => &mut root,
        };
        owner.layers.push(layer);
        owner
            .alpha
            .par_iter_mut()
            .zip(owner.weight.par_iter_mut())
            .zip(remaining.par_iter_mut())
            .zip(planes.par_iter_mut())
            .zip(px.par_iter())
            .for_each(|((((union, weight), left), plane), p)| {
                let a = p[3].clamp(0.0, 1.0);
                *union = 1.0 - (1.0 - *union) * (1.0 - a);
                *weight += a * *left;
                *plane += a * *left * depth;
                *left *= 1.0 - a;
            });
        Ok::<_, anyhow::Error>(())
    })?;
    let lifts: Vec<(Option<String>, Vec<[f32; 4]>)> = map_layers(doc, Role::Height)
        .into_iter()
        .map(|l| Ok((owner_of(doc, l), alone(doc, frame, rasters, l, scale)?)))
        .collect::<Result<_>>()?;
    let owners: Vec<(Option<String>, Owner)> = std::iter::once((None, root))
        .chain(owned.into_iter().map(|(id, o)| (Some(id), o)))
        .collect();
    let detail = export::detail_of(doc, scale);
    let rises: Vec<Vec<f32>> = owners
        .iter()
        .map(|(id, owner)| {
            let s = &owner.surface;
            let bevel = s.bevel * scale;
            let rise = match bevel > 0.0 && s.profile != klad_core::maps::Profile::Flat {
                true => {
                    let near: Vec<f32> = edge_distance(&owner.alpha, w, h)
                        .iter()
                        .map(|d| d.min(bevel * 1.5))
                        .collect();
                    let eased = match pixel {
                        true => near,
                        false => {
                            let reach = ((bevel * 0.08).round() as usize).clamp(1, 12);
                            smooth(&smooth(&near, w, h, reach), w, h, reach)
                        }
                    };
                    eased.par_iter().map(|d| s.profile.rise(d / bevel) * bevel).collect()
                }
                false => vec![0.0; n],
            };
            let mut cels = Vec::new();
            owner.layers.iter().for_each(|l| painted(l, frame, &mut cels));
            let maps: Vec<&Heights> = cels
                .iter()
                .filter_map(|c| rasters.get(*c).and_then(|r| r.heights()))
                .collect();
            let body = s.body * BODY_PX * scale;
            let lift = s.range * scale;
            let mine: Vec<&Vec<[f32; 4]>> = lifts.iter().filter(|(o, _)| o == id).map(|(_, px)| px).collect();
            let raw: Vec<f32> = rise
                .into_par_iter()
                .enumerate()
                .map(|(i, r)| {
                    let (x, y) = (i % w, i / w);
                    let (cx, cy) = ((x as f32 + 0.5) / scale * detail, (y as f32 + 0.5) / scale * detail);
                    let paint: f32 = maps.iter().map(|m| sampled(m, cx, cy)).sum();
                    let eased = 1.6 * libm::tanhf(paint / 1.6);
                    let raised: f32 = mine.iter().map(|px| px[i][3] * (2.0 * luminance(px[i]) - 1.0)).sum();
                    (r + eased * body + raised * lift) * owner.alpha[i].clamp(0.0, 1.0).sqrt()
                })
                .collect();
            match pixel {
                true => raw,
                false => soften(&soften(&raw, w, h), w, h),
            }
        })
        .collect();
    let mut normal = vec![[0.0f32; 3]; n];
    let mut rise = vec![0.0f32; n];
    owners.iter().zip(&rises).for_each(|((_, owner), r)| {
        let choices = palette(owner.surface.directions_for(pixel));
        let lit: Vec<[f32; 3]> = match choices.len() > 1 {
            true => normals(r, w, h, owner.surface.strength)
                .into_par_iter()
                .map(|v| nearest(v, &choices))
                .collect(),
            false => normals(r, w, h, owner.surface.strength),
        };
        normal
            .par_iter_mut()
            .zip(rise.par_iter_mut())
            .enumerate()
            .for_each(|(i, (nm, ri))| {
                let k = owner.weight[i];
                nm.iter_mut().zip(lit[i]).for_each(|(a, b)| *a += k * b);
                *ri += k * r[i];
            });
    });
    let cover: Vec<f32> = remaining.iter().map(|left| 1.0 - left).collect();
    normal.par_iter_mut().zip(&remaining).for_each(|(nm, left)| {
        nm[2] += left;
        *nm = unit(*nm);
    });
    let over = |role: Role| -> Result<Vec<Vec<[f32; 4]>>> {
        map_layers(doc, role)
            .into_iter()
            .map(|l| alone(doc, frame, rasters, l, scale))
            .collect()
    };
    over(Role::Normal)?.iter().for_each(|px| {
        normal.par_iter_mut().zip(px).for_each(|(nm, p)| {
            if p[3] <= 0.0 {
                return;
            }
            let painted = [0, 1, 2].map(|c| (p[c] / p[3]) * 2.0 - 1.0);
            *nm = unit([0, 1, 2].map(|c| nm[c] * (1.0 - p[3]) + painted[c] * p[3]));
        })
    });
    let traits = traits(doc)?;
    let shiny = traits.values().any(|t| t.shine > 0.0);
    let glowing = traits.values().any(|t| t.emissive);
    let (zmin, zmax) = traits
        .values()
        .fold((0.0f32, 0.0f32), |(lo, hi), t| (lo.min(t.z), hi.max(t.z)));
    let span = zmax - zmin;
    let mut specular: Vec<[f32; 3]> = match shiny {
        true => tinted(doc, frame, rasters, scale, &traits, |t| {
            Tint::Flat(t.specular.map(|c| c * t.shine))
        })?
        .into_par_iter()
        .map(|p| match p[3] > 0.0 {
            true => [p[0] / p[3], p[1] / p[3], p[2] / p[3]],
            false => [0.0; 3],
        })
        .collect(),
        false => vec![[0.0; 3]; n],
    };
    over(Role::Specular)?.iter().for_each(|px| {
        specular.par_iter_mut().zip(px).for_each(|(s, p)| {
            if p[3] <= 0.0 {
                return;
            }
            *s = [0, 1, 2].map(|c| s[c] * (1.0 - p[3]) + p[c]);
        });
    });
    let marks: Option<Vec<[f32; 4]>> = match glowing || span > 0.0 || zmin != 0.0 {
        true => Some(tinted(doc, frame, rasters, scale, &traits, |t| {
            Tint::Flat([
                if t.emissive { 1.0 } else { 0.0 },
                match span > 0.0 {
                    true => (t.z - zmin) / span,
                    false => 0.0,
                },
                0.0,
            ])
        })?),
        false => None,
    };
    let mut emission: Vec<[f32; 4]> = match (glowing, &marks) {
        (true, Some(m)) => tinted(doc, frame, rasters, scale, &traits, |t| match t.emissive {
            true => Tint::Own,
            false => Tint::Flat([0.0; 3]),
        })?
        .into_par_iter()
        .zip(m.par_iter())
        .map(|(e, m)| [e[0], e[1], e[2], m[0].clamp(0.0, 1.0)])
        .collect(),
        _ => vec![[0.0; 4]; n],
    };
    over(Role::Emission)?.iter().for_each(|px| {
        emission
            .par_iter_mut()
            .zip(px)
            .for_each(|(e, p)| *e = [0, 1, 2, 3].map(|c| p[c] + e[c] * (1.0 - p[3])));
    });
    let z: Vec<f32> = match &marks {
        Some(m) => m
            .par_iter()
            .map(|p| match p[3] > 0.0 {
                true => (zmin + p[1] / p[3] * span) * scale,
                false => 0.0,
            })
            .collect(),
        None => vec![0.0; n],
    };
    let elevation: Vec<f32> = rise.par_iter().zip(&z).map(|(r, z)| r + z).collect();
    let camera = doc.canvas.width.max(doc.canvas.height) as f32 * scale;
    let depth: Vec<f32> = (0..n)
        .into_par_iter()
        .map(|i| match cover[i] > 0.0 {
            true => {
                let plane = planes[i] / cover[i];
                let away = (plane - elevation[i] / camera).max(-0.25);
                (NEAREST / (1.0 + away)).clamp(0.0, 1.0)
            }
            false => 0.0,
        })
        .collect();
    let reach = match pixel {
        true => 1.0,
        false => (settings.surface.bevel * scale * 0.25).max(1.0),
    };
    let occlusion = occlusion(&elevation, &cover, w, h, reach);
    Ok(MapSet {
        width: w,
        height: h,
        cover,
        normal,
        rise,
        specular,
        emission,
        depth,
        occlusion,
    })
}

struct Lamp {
    kind: LightKind,
    at: [f32; 2],
    height: f32,
    range: f32,
    rgb: [f32; 3],
    toward: [f32; 3],
}

fn lamps(lights: &[Light], scale: f32) -> Result<Vec<Lamp>> {
    lights
        .iter()
        .map(|l| {
            let [r, g, b, _] = parse_hex(&l.color)?;
            let toward = match l.kind {
                LightKind::Directional => {
                    let len = libm::hypotf(l.at[0], l.at[1]).max(1e-6);
                    let angle = l.height * std::f32::consts::FRAC_PI_2;
                    let (up, along) = (libm::sinf(angle), libm::cosf(angle));
                    unit([l.at[0] / len * along, -l.at[1] / len * along, up])
                }
                LightKind::Point => [0.0, 0.0, 1.0],
            };
            Ok(Lamp {
                kind: l.kind,
                at: [l.at[0] * scale, l.at[1] * scale],
                height: l.height * scale,
                range: l.range * scale,
                rgb: [r, g, b].map(|c| c * l.energy),
                toward,
            })
        })
        .collect()
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub struct Lit {
    set: MapSet,
    colour: Vec<[f32; 4]>,
    ambient: [f32; 3],
    shininess: f32,
    scale: f32,
}

impl Lit {
    pub fn prepare(doc: &Document, frame: u32, rasters: &Rasters, scale: f32) -> Result<Lit> {
        let settings = settings(doc);
        let set = render(doc, frame, rasters, scale)?;
        let image = export::render(
            doc,
            frame,
            rasters,
            &Target::Frame,
            Options {
                background: true,
                ..options(doc, scale)
            },
        )?;
        let [r, g, b, _] = parse_hex(&settings.ambient)?;
        Ok(Lit {
            set,
            colour: floats(&image)?,
            ambient: [r, g, b],
            shininess: settings.shininess,
            scale,
        })
    }

    pub fn size(&self) -> (usize, usize) {
        (self.set.width, self.set.height)
    }

    pub fn maps(&self) -> &MapSet {
        &self.set
    }

    fn texel(&self, i: usize, lamps: &[Lamp]) -> [f32; 4] {
        let p = self.colour[i];
        let a = p[3];
        let base = match a > 0.0 {
            true => [p[0] / a, p[1] / a, p[2] / a],
            false => [0.0; 3],
        };
        let normal = self.set.normal[i];
        let specular = self.set.specular[i];
        let shiny = specular.iter().any(|s| *s > 0.0);
        let at = [(i % self.set.width) as f32 + 0.5, (i / self.set.width) as f32 + 0.5];
        let (light, shine) = lamps.iter().fold((self.ambient, [0.0f32; 3]), |(sum, shine), lamp| {
            let (dir, fade) = match lamp.kind {
                LightKind::Point => {
                    let (dx, dy) = (lamp.at[0] - at[0], lamp.at[1] - at[1]);
                    let reach = (1.0 - libm::hypotf(dx, dy) / lamp.range).clamp(0.0, 1.0);
                    (unit([dx, -dy, lamp.height]), reach * reach)
                }
                LightKind::Directional => (lamp.toward, 1.0),
            };
            let facing = dot(normal, dir).max(0.0);
            let gloss = match shiny && facing > 0.0 {
                true => libm::powf(
                    dot(normal, unit([dir[0], dir[1], dir[2] + 1.0])).max(0.0),
                    self.shininess,
                ),
                false => 0.0,
            };
            (
                [0, 1, 2].map(|c| sum[c] + lamp.rgb[c] * fade * facing),
                [0, 1, 2].map(|c| shine[c] + lamp.rgb[c] * fade * gloss * specular[c]),
            )
        });
        let e = self.set.emission[i];
        let lit = [0, 1, 2].map(|c| (base[c] * light[c] + shine[c]).clamp(0.0, 1.0) * a);
        let alpha = e[3] + a * (1.0 - e[3]);
        match alpha > 0.0 {
            true => {
                let [r, g, b] = [0, 1, 2].map(|c| ((e[c] + lit[c] * (1.0 - e[3])) / alpha).clamp(0.0, 1.0));
                [r, g, b, alpha.clamp(0.0, 1.0)]
            }
            false => [0.0; 4],
        }
    }

    pub fn shade(&self, lights: &[Light]) -> Result<Frame> {
        let lamps = lamps(lights, self.scale)?;
        let byte = |v: f32| (v * 255.0).round() as u8;
        let rgba = (0..self.colour.len())
            .into_par_iter()
            .flat_map_iter(|i| self.texel(i, &lamps).map(byte))
            .collect();
        Ok(Frame {
            width: self.set.width as u32,
            height: self.set.height as u32,
            rgba,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Problem {
    Inward,
    Flat,
    Stray,
    Spill,
    Seam,
}

impl Problem {
    pub fn name(self) -> &'static str {
        match self {
            Problem::Inward => "inward",
            Problem::Flat => "flat",
            Problem::Stray => "stray",
            Problem::Spill => "spill",
            Problem::Seam => "seam",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub problem: Problem,
    pub frame: u32,
    pub area: [f32; 4],
    pub pixels: usize,
    pub note: String,
}

const NOISE: usize = 4;

pub fn area_of(mask: &[bool], w: usize, scale: f32) -> Option<([f32; 4], usize)> {
    let (count, [x0, y0, x1, y1]) = mask.iter().enumerate().filter(|(_, on)| **on).fold(
        (0usize, [usize::MAX, usize::MAX, 0, 0]),
        |(n, [x0, y0, x1, y1]), (i, _)| {
            let (x, y) = (i % w, i / w);
            (n + 1, [x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)])
        },
    );
    (count >= NOISE).then(|| ([x0, y0, x1 - x0, y1 - y0].map(|v| v as f32 / scale), count))
}

pub fn check(doc: &Document, frame: u32, rasters: &Rasters, scale: f32) -> Result<(Vec<Finding>, MapSet)> {
    let set = render(doc, frame, rasters, scale)?;
    let n = set.width * set.height;
    let w = set.width;
    let mut out = Vec::new();
    let mut found = |problem: Problem, mask: &[bool], note: String| {
        if let Some((area, pixels)) = area_of(mask, w, scale) {
            out.push(Finding {
                problem,
                frame,
                area,
                pixels,
                note,
            });
        }
    };
    let inward: Vec<bool> = (0..n).map(|i| set.cover[i] >= 0.5 && set.normal[i][2] < 0.0).collect();
    found(
        Problem::Inward,
        &inward,
        "normals point into the surface here (blue under 128 in the normal map): a light behind the picture would light it; paint them facing out".into(),
    );
    let picture = floats(&export::render(
        doc,
        frame,
        rasters,
        &Target::Frame,
        options(doc, scale),
    )?)?;
    let empty = |i: usize| set.cover[i] < 0.05 && picture.get(i).is_none_or(|p| p[3] < 0.05);
    let spill: Vec<bool> = (0..n).map(|i| set.emission[i][3] > 0.1 && empty(i)).collect();
    found(
        Problem::Spill,
        &spill,
        "the glow is bigger than what is drawn: emission where the picture is empty".into(),
    );
    [Role::Height, Role::Normal, Role::Specular, Role::Emission]
        .iter()
        .filter(|r| **r != Role::Emission)
        .try_for_each(|role| {
            map_layers(doc, *role).into_iter().try_for_each(|layer| {
                let px = alone(doc, frame, rasters, layer, scale)?;
                let stray: Vec<bool> = (0..n).map(|i| px[i][3] > 0.5 && empty(i)).collect();
                found(
                    Problem::Stray,
                    &stray,
                    format!(
                        "the {} layer {} is painted where nothing is drawn: those pixels change no map",
                        role.name(),
                        layer.name
                    ),
                );
                Ok::<_, anyhow::Error>(())
            })
        })?;
    let mut taken = vec![false; n];
    doc.layers
        .iter()
        .filter(|l| is_surface(l))
        .rev()
        .try_for_each(|layer| {
            let px = alone(doc, frame, rasters, layer, scale)?;
            let mine: Vec<bool> = (0..n).map(|i| px[i][3] >= 0.5 && !taken[i]).collect();
            let shown = mine.iter().filter(|m| **m).count();
            let flat = (0..n).filter(|i| mine[*i] && set.normal[*i][2] > 0.9995).count();
            if shown >= 64 && flat as f32 >= 0.995 * shown as f32 {
                found(
                    Problem::Flat,
                    &mine,
                    format!(
                        "{} reads flat under light: no bevel, paint body, height or normal layer shapes it",
                        layer.name
                    ),
                );
            }
            taken.iter_mut().zip(&mine).for_each(|(t, m)| *t |= *m);
            Ok::<_, anyhow::Error>(())
        })?;
    Ok((out, set))
}

pub fn seam(before: (&MapSet, u64), after: (&MapSet, u64), frame: u32, scale: f32) -> Option<Finding> {
    if before.1 != after.1 || before.0.width != after.0.width || before.0.height != after.0.height {
        return None;
    }
    let jumped: Vec<bool> = before
        .0
        .normal
        .iter()
        .zip(&after.0.normal)
        .map(|(a, b)| dot(*a, *b) < 0.995)
        .collect();
    let (area, pixels) = area_of(&jumped, before.0.width, scale)?;
    Some(Finding {
        problem: Problem::Seam,
        frame,
        area,
        pixels,
        note: format!(
            "the normal map changes from frame {} to {frame} while the picture does not: in the game the light jumps on a still drawing",
            frame.saturating_sub(1)
        ),
    })
}

pub fn lit(doc: &Document, frame: u32, rasters: &Rasters, scale: f32) -> Result<Frame> {
    let lights = settings(doc).lights;
    if lights.is_empty() {
        bail!("no lights to light it with: klad lights <bundle> --add point --at x,y");
    }
    Lit::prepare(doc, frame, rasters, scale)?.shade(&lights)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{View, compose, rasterize_nodes_at};
    use klad_core::doc::{Node, Shape};
    use klad_core::maps::{Light, Profile};

    fn paint_doc(w: u32, h: u32) -> (Document, Rasters) {
        let mut doc = Document::new(w, h);
        doc.layers = vec![Layer::paint("p", "p", "cp")];
        (doc, Rasters::new())
    }

    fn stroked(doc: &Document, rasters: &mut Rasters, layer: &str, cel: &str, strokes: &str) {
        let ops = klad_core::ops::parse_lines(strokes).unwrap();
        let brushes = klad_brush::BrushSet::presets();
        let marks: Vec<klad_core::stroke::Mark> = ops
            .into_iter()
            .filter_map(|op| match op {
                klad_core::ops::Op::Stroke(spec) if spec.layer == layer => Some(spec.build().unwrap()),
                _ => None,
            })
            .flatten()
            .enumerate()
            .map(|(i, mut s)| {
                s.id = i as u32 + 1;
                klad_core::stroke::Mark::Stroke(s)
            })
            .collect();
        let raster = klad_brush::replay(doc.canvas.width, doc.canvas.height, &marks, &brushes)
            .unwrap()
            .raster;
        rasters.insert(cel.to_string(), raster);
    }

    fn disc(id: &str, cx: f32, cy: f32, r: f32, color: &str) -> Layer {
        Layer::vector(
            id,
            id,
            &format!("c{id}"),
            vec![Node::new(&format!("n{id}"), Shape::Ellipse { cx, cy, rx: r, ry: r }).filled(color)],
        )
    }

    fn rasters(doc: &Document) -> Rasters {
        rasters_at(doc, 1.0)
    }

    fn rasters_at(doc: &Document, detail: f32) -> Rasters {
        doc.layers
            .iter()
            .filter(|l| l.kind() == Kind::Vector)
            .filter_map(|l| l.cels().first())
            .map(|c| {
                let raster = rasterize_nodes_at(&c.nodes, doc.canvas.width, doc.canvas.height, detail).unwrap();
                (c.id.clone(), raster)
            })
            .collect()
    }

    fn doc_of(layers: Vec<Layer>) -> Document {
        let mut doc = Document::new(128, 128);
        doc.layers = layers;
        doc
    }

    fn normal_at(doc: &Document, rasters: &Rasters, x: usize, y: usize) -> [f32; 3] {
        let set = render(doc, 0, rasters, 1.0).unwrap();
        set.normal[y * set.width + x]
    }

    #[test]
    fn a_disc_faces_out_at_its_rim_and_forward_in_the_middle() {
        let doc = doc_of(vec![disc("a", 64.0, 64.0, 40.0, "#c08040")]);
        let r = rasters(&doc);
        let set = render(&doc, 0, &r, 1.0).unwrap();
        let at = |x: usize, y: usize| set.normal[y * set.width + x];
        assert!(at(64, 64)[2] > 0.99, "the middle faces the viewer: {:?}", at(64, 64));
        assert!(at(27, 64)[0] < -0.3, "the left rim faces left: {:?}", at(27, 64));
        assert!(at(101, 64)[0] > 0.3, "the right rim faces right: {:?}", at(101, 64));
        assert!(at(64, 27)[1] > 0.3, "the top rim faces up, green up: {:?}", at(64, 27));
        assert!(at(64, 101)[1] < -0.3, "the bottom rim faces down: {:?}", at(64, 101));
        let frame = set.frame(Map::Normal);
        assert_eq!(frame.pixel(2, 2), [128, 128, 255, 0], "outside: flat and clear");
        assert!(frame.pixel(27, 64)[0] < 100 && frame.pixel(101, 64)[0] > 155);
        assert_eq!(frame.pixel(64, 64)[3], 255);
    }

    #[test]
    fn the_bevel_sets_how_much_of_the_form_rounds() {
        let mut doc = doc_of(vec![disc("a", 64.0, 64.0, 40.0, "#c08040")]);
        let r = rasters(&doc);
        let mut wide = Maps::default();
        wide.surface.bevel = 30.0;
        doc.maps = Some(wide);
        assert!(
            normal_at(&doc, &r, 39, 64)[0] < -0.2,
            "a wide bevel still turns 15 px in"
        );
        let mut narrow = Maps::default();
        narrow.surface.bevel = 4.0;
        doc.maps = Some(narrow);
        assert!(
            normal_at(&doc, &r, 39, 64)[0].abs() < 0.01,
            "a narrow one is flat there"
        );
        let mut flat = Maps::default();
        flat.surface.profile = Profile::Flat;
        doc.maps = Some(flat);
        assert!(normal_at(&doc, &r, 27, 64)[2] > 0.999, "a flat profile has no rim");
    }

    #[test]
    fn a_height_layer_lifts_its_surface_and_never_shows_in_the_colour() {
        let base = doc_of(vec![disc("a", 64.0, 64.0, 40.0, "#c08040")]);
        let mut bump = disc("h", 64.0, 64.0, 10.0, "#ffffff");
        bump.map = Some(Role::Height);
        let doc = doc_of(vec![base.layers[0].clone(), bump]);
        doc.validate().unwrap();
        let r = rasters(&doc);
        assert_eq!(
            compose(&doc, 0, &r, View::scaled(1.0)).unwrap().hash(),
            compose(&base, 0, &rasters(&base), View::scaled(1.0)).unwrap().hash(),
            "a map layer is not drawn in the colour"
        );
        let set = render(&doc, 0, &r, 1.0).unwrap();
        let flat = render(&base, 0, &rasters(&base), 1.0).unwrap();
        let i = 64 * 128 + 64;
        assert!(
            set.rise[i] > flat.rise[i] + 5.0,
            "white raises: {} vs {}",
            set.rise[i],
            flat.rise[i]
        );
        assert!(set.normal[64 * 128 + 54][0] < -0.3, "the bump's left side faces left");
        assert!(flat.normal[64 * 128 + 54][2] > 0.999);
    }

    #[test]
    fn a_top_layer_with_its_own_surface_gets_its_own_rim() {
        let small = disc("b", 64.0, 64.0, 16.0, "#80a0c0");
        let together = doc_of(vec![disc("a", 64.0, 64.0, 50.0, "#c08040"), small.clone()]);
        let r = rasters(&together);
        assert!(
            normal_at(&together, &r, 50, 64)[2] > 0.999,
            "inside one surface the small disc is part of the plateau"
        );
        let mut own = small;
        own.surface = Some(Surface::default());
        let apart = doc_of(vec![together.layers[0].clone(), own]);
        apart.validate().unwrap();
        assert!(
            normal_at(&apart, &r, 50, 64)[0] < -0.3,
            "its own surface has its own rim"
        );
        let mut hidden = apart.clone();
        hidden.layers.swap(0, 1);
        assert!(
            normal_at(&hidden, &r, 50, 64)[2] > 0.999,
            "covered by the big disc, the small one's rim is not seen"
        );
    }

    #[test]
    fn paint_body_raises_ridges_and_its_share_is_a_setting() {
        let mut doc = Document::new(128, 128);
        doc.layers = vec![Layer::paint("p", "p", "cp")];
        let mut raster = klad_brush::Raster::new(128, 128);
        raster.ensure(0).chunks_mut(4).for_each(|px| {
            px.copy_from_slice(&[0.5, 0.5, 0.5, 1.0].map(half::f16::from_f32));
        });
        raster
            .heights_mut()
            .ensure(0)
            .iter_mut()
            .enumerate()
            .for_each(|(i, h)| {
                if (60..68).contains(&(i % TILE)) {
                    *h = half::f16::from_f32(1.0);
                }
            });
        let r: Rasters = [("cp".to_string(), raster)].into_iter().collect();
        let mut maps = Maps::default();
        maps.surface.bevel = 0.0;
        doc.maps = Some(maps.clone());
        assert!(
            normal_at(&doc, &r, 59, 64)[0] < -0.2,
            "the ridge's left side faces left"
        );
        maps.surface.body = 0.0;
        doc.maps = Some(maps);
        assert!(normal_at(&doc, &r, 59, 64)[2] > 0.999, "without body the paint is flat");
    }

    #[test]
    fn normal_specular_and_emission_layers_paint_their_maps() {
        let mut facing = disc("n", 64.0, 64.0, 12.0, "#ff8080");
        facing.map = Some(Role::Normal);
        let mut shiny = disc("s", 30.0, 30.0, 8.0, "#ffffff");
        shiny.map = Some(Role::Specular);
        let mut glow = disc("e", 100.0, 100.0, 8.0, "#ffcc00");
        glow.map = Some(Role::Emission);
        let doc = doc_of(vec![disc("a", 64.0, 64.0, 60.0, "#406080"), facing, shiny, glow]);
        doc.validate().unwrap();
        let set = render(&doc, 0, &rasters(&doc), 1.0).unwrap();
        let n = set.normal[64 * 128 + 64];
        assert!(n[0] > 0.99, "#ff8080 is a normal facing right: {n:?}");
        assert!(set.specular[30 * 128 + 30][0] > 0.99 && set.specular[64 * 128 + 64] == [0.0; 3]);
        assert_eq!(set.frame(Map::Emission).pixel(100, 100), [255, 204, 0, 255]);
        assert_eq!(set.frame(Map::Emission).pixel(64, 64)[3], 0);
    }

    #[test]
    fn the_side_facing_a_light_is_lit() {
        let mut doc = doc_of(vec![disc("a", 64.0, 64.0, 40.0, "#ffffff")]);
        let r = rasters(&doc);
        assert!(lit(&doc, 0, &r, 1.0).is_err_and(|e| e.to_string().contains("no lights")));
        let mut maps = Maps {
            lights: vec![Light {
                at: [0.0, 64.0],
                height: 30.0,
                ..Light::default()
            }],
            ..Maps::default()
        };
        doc.maps = Some(maps.clone());
        let frame = lit(&doc, 0, &r, 1.0).unwrap();
        let (left, right) = (frame.pixel(27, 64)[0], frame.pixel(101, 64)[0]);
        assert!(
            left > right + 40,
            "a lamp on the left lights the left rim: {left} vs {right}"
        );
        maps.lights = vec![Light::directional([1.0, 0.0], 0.3)];
        doc.maps = Some(maps);
        let frame = lit(&doc, 0, &r, 1.0).unwrap();
        assert!(
            frame.pixel(101, 64)[0] > frame.pixel(27, 64)[0] + 40,
            "light from the right"
        );
        let ambient = parse_hex(&Maps::default().ambient).unwrap();
        assert!(
            (frame.pixel(27, 64)[0] as f32 / 255.0 - ambient[0]).abs() < 0.02,
            "the side turned away gets the ambient only"
        );
    }

    #[test]
    fn maps_scale_with_the_export() {
        let doc = doc_of(vec![disc("a", 64.0, 64.0, 40.0, "#c08040")]);
        let r = rasters_at(&doc, 2.0);
        let set = render(&doc, 0, &r, 2.0).unwrap();
        assert_eq!((set.width, set.height), (256, 256));
        assert!(set.normal[128 * 256 + 54][0] < -0.3, "the left rim at twice the size");
        assert!(set.normal[128 * 256 + 128][2] > 0.99);
    }

    #[test]
    fn the_edge_distance_is_euclidean() {
        let (w, h) = (9, 9);
        let alpha: Vec<f32> = (0..w * h)
            .map(|i| {
                if (1..8).contains(&(i % w)) && (1..8).contains(&(i / w)) {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();
        let d = edge_distance(&alpha, w, h);
        assert_eq!(d[0], 0.0);
        assert_eq!(d[4 * w + 4], 4.0, "the middle is 4 px from the frame of empty pixels");
        assert_eq!(d[w + 1], 1.0);
        assert!(
            edge_distance(&[1.0; 16], 4, 4).iter().all(|v| *v >= FAR),
            "nothing outside: no rim"
        );
    }

    #[test]
    fn a_stroke_on_a_height_layer_bends_the_normals_where_it_lands() {
        let (mut doc, mut r) = paint_doc(160, 120);
        doc.layers[0] = disc("a", 80.0, 60.0, 70.0, "#a08060");
        let mut bump = Layer::paint("h", "h", "ch");
        bump.map = Some(Role::Height);
        doc.layers.push(bump);
        doc.validate().unwrap();
        r.extend(rasters(&doc));
        let flat = render(&doc, 0, &r, 1.0).unwrap();
        stroked(
            &doc,
            &mut r,
            "h",
            "ch",
            r##"{"op":"stroke","layer":"h","brush":"soft","size":30,"color":"#ffffff","path":"M40 60 L120 60"}"##,
        );
        let raised = render(&doc, 0, &r, 1.0).unwrap();
        let at = |x: usize, y: usize| y * raised.width + x;
        assert!(flat.normal[at(80, 50)][2] > 0.999, "flat before the stroke");
        assert!(
            raised.normal[at(80, 50)][1] > 0.1,
            "above the soft ridge the surface faces up: {:?}",
            raised.normal[at(80, 50)]
        );
        assert!(raised.normal[at(80, 70)][1] < -0.1, "below it, down");
        assert!(raised.rise[at(80, 60)] > flat.rise[at(80, 60)] + 4.0);
    }

    #[test]
    fn a_stroke_on_a_normal_layer_writes_its_direction() {
        let (mut doc, mut r) = paint_doc(120, 120);
        doc.layers[0] = disc("a", 60.0, 60.0, 55.0, "#a08060");
        let mut facing = Layer::paint("n", "n", "cn");
        facing.map = Some(Role::Normal);
        doc.layers.push(facing);
        r.extend(rasters(&doc));
        stroked(
            &doc,
            &mut r,
            "n",
            "cn",
            r##"{"op":"stroke","layer":"n","brush":"ink","size":20,"normal":"e","path":"M30 60 L90 60"}"##,
        );
        let set = render(&doc, 0, &r, 1.0).unwrap();
        let n = set.normal[60 * set.width + 60];
        let east = klad_core::maps::named("e").unwrap();
        assert!(dot(n, east) > 0.995, "{n:?} is the east lean {east:?}");
        let outside = set.normal[20 * set.width + 60];
        assert!(
            outside[2] > 0.99,
            "the derived normal stays where nothing was painted: {outside:?}"
        );
    }

    #[test]
    fn a_tilted_pen_paints_the_direction_it_leans() {
        let (doc, mut r) = paint_doc(80, 40);
        let mut def = klad_brush::BrushSet::presets().def("ink").unwrap().clone();
        def.color = Some(klad_core::brushes::ColorDef::tilted());
        let mut brushes = klad_brush::BrushSet::presets();
        brushes.insert("ink-tilt", def);
        let mut stroke = klad_core::stroke::Stroke::new(1, "ink-tilt", [0.0, 0.0, 0.0, 1.0], 16.0, 1);
        stroke.samples = [10.0, 40.0, 70.0]
            .iter()
            .map(|x| klad_core::stroke::Sample::quantize(*x, 20.0, 1.0, (0.6, 0.0), 8.0))
            .collect();
        let raster = klad_brush::replay(80, 40, &[stroke.into()], &brushes).unwrap().raster;
        r.insert("cp".into(), raster);
        let image = export::render(&doc, 0, &r, &Target::Frame, options(&doc, 1.0)).unwrap();
        let px = floats(&image).unwrap()[20 * 80 + 40];
        let n = klad_core::maps::decoded([0, 1, 2].map(|c| px[c] / px[3]));
        assert!(
            n[0] > 0.5 && n[1].abs() < 0.05,
            "leaning right paints a surface facing right: {n:?}"
        );
    }

    #[test]
    fn shine_and_glow_come_from_the_layers_that_show() {
        let mut shiny = disc("s", 40.0, 64.0, 20.0, "#406080");
        shiny.shine = Some(0.6);
        shiny.specular = Some("#ff8000".into());
        let mut lamp = disc("e", 90.0, 64.0, 16.0, "#ffd040");
        lamp.emissive = true;
        let mut cover = disc("c", 100.0, 64.0, 6.0, "#202020");
        cover.shine = Some(0.0);
        let doc = doc_of(vec![disc("a", 64.0, 64.0, 60.0, "#808080"), shiny, lamp, cover]);
        doc.validate().unwrap();
        let r = rasters(&doc);
        let set = render(&doc, 0, &r, 1.0).unwrap();
        let s = set.specular[64 * 128 + 40];
        assert!(
            (s[0] - 0.6).abs() < 0.02 && (s[1] - 0.3).abs() < 0.02 && s[2] < 0.01,
            "shine times its colour: {s:?}"
        );
        assert_eq!(set.specular[20 * 128 + 64], [0.0; 3], "matte by default");
        let glow = set.frame(Map::Emission);
        assert_eq!(
            glow.pixel(84, 64),
            [255, 208, 64, 255],
            "an emissive layer glows in its own colour"
        );
        assert_eq!(glow.pixel(100, 64)[3], 0, "a layer drawn over it hides the glow");
        assert_eq!(glow.pixel(40, 64)[3], 0);
        let mut lit_doc = doc.clone();
        lit_doc.maps = Some(Maps {
            lights: vec![Light {
                at: [40.0, 64.0],
                height: 40.0,
                ..Light::default()
            }],
            ..Maps::default()
        });
        let frame = lit(&lit_doc, 0, &r, 1.0).unwrap();
        assert_eq!(
            frame.pixel(84, 64),
            [255, 208, 64, 255],
            "glow is at full brightness, lit or not"
        );
        let mut matte = lit_doc.clone();
        matte.layers[1].shine = None;
        let dull = lit(&matte, 0, &r, 1.0).unwrap();
        assert!(
            frame.pixel(40, 64)[0] > dull.pixel(40, 64)[0] + 30,
            "the shiny disc catches an orange highlight: {:?} vs {:?}",
            frame.pixel(40, 64),
            dull.pixel(40, 64)
        );
    }

    #[test]
    fn nearer_planes_and_raised_paint_are_whiter_in_the_depth_map() {
        let mut far = disc("f", 32.0, 64.0, 24.0, "#406080");
        far.depth = Some(2.0);
        let mut near = disc("n", 96.0, 64.0, 24.0, "#806040");
        near.depth = Some(0.0);
        let doc = doc_of(vec![far, near]);
        let r = rasters(&doc);
        let set = render(&doc, 0, &r, 1.0).unwrap();
        let d = |x: usize| set.depth[64 * 128 + x];
        assert!(d(96) > d(32) * 2.5, "near {} far {}", d(96), d(32));
        assert!(
            (d(32) - NEAREST / 3.0).abs() < 0.01,
            "depth 2 is a third of the nearest plane"
        );
        assert!(d(96 - 23) < d(96), "the rim is lower than the raised middle");
        let mut forward = doc.clone();
        forward.layers[0].z = Some(60.0);
        forward.validate().unwrap();
        let moved = render(&forward, 0, &r, 1.0).unwrap();
        assert!(moved.depth[64 * 128 + 32] > d(32) + 0.01, "z brings a layer nearer");
        let mut inverted = render(&doc, 0, &r, 1.0).unwrap();
        inverted.invert_depth();
        assert!(inverted.depth[64 * 128 + 96] < inverted.depth[64 * 128 + 32]);
        assert_eq!(inverted.depth[0], 1.0, "nothing there is far, white when inverted");
        let wide = set.straight(Map::Depth);
        assert_eq!(wide[0][3], 0.0);
    }

    #[test]
    fn a_crease_is_darker_in_the_occlusion_map() {
        let mut left = disc("l", 44.0, 64.0, 26.0, "#a08060");
        left.surface = Some(Surface {
            bevel: 20.0,
            ..Surface::default()
        });
        let mut right = disc("r", 84.0, 64.0, 26.0, "#a08060");
        right.surface = Some(Surface {
            bevel: 20.0,
            ..Surface::default()
        });
        let mut ground = Layer::vector(
            "g",
            "g",
            "cg",
            vec![
                Node::new(
                    "ng",
                    Shape::Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 128.0,
                        h: 128.0,
                        radius: 0.0,
                    },
                )
                .filled("#406040"),
            ],
        );
        ground.surface = Some(Surface {
            bevel: 0.0,
            ..Surface::default()
        });
        let doc = doc_of(vec![ground, left, right]);
        let r = rasters(&doc);
        let set = render(&doc, 0, &r, 1.0).unwrap();
        let o = |x: usize, y: usize| set.occlusion[y * 128 + x];
        assert!(
            o(59, 64) < 0.8,
            "where the near form meets the one behind: {}",
            o(59, 64)
        );
        assert!(o(44, 64) > 0.95, "the top of a form is open: {}", o(44, 64));
        assert!(
            o(44, 64 - 28) < o(44, 4),
            "the ground at the foot of a form is darker than open ground"
        );
    }

    #[test]
    fn a_pixel_surface_steps_by_whole_pixels_in_palette_directions() {
        let mut doc = Document::new(16, 16);
        let mut sprite = Layer::paint("p", "p", "cp");
        if let LayerContent::Paint { pixel, .. } = &mut sprite.content {
            *pixel = true;
        }
        doc.layers = vec![sprite];
        doc.maps = Some(Maps {
            surface: Surface {
                bevel: 3.0,
                ..Surface::default()
            },
            ..Maps::default()
        });
        let mut raster = klad_brush::Raster::new(16, 16);
        let tile = raster.ensure(0);
        (0..16 * 16)
            .filter(|i| (3..13).contains(&(i % 16)) && (3..13).contains(&(i / 16)))
            .for_each(|i| {
                let at = ((i / 16) * TILE + i % 16) * 4;
                tile[at..at + 4].copy_from_slice(&[0.4, 0.3, 0.2, 1.0].map(f16::from_f32));
            });
        let r: Rasters = [("cp".to_string(), raster)].into_iter().collect();
        let set = render(&doc, 0, &r, 1.0).unwrap();
        let choices = palette(9);
        (0..256).filter(|i| set.cover[*i] > 0.0).for_each(|i| {
            assert!(
                choices.iter().any(|(_, n)| dot(*n, set.normal[i]) > 0.9999),
                "pixel {i} faces {:?}, not one of the nine directions",
                set.normal[i]
            );
        });
        assert!(set.normal[8 * 16 + 3][0] < -0.5, "the left edge leans left");
        assert_eq!(set.normal[8 * 16 + 8], [0.0, 0.0, 1.0], "the middle faces out");
        let big = render(&doc, 0, &r, 4.0).unwrap();
        assert_eq!((big.width, big.height), (64, 64));
        (0..64 * 64).for_each(|i| {
            let (x, y) = (i % 64, i / 64);
            assert_eq!(big.normal[i], set.normal[(y / 4) * 16 + x / 4], "sharp at 4x");
        });
    }

    #[test]
    fn the_map_check_finds_what_goes_wrong_in_practice() {
        let kinds = |doc: &Document| -> Vec<&'static str> {
            let (found, _) = check(doc, 0, &rasters(doc), 1.0).unwrap();
            found.iter().map(|f| f.problem.name()).collect()
        };
        let clean = doc_of(vec![disc("a", 64.0, 64.0, 40.0, "#c08040")]);
        assert!(kinds(&clean).is_empty(), "a bevelled disc is fine: {:?}", kinds(&clean));
        let mut inward = disc("n", 64.0, 64.0, 10.0, "#808020");
        inward.map = Some(Role::Normal);
        let mut doc = clean.clone();
        doc.layers.push(inward);
        assert_eq!(kinds(&doc), ["inward"], "blue under 128 points into the surface");
        let mut stray = disc("h", 120.0, 8.0, 6.0, "#ffffff");
        stray.map = Some(Role::Height);
        let mut doc = clean.clone();
        doc.layers.push(stray);
        assert_eq!(kinds(&doc), ["stray"], "height painted where nothing is drawn");
        let mut spill = disc("e", 64.0, 64.0, 60.0, "#ffd040");
        spill.map = Some(Role::Emission);
        let mut doc = clean.clone();
        doc.layers.push(spill);
        assert_eq!(kinds(&doc), ["spill"], "a glow wider than the disc");
        let mut flat = clean.clone();
        flat.layers[0].surface = Some(Surface {
            profile: Profile::Flat,
            ..Surface::default()
        });
        let (found, _) = check(&flat, 0, &rasters(&flat), 1.0).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].problem, Problem::Flat);
        assert!(found[0].note.contains("reads flat"), "{}", found[0].note);
        assert!(
            (found[0].area[0] - 24.0).abs() < 2.0 && (found[0].area[2] - 80.0).abs() < 3.0,
            "the finding boxes the disc: {:?}",
            found[0].area
        );
        let r = rasters(&clean);
        let still = render(&clean, 0, &r, 1.0).unwrap();
        let mut bumped = clean.clone();
        let mut bump = disc("b", 64.0, 64.0, 12.0, "#ffffff");
        bump.map = Some(Role::Height);
        bumped.layers.push(bump);
        let moved = render(&bumped, 0, &rasters(&bumped), 1.0).unwrap();
        let jump = seam((&still, 7), (&moved, 7), 5, 1.0).unwrap();
        assert_eq!((jump.problem, jump.frame), (Problem::Seam, 5));
        assert!(jump.note.contains("from frame 4 to 5"));
        assert!(
            seam((&still, 7), (&moved, 8), 5, 1.0).is_none(),
            "the picture changed too: no seam"
        );
        assert!(seam((&still, 7), (&still, 7), 5, 1.0).is_none());
    }
}
