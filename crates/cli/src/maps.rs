use crate::agent::{self, Output, parse_props};
use crate::layers;
use anyhow::{Result, anyhow, bail};
use klad_brush::BrushSet;
use klad_core::bundle::{Bundle, write_atomic};
use klad_core::maps::{Light, LightKind, Maps};
use klad_core::ops::Op;
use klad_render::maps::{Lit, Map};
use klad_render::{Frame, View};
use std::path::{Path, PathBuf};
use std::time::Instant;

const SURFACE_KEYS: &[&str] = &["bevel", "profile", "strength", "body", "range", "directions"];
const TOP_KEYS: &[&str] = &["ambient", "shininess"];

fn fitted(bundle: &Bundle, props: &[String], owner: Option<&str>) -> Result<Vec<String>> {
    props
        .iter()
        .map(|p| match p.split_once('=') {
            Some(("bevel", "fit")) => {
                let rasters = layers::load_at(bundle, 0, &BrushSet::for_bundle(bundle)?, 1.0)?;
                let bevel = klad_render::maps::fit_bevel(&bundle.doc, 0, &rasters, owner)?;
                eprintln!("bevel fitted to {bevel} px: as far as the deepest point of frame 0 lies from its outline");
                Ok(format!("bevel={bevel}"))
            }
            _ => Ok(p.clone()),
        })
        .collect()
}

pub fn maps(path: &Path, props: &[String], on: bool, off: bool, out: &Output) -> Result<()> {
    let bundle = Bundle::open(path)?;
    let props = &fitted(&bundle, props, None)?;
    if props.is_empty() && !on && !off {
        match &bundle.doc.maps {
            Some(maps) => println!("{}", serde_json::to_string(maps)?),
            None => println!(
                "maps are off: klad maps <bundle> --on, or set the surface: klad maps <bundle> bevel=8 profile=round strength=1"
            ),
        }
        return Ok(());
    }
    if on && off {
        bail!("--on or --off, not both");
    }
    let op = match off {
        true => Op::Maps(None),
        false => Op::Maps(Some(changed(bundle.doc.maps.clone().unwrap_or_default(), props)?)),
    };
    agent::edit(path, &[op], out)
}

fn changed(maps: Maps, props: &[String]) -> Result<Maps> {
    let mut value = serde_json::to_value(maps)?;
    parse_props(props)?.into_iter().try_for_each(|(key, v)| {
        let place = match key.as_str() {
            k if SURFACE_KEYS.contains(&k) => &mut value["surface"],
            k if TOP_KEYS.contains(&k) => &mut value,
            other => bail!(
                "maps has no {other:?}: the drawing's surface takes {} and the light preview {}",
                SURFACE_KEYS.join(", "),
                TOP_KEYS.join(", ")
            ),
        };
        place[key.as_str()] = v;
        Ok(())
    })?;
    let maps: Maps = serde_json::from_value(value)?;
    maps.validate()?;
    Ok(maps)
}

pub fn palette_colour(doc: &klad_core::doc::Document, text: &str) -> Result<String> {
    let wanted = klad_core::maps::direction(text)?;
    let directions = klad_render::maps::settings(doc).surface.directions_for(true);
    Ok(klad_core::maps::hex_of(match directions {
        0 => wanted,
        d => klad_core::maps::nearest(wanted, &klad_core::maps::palette(d)),
    }))
}

pub fn surface(path: &Path, args: &[String], directions: Option<u32>, off: bool, out: &Output) -> Result<()> {
    let bundle = Bundle::open(path)?;
    let doc = &bundle.doc;
    let (layer, props) = match args.first() {
        Some(first) if !first.contains('=') => (Some(doc.find_layer(first)?), &args[1..]),
        _ => (None, args),
    };
    if let Some(l) = layer
        && !doc.layers.iter().any(|t| t.id == l.id)
    {
        bail!(
            "{} is inside a group: a surface of its own is for layers at the top of the stack; give it to the group it is in",
            l.id
        );
    }
    let props = &fitted(&bundle, props, layer.map(|l| l.id.as_str()))?;
    let current = match layer {
        Some(l) => l.surface.clone(),
        None => doc.maps.as_ref().map(|m| m.surface.clone()),
    };
    if props.is_empty() && directions.is_none() && !off {
        match (&current, layer) {
            (Some(s), _) => println!("{}", serde_json::to_string(s)?),
            (None, Some(l)) => println!(
                "{} is part of the drawing's surface: klad surface <bundle> {} bevel=6 gives it its own",
                l.id, l.id
            ),
            (None, None) => println!("maps are off: klad surface <bundle> bevel=8 turns them on"),
        }
        return Ok(());
    }
    let ops = match (layer, off) {
        (Some(l), true) => vec![Op::Set {
            id: l.id.clone(),
            props: [("surface".to_string(), serde_json::Value::Null)].into_iter().collect(),
        }],
        (None, true) => {
            bail!("klad maps <bundle> --off turns the drawing's maps off; --off here is for a layer's own surface")
        }
        (layer, false) => {
            let base = current.unwrap_or_else(|| klad_render::maps::settings(doc).surface);
            let mut value = serde_json::to_value(base)?;
            parse_props(props)?.into_iter().try_for_each(|(key, v)| {
                if !SURFACE_KEYS.contains(&key.as_str()) {
                    bail!("a surface has no {key:?}: it takes {}", SURFACE_KEYS.join(", "));
                }
                value[key.as_str()] = v;
                Ok(())
            })?;
            if let Some(d) = directions {
                value["directions"] = serde_json::json!(d);
            }
            let surface: klad_core::maps::Surface = serde_json::from_value(value)?;
            surface.validate()?;
            match layer {
                Some(l) => vec![Op::Set {
                    id: l.id.clone(),
                    props: [("surface".to_string(), serde_json::to_value(&surface)?)]
                        .into_iter()
                        .collect(),
                }],
                None => vec![Op::Maps(Some(Maps {
                    surface,
                    ..doc.maps.clone().unwrap_or_default()
                }))],
            }
        }
    };
    agent::edit(path, &ops, out)
}

pub struct LightArgs {
    pub add: Option<String>,
    pub change: Option<usize>,
    pub rm: Option<usize>,
    pub clear: bool,
    pub at: Option<[f32; 2]>,
    pub height: Option<f32>,
    pub color: Option<String>,
    pub energy: Option<f32>,
    pub range: Option<f32>,
}

impl LightArgs {
    fn shaped(&self, mut light: Light) -> Light {
        if let Some(at) = self.at {
            light.at = at;
        }
        if let Some(height) = self.height {
            light.height = height;
        }
        if let Some(color) = &self.color {
            light.color = color.clone();
        }
        if let Some(energy) = self.energy {
            light.energy = energy;
        }
        if let Some(range) = self.range {
            light.range = range;
        }
        light
    }

    fn fields(&self) -> bool {
        self.at.is_some()
            || self.height.is_some()
            || self.color.is_some()
            || self.energy.is_some()
            || self.range.is_some()
    }
}

fn describe(i: usize, light: &Light) -> String {
    match light.kind {
        LightKind::Point => format!(
            "{i}  point at {},{} height {} px, {} x{} reaching {} px",
            light.at[0], light.at[1], light.height, light.color, light.energy, light.range
        ),
        LightKind::Directional => format!(
            "{i}  directional from {},{} height {}, {} x{}",
            light.at[0], light.at[1], light.height, light.color, light.energy
        ),
    }
}

pub fn lights(path: &Path, args: &LightArgs, out: &Output) -> Result<()> {
    let bundle = Bundle::open(path)?;
    let mut maps = bundle.doc.maps.clone().unwrap_or_default();
    let chosen = [args.add.is_some(), args.change.is_some(), args.rm.is_some(), args.clear]
        .iter()
        .filter(|on| **on)
        .count();
    if chosen > 1 {
        bail!("one of --add, --set, --rm or --clear at a time");
    }
    if chosen == 0 {
        if args.fields() {
            bail!("say what to do with the light: --add point|directional, or --set <n> to change light n");
        }
        match maps.lights.is_empty() {
            true => println!("no lights: klad lights <bundle> --add point --at x,y [--height 100] [--color #ffffff]"),
            false => maps
                .lights
                .iter()
                .enumerate()
                .for_each(|(i, l)| println!("{}", describe(i + 1, l))),
        }
        println!("ambient {}", maps.ambient);
        return Ok(());
    }
    let pick = |n: usize, count: usize| -> Result<usize> {
        match n {
            1.. if n <= count => Ok(n - 1),
            _ => Err(anyhow!("there is no light {n}: the drawing has {count}")),
        }
    };
    if let Some(kind) = &args.add {
        let light = match kind.as_str() {
            "point" => args.shaped(Light::default()),
            "directional" => args.shaped(Light::directional([-1.0, -1.0], 0.5)),
            other => bail!("--add takes point or directional, not {other:?}"),
        };
        maps.lights.push(light);
    }
    if let Some(n) = args.change {
        let at = pick(n, maps.lights.len())?;
        if !args.fields() {
            bail!("--set {n} needs what to change: --at, --height, --color, --energy or --range");
        }
        maps.lights[at] = args.shaped(maps.lights[at].clone());
    }
    if let Some(n) = args.rm {
        let at = pick(n, maps.lights.len())?;
        maps.lights.remove(at);
    }
    if args.clear {
        maps.lights.clear();
    }
    maps.validate()?;
    agent::edit(path, &[Op::Maps(Some(maps))], out)
}

fn crop(frame: &Frame, [x, y, w, h]: [i32; 4], scale: f32) -> Result<Frame> {
    let s = |v: i32| (v as f32 * scale).round() as i64;
    let (x, y, w, h) = (s(x), s(y), s(w).max(1), s(h).max(1));
    if x < 0 || y < 0 || x + w > frame.width as i64 || y + h > frame.height as i64 {
        bail!("the region is outside the canvas");
    }
    let row = frame.width as usize * 4;
    let rgba = (y as usize..(y + h) as usize)
        .flat_map(|r| {
            frame.rgba[r * row + x as usize * 4..r * row + (x + w) as usize * 4]
                .iter()
                .copied()
        })
        .collect();
    Ok(Frame {
        width: w as u32,
        height: h as u32,
        rgba,
    })
}

pub fn render(
    path: &Path,
    out: Option<PathBuf>,
    frame: u32,
    (scale, region): (Option<f32>, Option<[i32; 4]>),
    (map, invert): (Option<&str>, bool),
    lit: bool,
) -> Result<()> {
    let start = Instant::now();
    let bundle = Bundle::open(path)?;
    let doc = &bundle.doc;
    let longest = region.map_or(doc.canvas.width.max(doc.canvas.height) as f32, |r| {
        r[2].max(r[3]) as f32
    });
    let scale = scale.or_else(|| agent::pixel_scale(&bundle, longest)).unwrap_or(1.0);
    let rasters = layers::load_at(
        &bundle,
        frame,
        &BrushSet::for_bundle(&bundle)?,
        klad_render::export::detail_of(doc, scale),
    )?;
    let (picture, tag) = match (map, lit) {
        (Some(_), true) => bail!("--map shows one map, --lit the picture lit by the drawing's lights: one at a time"),
        (Some(name), false) => {
            let map = Map::parse(name)?;
            let mut set = klad_render::maps::render(doc, frame, &rasters, scale)?;
            if invert {
                set.invert_depth();
            }
            (set.frame(map), format!("-{}", map.name()))
        }
        (None, true) => {
            let lights = klad_render::maps::settings(doc).lights;
            if lights.is_empty() {
                bail!("no lights to light it with: klad lights <bundle> --add point --at x,y");
            }
            (
                Lit::prepare(doc, frame, &rasters, scale)?.shade(&lights)?,
                "-lit".to_string(),
            )
        }
        (None, false) => {
            bail!(
                "render a map with --map normal|height|specular|emission|depth|occlusion, or the lit picture with --lit"
            )
        }
    };
    let mut picture = match region {
        Some(r) => crop(&picture, r, scale)?,
        None => picture,
    };
    let view = View {
        scale,
        region,
        references: false,
    };
    let path = out.unwrap_or_else(|| agent::render_path(&bundle, frame, view, &tag));
    write_atomic(&path, &picture.png()?)?;
    println!("{}", path.display());
    eprintln!(
        "{}x{}  hash {:016x}  {:.0} ms",
        picture.width,
        picture.height,
        picture.hash(),
        start.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}

pub fn hash(path: &Path, frames: &[u32], map: &str) -> Result<()> {
    let bundle = Bundle::open(path)?;
    let map = Map::parse(map)?;
    let brushes = BrushSet::for_bundle(&bundle)?;
    frames.iter().try_for_each(|frame| {
        let rasters = layers::load_at(&bundle, *frame, &brushes, 1.0)?;
        let picture = klad_render::maps::render(&bundle.doc, *frame, &rasters, 1.0)?.frame(map);
        println!("frame {frame:<4} {} {:016x}", map.name(), picture.hash());
        Ok(())
    })
}

pub fn parse_at(text: &str) -> Result<[f32; 2], String> {
    let parts: Vec<f32> = text
        .split(',')
        .map(|p| p.trim().parse::<f32>().map_err(|_| format!("{text:?} is not x,y")))
        .collect::<Result<_, _>>()?;
    match parts[..] {
        [x, y] => Ok([x, y]),
        _ => Err(format!("{text:?} is not x,y")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_props_go_to_the_surface_or_the_preview() {
        let maps = changed(
            Maps::default(),
            &["bevel=12".into(), "profile=chamfer".into(), "ambient=#101020".into()],
        )
        .unwrap();
        assert_eq!(maps.surface.bevel, 12.0);
        assert_eq!(maps.surface.profile, klad_core::maps::Profile::Chamfer);
        assert_eq!(maps.ambient, "#101020");
        assert!(
            changed(Maps::default(), &["glow=1".into()])
                .unwrap_err()
                .to_string()
                .contains("bevel")
        );
        assert!(changed(Maps::default(), &["bevel=-2".into()]).is_err());
    }

    #[test]
    fn a_region_crops_at_the_render_scale() {
        let frame = Frame {
            width: 4,
            height: 4,
            rgba: (0..64).collect(),
        };
        let part = crop(&frame, [1, 1, 1, 1], 2.0).unwrap();
        assert_eq!((part.width, part.height), (2, 2));
        assert_eq!(part.pixel(0, 0), frame.pixel(2, 2));
        assert!(crop(&frame, [3, 3, 2, 2], 1.0).is_err());
        assert_eq!(parse_at("3, 4.5").unwrap(), [3.0, 4.5]);
        assert!(parse_at("3").is_err());
    }

    #[test]
    fn bevel_fit_becomes_the_depth_of_the_shape() {
        use klad_core::doc::{Document, Layer, Node, Shape};
        let dir = std::env::temp_dir().join(format!("draw-fit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut doc = Document::new(128, 128);
        doc.layers = vec![Layer::vector(
            "l1",
            "disc",
            "c1",
            vec![
                Node::new(
                    "n1",
                    Shape::Ellipse {
                        cx: 64.0,
                        cy: 64.0,
                        rx: 40.0,
                        ry: 40.0,
                    },
                )
                .filled("#c0392b"),
            ],
        )];
        let bundle = Bundle::create(&dir, doc).unwrap();
        let props = fitted(&bundle, &["bevel=fit".into(), "profile=round".into()], None).unwrap();
        let bevel: f32 = props[0].strip_prefix("bevel=").unwrap().parse().unwrap();
        assert!((bevel - 40.0).abs() <= 1.0, "{props:?}");
        assert_eq!(props[1], "profile=round");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
