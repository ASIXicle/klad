use crate::maps::Map;
use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effect {
    Fog,
    Rim,
    Occlusion,
}

impl Effect {
    pub const ALL: [Effect; 3] = [Effect::Fog, Effect::Rim, Effect::Occlusion];

    pub fn name(self) -> &'static str {
        match self {
            Effect::Fog => "fog",
            Effect::Rim => "rim",
            Effect::Occlusion => "occlusion",
        }
    }

    pub fn map(self) -> Map {
        match self {
            Effect::Fog => Map::Depth,
            Effect::Rim => Map::Height,
            Effect::Occlusion => Map::Occlusion,
        }
    }

    pub fn uniform(self) -> &'static str {
        match self {
            Effect::Fog => "depth_map",
            Effect::Rim => "height_map",
            Effect::Occlusion => "occlusion_map",
        }
    }

    pub fn list(text: &str) -> Result<Vec<Effect>> {
        let mut out: Vec<Effect> = match text.trim() {
            "all" => Effect::ALL.to_vec(),
            list => list
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(|t| match Effect::ALL.iter().find(|e| e.name() == t) {
                    Some(e) => Ok(*e),
                    None => bail!("no {t:?} shader: the shaders are fog, rim and occlusion (or all)"),
                })
                .collect::<Result<_>>()?,
        };
        out.sort();
        out.dedup();
        Ok(out)
    }
}

pub fn source(effects: &[Effect], nearest: bool) -> String {
    let filter = match nearest {
        true => "filter_nearest",
        false => "filter_linear",
    };
    let has = |e: Effect| effects.contains(&e);
    let mut uniforms = String::new();
    let mut body = String::new();
    if has(Effect::Occlusion) {
        uniforms += &format!(
            "\n// occlusion: darkens creases and the foot of raised forms\nuniform sampler2D occlusion_map : hint_default_white, {filter};\nuniform float occlusion_amount : hint_range(0.0, 1.0) = 0.8;\n"
        );
        body += "\tCOLOR.rgb *= mix(1.0, texture(occlusion_map, UV).r, occlusion_amount);\n";
    }
    if has(Effect::Rim) {
        uniforms += &format!(
            "\n// rim: a thin light on the edges of the form that face rim_from, read from the height map\nuniform sampler2D height_map : hint_default_black, {filter};\nuniform vec4 rim_color : source_color = vec4(1.0, 0.92, 0.75, 1.0);\nuniform vec2 rim_from = vec2(-1.0, -1.0);\nuniform float rim_strength : hint_range(0.0, 4.0) = 1.0;\nuniform float rim_sharpness : hint_range(1.0, 64.0) = 12.0;\n"
        );
        body += "\tvec2 step_px = TEXTURE_PIXEL_SIZE;\n\tvec2 slope = vec2(\n\t\ttexture(height_map, UV + vec2(step_px.x, 0.0)).r - texture(height_map, UV - vec2(step_px.x, 0.0)).r,\n\t\ttexture(height_map, UV + vec2(0.0, step_px.y)).r - texture(height_map, UV - vec2(0.0, step_px.y)).r);\n\tfloat steep = clamp(length(slope) * rim_sharpness, 0.0, 1.0);\n\tfloat facing = length(slope) > 0.0 ? max(dot(normalize(-slope), normalize(rim_from)), 0.0) : 0.0;\n\tCOLOR.rgb += rim_color.rgb * steep * facing * rim_strength * COLOR.a;\n";
    }
    if has(Effect::Fog) {
        uniforms += &format!(
            "\n// fog: towards fog_color the farther away a pixel is in the depth map (near is white; no depth, no fog)\nuniform sampler2D depth_map : hint_default_white, {filter};\nuniform vec4 fog_color : source_color = vec4(0.62, 0.68, 0.76, 1.0);\nuniform float fog_near : hint_range(0.0, 1.0) = 0.75;\nuniform float fog_far : hint_range(0.0, 1.0) = 0.1;\nuniform float fog_amount : hint_range(0.0, 1.0) = 0.85;\n"
        );
        body += "\tvec4 depth = texture(depth_map, UV);\n\tfloat fogged = clamp((fog_near - depth.r) / max(fog_near - fog_far, 0.0001), 0.0, 1.0) * fog_amount * depth.a;\n\tCOLOR.rgb = mix(COLOR.rgb, fog_color.rgb, fogged);\n";
    }
    let names: Vec<&str> = effects.iter().map(|e| e.name()).collect();
    format!(
        "// Made by klad from the drawing's light maps: {}. A sample to start from: change it freely.\nshader_type canvas_item;\n{uniforms}\nvoid fragment() {{\n{body}}}\n",
        names.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shader_holds_only_what_was_asked_for() {
        let fog = source(&[Effect::Fog], false);
        assert!(fog.contains("uniform sampler2D depth_map") && !fog.contains("height_map"));
        assert!(fog.starts_with("// Made by klad"));
        let all = source(&Effect::list("all").unwrap(), true);
        assert!(all.contains("filter_nearest") && all.contains("occlusion_map") && all.contains("rim_from"));
        assert_eq!(Effect::list("rim, fog").unwrap(), [Effect::Fog, Effect::Rim]);
        assert!(Effect::list("bloom").is_err());
        assert_eq!(Effect::Fog.map(), Map::Depth);
    }
}
