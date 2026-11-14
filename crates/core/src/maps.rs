use crate::color::parse_hex;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    #[default]
    Round,
    Chamfer,
    Pillow,
    Flat,
}

impl Profile {
    pub const ALL: [Profile; 4] = [Profile::Round, Profile::Chamfer, Profile::Pillow, Profile::Flat];

    pub fn name(self) -> &'static str {
        match self {
            Profile::Round => "round",
            Profile::Chamfer => "chamfer",
            Profile::Pillow => "pillow",
            Profile::Flat => "flat",
        }
    }

    pub fn rise(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Profile::Round => libm::sqrtf(1.0 - (1.0 - t) * (1.0 - t)),
            Profile::Chamfer => t,
            Profile::Pillow => t * t * (3.0 - 2.0 * t),
            Profile::Flat => 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Surface {
    pub bevel: f32,
    pub profile: Profile,
    pub strength: f32,
    pub body: f32,
    pub range: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directions: Option<u32>,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            bevel: 8.0,
            profile: Profile::Round,
            strength: 1.0,
            body: 1.0,
            range: 12.0,
            directions: None,
        }
    }
}

pub const DIRECTIONS: [u32; 3] = [0, 9, 17];

impl Surface {
    pub fn directions_for(&self, pixel: bool) -> u32 {
        self.directions.unwrap_or(if pixel { 9 } else { 0 })
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(d) = self.directions
            && !DIRECTIONS.contains(&d)
        {
            bail!("surface: directions {d} is not 0 (smooth), 9 or 17");
        }
        [
            ("bevel", self.bevel, 512.0),
            ("strength", self.strength, 10.0),
            ("body", self.body, 10.0),
            ("range", self.range, 512.0),
        ]
        .iter()
        .try_for_each(|(name, value, most)| {
            if !value.is_finite() || *value < 0.0 || value > most {
                bail!("surface: {name} {value} is outside 0..{most}");
            }
            Ok(())
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Height,
    Normal,
    Specular,
    Emission,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Height, Role::Normal, Role::Specular, Role::Emission];

    pub fn name(self) -> &'static str {
        match self {
            Role::Height => "height",
            Role::Normal => "normal",
            Role::Specular => "specular",
            Role::Emission => "emission",
        }
    }
}

const COMPASS: [(&str, [f32; 2]); 8] = [
    ("n", [0.0, -1.0]),
    ("ne", [1.0, -1.0]),
    ("e", [1.0, 0.0]),
    ("se", [1.0, 1.0]),
    ("s", [0.0, 1.0]),
    ("sw", [-1.0, 1.0]),
    ("w", [-1.0, 0.0]),
    ("nw", [-1.0, -1.0]),
];
pub const LEAN_DEGREES: f32 = 45.0;
pub const STEEP_DEGREES: f32 = 70.0;

pub fn unit(v: [f32; 3]) -> [f32; 3] {
    let n = libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    match n > 1e-9 {
        true => v.map(|c| c / n),
        false => [0.0, 0.0, 1.0],
    }
}

pub fn leaning(x: f32, y: f32) -> [f32; 3] {
    let along = libm::hypotf(x, y);
    let (x, y, along) = match along > 1.0 {
        true => (x / along, y / along, 1.0),
        false => (x, y, along),
    };
    unit([x, -y, libm::sqrtf((1.0 - along * along).max(0.0))])
}

fn tipped(toward: [f32; 2], degrees: f32) -> [f32; 3] {
    let len = libm::hypotf(toward[0], toward[1]).max(1e-6);
    let s = libm::sinf(degrees.to_radians());
    unit([
        toward[0] / len * s,
        -toward[1] / len * s,
        libm::cosf(degrees.to_radians()),
    ])
}

pub fn named(name: &str) -> Option<[f32; 3]> {
    let name = name.trim().to_ascii_lowercase();
    if name == "out" {
        return Some([0.0, 0.0, 1.0]);
    }
    let (compass, degrees) = match name.strip_suffix("-steep") {
        Some(c) => (c.to_string(), STEEP_DEGREES),
        None => (name.clone(), LEAN_DEGREES),
    };
    COMPASS
        .iter()
        .find(|(n, _)| *n == compass)
        .map(|(_, toward)| tipped(*toward, degrees))
}

pub fn palette(directions: u32) -> Vec<(String, [f32; 3])> {
    let ring = |suffix: &str| -> Vec<(String, [f32; 3])> {
        COMPASS
            .iter()
            .map(|(n, _)| {
                let name = format!("{n}{suffix}");
                let normal = named(&name).unwrap_or([0.0, 0.0, 1.0]);
                (name, normal)
            })
            .collect()
    };
    let mut out = vec![("out".to_string(), [0.0, 0.0, 1.0])];
    match directions {
        9 => out.extend(ring("")),
        17 => {
            out.extend(ring(""));
            out.extend(ring("-steep"));
        }
        _ => {}
    }
    out
}

pub fn nearest(normal: [f32; 3], choices: &[(String, [f32; 3])]) -> [f32; 3] {
    choices
        .iter()
        .map(|(_, n)| *n)
        .max_by(|a, b| {
            let da = a[0] * normal[0] + a[1] * normal[1] + a[2] * normal[2];
            let db = b[0] * normal[0] + b[1] * normal[1] + b[2] * normal[2];
            da.total_cmp(&db)
        })
        .unwrap_or(normal)
}

pub fn encoded(normal: [f32; 3]) -> [f32; 3] {
    normal.map(|c| (c * 0.5 + 0.5).clamp(0.0, 1.0))
}

pub fn decoded(rgb: [f32; 3]) -> [f32; 3] {
    unit(rgb.map(|c| c * 2.0 - 1.0))
}

pub fn hex_of(normal: [f32; 3]) -> String {
    let [r, g, b] = encoded(normal).map(|c| (c * 255.0).round() as u8);
    format!("#{r:02x}{g:02x}{b:02x}")
}

pub fn direction(text: &str) -> Result<[f32; 3]> {
    if let Some(n) = named(text) {
        return Ok(n);
    }
    let parts: Vec<f32> = text
        .split(',')
        .map(|p| p.trim().parse::<f32>())
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| anyhow::anyhow!("{text:?} is not a direction: give x,y (how far it leans right and down, -1 to 1) or a name: out, n, ne, e, se, s, sw, w, nw, or n-steep and the like"))?;
    match parts[..] {
        [x, y] if x.is_finite() && y.is_finite() => Ok(leaning(x, y)),
        _ => bail!("{text:?} is not a direction: give x,y, for example 0.5,-0.5 leans right and up"),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LightKind {
    #[default]
    Point,
    Directional,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Light {
    pub kind: LightKind,
    pub at: [f32; 2],
    pub height: f32,
    pub color: String,
    pub energy: f32,
    pub range: f32,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            kind: LightKind::Point,
            at: [0.0, 0.0],
            height: 100.0,
            color: "#ffffff".into(),
            energy: 1.0,
            range: 600.0,
        }
    }
}

impl Light {
    pub fn directional(toward: [f32; 2], height: f32) -> Self {
        Self {
            kind: LightKind::Directional,
            at: toward,
            height,
            ..Self::default()
        }
    }

    pub fn validate(&self) -> Result<()> {
        parse_hex(&self.color)?;
        if !self.at.iter().all(|v| v.is_finite()) {
            bail!("light: at must be two numbers");
        }
        if !self.energy.is_finite() || self.energy < 0.0 {
            bail!("light: energy {} must be 0 or more", self.energy);
        }
        match self.kind {
            LightKind::Point => {
                if !self.height.is_finite() || self.height < 0.0 {
                    bail!("light: height {} must be 0 px or more", self.height);
                }
                if !self.range.is_finite() || self.range <= 0.0 {
                    bail!("light: range {} must be more than 0 px", self.range);
                }
            }
            LightKind::Directional => {
                if !(0.0..=1.0).contains(&self.height) {
                    bail!(
                        "light: a directional light's height {} is from 0 (along the picture) to 1 (from straight in front)",
                        self.height
                    );
                }
                if self.at == [0.0, 0.0] {
                    bail!("light: a directional light needs the direction it comes from, at x,y");
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Maps {
    pub surface: Surface,
    pub ambient: String,
    pub shininess: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lights: Vec<Light>,
}

impl Default for Maps {
    fn default() -> Self {
        Self {
            surface: Surface::default(),
            ambient: "#3a3a46".into(),
            shininess: 24.0,
            lights: Vec::new(),
        }
    }
}

pub fn godot_shininess(shininess: f32) -> f32 {
    ((libm::log2f((shininess * 4.0).max(2.0)) - 1.0) / 15.0).clamp(0.0, 1.0)
}

impl Maps {
    pub fn validate(&self) -> Result<()> {
        self.surface.validate()?;
        parse_hex(&self.ambient).map_err(|e| anyhow::anyhow!("maps: ambient: {e}"))?;
        if !self.shininess.is_finite() || !(1.0..=512.0).contains(&self.shininess) {
            bail!("maps: shininess {} is outside 1..512", self.shininess);
        }
        self.lights
            .iter()
            .enumerate()
            .try_for_each(|(i, l)| l.validate().map_err(|e| anyhow::anyhow!("light {}: {e}", i + 1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_rise_from_the_edge_to_the_top() {
        Profile::ALL.iter().filter(|p| **p != Profile::Flat).for_each(|p| {
            assert_eq!(p.rise(0.0), 0.0, "{}", p.name());
            assert!((p.rise(1.0) - 1.0).abs() < 1e-6, "{}", p.name());
            assert!(p.rise(0.3) < p.rise(0.6), "{} rises", p.name());
        });
        assert!(
            Profile::Round.rise(0.2) > Profile::Chamfer.rise(0.2),
            "a round rim is steep at the edge"
        );
        assert!(
            Profile::Pillow.rise(0.2) < Profile::Chamfer.rise(0.2),
            "a pillow starts soft"
        );
        assert_eq!(Profile::Flat.rise(0.7), 0.0);
    }

    #[test]
    fn maps_read_from_json_with_defaults_and_refuse_nonsense() {
        let maps: Maps = serde_json::from_str(r##"{"surface": {"bevel": 4}, "lights": [{"at": [10, 20]}]}"##).unwrap();
        assert_eq!(maps.surface.bevel, 4.0);
        assert_eq!(maps.surface.profile, Profile::Round);
        assert_eq!(maps.lights[0].kind, LightKind::Point);
        maps.validate().unwrap();
        assert!(
            serde_json::from_str::<Maps>(r#"{"glow": 1}"#).is_err(),
            "unknown keys are refused"
        );
        let bad = Maps {
            lights: vec![Light::directional([0.0, 0.0], 0.5)],
            ..Maps::default()
        };
        assert!(bad.validate().unwrap_err().to_string().contains("direction"));
        let steep = Maps {
            surface: Surface {
                bevel: -1.0,
                ..Surface::default()
            },
            ..Maps::default()
        };
        assert!(steep.validate().unwrap_err().to_string().contains("bevel"));
        assert_eq!(serde_json::to_value(Role::Height).unwrap(), "height");
    }
}
