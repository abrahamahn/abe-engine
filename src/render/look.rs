//! A game's art direction as data: shading style, post-processing, sky palette, place and time,
//! named camera views. Engine defaults are a neutral soft-cel look; games override any field from
//! a JSON Look file so one renderer serves many styles.

use super::math::{V3, hex};
use super::sky::{Palette, SkyKey, TimeOfDay, Weather};
use abe_assets::json::Value;

/// Shading and post-processing parameters. Colours are linear RGB.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    /// Half-Lambert position of the light/shadow boundary (0..1) and the width of the soft band.
    pub ramp_center: f32,
    pub ramp_softness: f32,
    /// Colour multiplying ambient light on the shadowed side, and how much darker shade gets.
    pub shadow_tint: V3,
    pub shadow_strength: f32,
    pub rim_strength: f32,
    pub rim_power: f32,
    pub specular: f32,
    pub specular_size: f32,
    /// Foliage: how far light wraps around the canopy, and back-lit glow.
    pub foliage_wrap: f32,
    pub foliage_translucency: f32,
    /// Positive values soften textures (sampling coarser mips) for a painted look.
    pub texture_softness: f32,
    pub normal_map_strength: f32,
    pub ambient_occlusion: f32,
    pub ao_radius: f32,
    pub ao_tint: V3,
    pub outline: f32,
    pub outline_color: V3,
    /// Painterly (Kuwahara) filter radius in pixels at 1080p; 0 disables it.
    pub painterly: f32,
    pub painterly_far: f32,
    pub bloom: f32,
    pub bloom_threshold: f32,
    pub exposure: f32,
    pub saturation: f32,
    pub contrast: f32,
    /// Grading tints added to shadows and multiplied into highlights.
    pub lift: V3,
    pub gain: V3,
    pub vignette: f32,
    /// Scale of point-light (lamp) intensity and of lamp-emission glow.
    pub lamp_light: f32,
    pub lamp_glow: f32,
    pub fog: f32,
    pub fog_height: f32,
    pub fog_falloff: f32,
    pub water_color: V3,
    pub water_reflection: f32,
    pub water_ripples: f32,
    pub cloud_scale: f32,
    pub star_density: f32,
    pub moon_size: f32,
    pub palette: Palette,
    /// Painted brushwork: how much world-space brush strokes break up the light/shade edge and
    /// flat colour (0 = none), and the stroke size in metres.
    pub brush: f32,
    pub brush_scale: f32,
    /// Final sharpening (0 = none).
    pub sharpen: f32,
    /// Light bounced up from sunlit (or moonlit) ground onto eaves, ceilings and the shaded
    /// undersides of things: the fraction of the key light added to the ground ambient.
    pub bounce: f32,
    /// Light shafts from the sun or moon through gaps in the scenery (0 = none).
    pub sun_shafts: f32,
    /// Shallow-water colour, shoreline foam strength and sun/moon glitter on water.
    pub water_shallow: V3,
    pub foam: f32,
    pub sparkle: f32,
    /// Procedural surface detail by material name; the first matching rule wins.
    pub details: Vec<DetailRule>,
    pub grass: Grass,
}

/// A procedural surface pattern a Look file can give materials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailPreset {
    None = 0,
    /// Rows of rounded roof tiles aligned to the slope, with lapped edges.
    RoofTiles = 1,
    /// Planks and grain.
    Wood = 2,
    /// Soft mottling and streaks.
    Plaster = 3,
    /// Irregular stones with dark joints.
    Stone = 4,
    /// Polished: glossier, with a sheen.
    Lacquer = 5,
    /// Grassy ground: grass grows on it (see [`Grass`]) and it takes the grass colour.
    Grass = 6,
}

impl DetailPreset {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Self::None,
            "roof_tiles" => Self::RoofTiles,
            "wood" => Self::Wood,
            "plaster" => Self::Plaster,
            "stone" => Self::Stone,
            "lacquer" => Self::Lacquer,
            "grass" => Self::Grass,
            _ => return None,
        })
    }
}

/// Which materials get which [`DetailPreset`], and how.
#[derive(Clone, Debug, PartialEq)]
pub struct DetailRule {
    /// Material-name patterns; `*` matches any run of characters.
    pub patterns: Vec<String>,
    pub preset: DetailPreset,
    /// Pattern size in metres: tile width and row length, plank width and length, stone size.
    pub scale: [f32; 2],
    /// How strongly the pattern shows (relief, contrast).
    pub strength: f32,
    /// Colour variation between tiles, planks or stones.
    pub variation: f32,
    /// Replaces the material's colour (linear RGB) when set.
    pub color: Option<V3>,
    /// Extra gloss (0..1), for lacquer and polished wood.
    pub gloss: f32,
}

impl DetailRule {
    /// True when one of the patterns matches `name`.
    pub fn matches(&self, name: &str) -> bool {
        self.patterns.iter().any(|p| glob(p, name))
    }
}

/// `*` matches any run of characters; everything else matches itself.
pub fn glob(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || name.len() < first.len() + last.len() || !name.ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }
    true
}

/// Grass and flowers grown on materials with the [`DetailPreset::Grass`] detail.
#[derive(Clone, Debug, PartialEq)]
pub struct Grass {
    /// Blade density (1 = the quality's default count, 0 = no grass).
    pub density: f32,
    /// Blade height and width in metres.
    pub height: f32,
    pub width: f32,
    /// Colours at the root and the tip (linear RGB).
    pub base: V3,
    pub tip: V3,
    /// Fraction of blades that carry a flower, and the flower colours.
    pub flowers: f32,
    pub flower_colors: [V3; 4],
    /// How far blades bend in the wind.
    pub sway: f32,
}

impl Default for Grass {
    fn default() -> Self {
        Self {
            density: 0.0,
            height: 0.38,
            width: 0.045,
            base: [0.06, 0.14, 0.03],
            tip: [0.34, 0.52, 0.12],
            flowers: 0.03,
            flower_colors: [
                [0.95, 0.95, 0.9],
                [1.0, 0.82, 0.25],
                [0.95, 0.5, 0.65],
                [0.65, 0.55, 0.95],
            ],
            sway: 1.0,
        }
    }
}

impl Default for Look {
    fn default() -> Self {
        Self {
            ramp_center: 0.5,
            ramp_softness: 0.08,
            shadow_tint: [0.62, 0.68, 0.95],
            shadow_strength: 0.55,
            rim_strength: 0.35,
            rim_power: 3.5,
            specular: 0.35,
            specular_size: 0.25,
            foliage_wrap: 0.6,
            foliage_translucency: 0.5,
            texture_softness: 0.8,
            normal_map_strength: 0.5,
            ambient_occlusion: 0.65,
            ao_radius: 0.6,
            ao_tint: [0.28, 0.3, 0.5],
            outline: 0.25,
            outline_color: [0.05, 0.05, 0.12],
            painterly: 2.5,
            painterly_far: 2.0,
            bloom: 0.6,
            bloom_threshold: 1.0,
            exposure: 1.0,
            saturation: 1.1,
            contrast: 1.05,
            lift: [0.0, 0.004, 0.012],
            gain: [1.0, 1.0, 1.0],
            vignette: 0.25,
            lamp_light: 1.0,
            lamp_glow: 1.0,
            fog: 1.0,
            fog_height: 0.0,
            fog_falloff: 0.08,
            water_color: [0.01, 0.06, 0.1],
            water_reflection: 0.85,
            water_ripples: 1.0,
            cloud_scale: 1.0,
            star_density: 1.0,
            moon_size: 1.0,
            palette: Palette::default(),
            brush: 0.3,
            brush_scale: 0.35,
            sharpen: 0.2,
            bounce: 0.1,
            sun_shafts: 0.5,
            water_shallow: [0.05, 0.2, 0.2],
            foam: 0.6,
            sparkle: 0.5,
            details: vec![],
            grass: Grass::default(),
        }
    }
}

/// A named camera. `fov` is the vertical field of view in degrees.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub name: String,
    pub eye: V3,
    pub target: V3,
    pub fov: f32,
}

/// A parsed Look file: the look plus where/when the scene is, its default weather and views.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LookFile {
    pub look: Look,
    pub time: TimeOfDay,
    pub weather: Weather,
    pub views: Vec<View>,
}
fn num(v: &Value, key: &str, out: &mut f32) -> Result<(), String> {
    if let Some(x) = v.get(key) {
        *out = x.float().map_err(|e| format!("{key}: {e}"))?;
    }
    Ok(())
}
fn vec3(v: &Value) -> Result<V3, String> {
    match v {
        Value::String(s) => hex(s).ok_or_else(|| format!("bad colour {s:?} (want \"#rrggbb\")")),
        Value::Array(a) if a.len() == 3 => Ok([a[0].float()?, a[1].float()?, a[2].float()?]),
        _ => Err("expected [x, y, z] or \"#rrggbb\"".into()),
    }
}
fn col(v: &Value, key: &str, out: &mut V3) -> Result<(), String> {
    if let Some(x) = v.get(key) {
        *out = vec3(x).map_err(|e| format!("{key}: {e}"))?;
    }
    Ok(())
}

impl LookFile {
    /// Parse a Look file. Every field is optional; unknown keys are ignored so files can carry
    /// notes. Colours are `"#rrggbb"` (sRGB) or `[r, g, b]` (linear).
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let root = abe_assets::json::parse(bytes)?;
        let mut file = Self::default();
        if let Some(s) = root.get("shading") {
            let l = &mut file.look;
            num(s, "ramp_center", &mut l.ramp_center)?;
            num(s, "ramp_softness", &mut l.ramp_softness)?;
            col(s, "shadow_tint", &mut l.shadow_tint)?;
            num(s, "shadow_strength", &mut l.shadow_strength)?;
            num(s, "rim_strength", &mut l.rim_strength)?;
            num(s, "rim_power", &mut l.rim_power)?;
            num(s, "specular", &mut l.specular)?;
            num(s, "specular_size", &mut l.specular_size)?;
            num(s, "foliage_wrap", &mut l.foliage_wrap)?;
            num(s, "foliage_translucency", &mut l.foliage_translucency)?;
            num(s, "texture_softness", &mut l.texture_softness)?;
            num(s, "normal_map_strength", &mut l.normal_map_strength)?;
            num(s, "lamp_light", &mut l.lamp_light)?;
            num(s, "lamp_glow", &mut l.lamp_glow)?;
            col(s, "water_color", &mut l.water_color)?;
            num(s, "water_reflection", &mut l.water_reflection)?;
            num(s, "water_ripples", &mut l.water_ripples)?;
            col(s, "water_shallow", &mut l.water_shallow)?;
            num(s, "foam", &mut l.foam)?;
            num(s, "sparkle", &mut l.sparkle)?;
            num(s, "brush", &mut l.brush)?;
            num(s, "bounce", &mut l.bounce)?;
            num(s, "brush_scale", &mut l.brush_scale)?;
        }
        if let Some(p) = root.get("post") {
            let l = &mut file.look;
            num(p, "ambient_occlusion", &mut l.ambient_occlusion)?;
            num(p, "ao_radius", &mut l.ao_radius)?;
            col(p, "ao_tint", &mut l.ao_tint)?;
            num(p, "outline", &mut l.outline)?;
            col(p, "outline_color", &mut l.outline_color)?;
            num(p, "painterly", &mut l.painterly)?;
            num(p, "painterly_far", &mut l.painterly_far)?;
            num(p, "bloom", &mut l.bloom)?;
            num(p, "bloom_threshold", &mut l.bloom_threshold)?;
            num(p, "exposure", &mut l.exposure)?;
            num(p, "saturation", &mut l.saturation)?;
            num(p, "contrast", &mut l.contrast)?;
            col(p, "lift", &mut l.lift)?;
            col(p, "gain", &mut l.gain)?;
            num(p, "vignette", &mut l.vignette)?;
            num(p, "fog", &mut l.fog)?;
            num(p, "fog_height", &mut l.fog_height)?;
            num(p, "fog_falloff", &mut l.fog_falloff)?;
            num(p, "sharpen", &mut l.sharpen)?;
            num(p, "sun_shafts", &mut l.sun_shafts)?;
        }
        if let Some(rules) = root.get("details") {
            for r in rules.array()? {
                file.look.details.push(parse_detail(r)?);
            }
        }
        if let Some(g) = root.get("grass") {
            let grass = &mut file.look.grass;
            num(g, "density", &mut grass.density)?;
            num(g, "height", &mut grass.height)?;
            num(g, "width", &mut grass.width)?;
            col(g, "base", &mut grass.base)?;
            col(g, "tip", &mut grass.tip)?;
            num(g, "flowers", &mut grass.flowers)?;
            num(g, "sway", &mut grass.sway)?;
            if let Some(c) = g.get("flower_colors") {
                let c = c.array()?;
                if c.is_empty() {
                    return Err("grass.flower_colors is empty".into());
                }
                for (i, slot) in grass.flower_colors.iter_mut().enumerate() {
                    *slot = vec3(&c[i % c.len()]).map_err(|e| format!("flower_colors: {e}"))?;
                }
            }
        }
        if let Some(s) = root.get("sky") {
            let l = &mut file.look;
            num(s, "cloud_scale", &mut l.cloud_scale)?;
            num(s, "star_density", &mut l.star_density)?;
            num(s, "moon_size", &mut l.moon_size)?;
            col(s, "moon_color", &mut l.palette.moon)?;
            num(s, "moon_intensity", &mut l.palette.moon_intensity)?;
            num(s, "dusk_warmth", &mut l.palette.dusk_warmth)?;
            num(s, "dawn_mist", &mut l.palette.dawn_mist)?;
            if let Some(keys) = s.get("keys") {
                file.look.palette.keys = parse_keys(keys)?;
            }
        }
        if let Some(t) = root.get("time") {
            let time = &mut file.time;
            num(t, "hours", &mut time.hours)?;
            num(t, "day_of_year", &mut time.day_of_year)?;
            num(t, "latitude", &mut time.latitude_deg)?;
            num(t, "moon_lag_hours", &mut time.moon_lag_hours)?;
            if let Some(d) = t.get("moon_declination") {
                time.moon_declination = Some(d.float()?);
            }
            if let Some(n) = t.get("north") {
                time.north = vec3(n).map_err(|e| format!("time.north: {e}"))?;
            }
        }
        if let Some(w) = root.get("weather") {
            if let Some(name) = w.get("preset") {
                let name = name.string()?;
                file.weather =
                    Weather::preset(name).ok_or_else(|| format!("unknown weather {name:?}"))?;
            }
            let wt = &mut file.weather;
            num(w, "clouds", &mut wt.clouds)?;
            num(w, "rain", &mut wt.rain)?;
            num(w, "mist", &mut wt.mist)?;
            num(w, "wind", &mut wt.wind)?;
            num(w, "petals", &mut wt.petals)?;
        }
        if let Some(views) = root.get("views") {
            for v in views.array()? {
                let name = v
                    .get("name")
                    .ok_or("view without name")?
                    .string()?
                    .to_owned();
                let eye = vec3(v.get("eye").ok_or("view without eye")?)?;
                let target = vec3(v.get("target").ok_or("view without target")?)?;
                let mut fov = 50.0;
                num(v, "fov", &mut fov)?;
                file.views.push(View {
                    name,
                    eye,
                    target,
                    fov,
                });
            }
        }
        Ok(file)
    }
}

fn parse_detail(r: &Value) -> Result<DetailRule, String> {
    let patterns = match r.get("match").ok_or("detail rule without \"match\"")? {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a
            .iter()
            .map(|v| v.string().map(str::to_owned))
            .collect::<Result<_, _>>()?,
        _ => return Err("detail \"match\" must be a string or an array of strings".into()),
    };
    let name = r
        .get("preset")
        .ok_or("detail rule without \"preset\"")?
        .string()?;
    let preset =
        DetailPreset::parse(name).ok_or_else(|| format!("unknown detail preset {name:?}"))?;
    let mut rule = DetailRule {
        patterns,
        preset,
        scale: [0.3, 0.3],
        strength: 1.0,
        variation: 0.1,
        color: None,
        gloss: 0.0,
    };
    match r.get("scale") {
        Some(Value::Array(a)) if a.len() == 2 => rule.scale = [a[0].float()?, a[1].float()?],
        Some(v) => {
            let x = v.float().map_err(|e| format!("detail scale: {e}"))?;
            rule.scale = [x, x];
        }
        None => {}
    }
    num(r, "strength", &mut rule.strength)?;
    num(r, "variation", &mut rule.variation)?;
    num(r, "gloss", &mut rule.gloss)?;
    if let Some(c) = r.get("color") {
        rule.color = Some(vec3(c).map_err(|e| format!("detail color: {e}"))?);
    }
    Ok(rule)
}

fn parse_keys(v: &Value) -> Result<Vec<SkyKey>, String> {
    let mut keys = vec![];
    let base = Palette::default();
    for k in v.array()? {
        let mut altitude = 0.0;
        num(k, "altitude", &mut altitude)?;
        let mut key = base.keys[0];
        // Start from the default key nearest in altitude so partial keys stay sensible.
        if let Some(near) = base.keys.iter().min_by(|a, b| {
            (a.altitude - altitude)
                .abs()
                .total_cmp(&(b.altitude - altitude).abs())
        }) {
            key = *near;
        }
        key.altitude = altitude;
        col(k, "zenith", &mut key.zenith)?;
        col(k, "horizon", &mut key.horizon)?;
        col(k, "glow", &mut key.glow)?;
        col(k, "ambient_sky", &mut key.ambient_sky)?;
        col(k, "ambient_ground", &mut key.ambient_ground)?;
        col(k, "fog", &mut key.fog)?;
        col(k, "cloud_lit", &mut key.cloud_lit)?;
        col(k, "cloud_shadow", &mut key.cloud_shadow)?;
        col(k, "sun", &mut key.sun)?;
        num(k, "sun_intensity", &mut key.sun_intensity)?;
        num(k, "ambient", &mut key.ambient)?;
        num(k, "stars", &mut key.stars)?;
        num(k, "exposure", &mut key.exposure)?;
        keys.push(key);
    }
    if keys.is_empty() {
        return Err("sky.keys is empty".into());
    }
    keys.sort_by(|a, b| a.altitude.total_cmp(&b.altitude));
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_overrides_and_views() {
        let f = LookFile::parse(
            br##"{"shading": {"rim_strength": 0.7, "shadow_tint": "#808080"},
                 "time": {"hours": 6.5, "north": [0, 0, 1]},
                 "weather": {"preset": "rain", "wind": 2},
                 "views": [{"name": "a", "eye": [0, 1, 2], "target": [0, 0, 0], "fov": 40}]}"##,
        )
        .unwrap();
        assert_eq!(f.look.rim_strength, 0.7);
        assert!((f.look.shadow_tint[0] - 0.2158).abs() < 1e-3);
        assert_eq!(f.time.hours, 6.5);
        assert_eq!(f.weather.rain, Weather::preset("rain").unwrap().rain);
        assert_eq!(f.weather.wind, 2.0);
        assert_eq!(f.views[0].fov, 40.0);
    }

    #[test]
    fn detail_rules_and_grass() {
        let f = LookFile::parse(
            br##"{"details": [
                    {"match": ["roof-*", "*-tile"], "preset": "roof_tiles", "scale": [0.25, 0.3]},
                    {"match": "lawn", "preset": "grass", "color": "#4a7a30"}],
                  "grass": {"density": 0.8, "flower_colors": ["#ffffff"]}}"##,
        )
        .unwrap();
        let d = &f.look.details;
        assert_eq!(d[0].preset, DetailPreset::RoofTiles);
        assert!(d[0].matches("roof-main") && d[0].matches("blue-tile") && !d[0].matches("tiles"));
        assert_eq!(d[0].scale, [0.25, 0.3]);
        assert!(d[1].matches("lawn") && !d[1].matches("lawns") && d[1].color.is_some());
        assert_eq!(f.look.grass.density, 0.8);
        assert_eq!(f.look.grass.flower_colors[3], [1.0, 1.0, 1.0]);
        assert!(LookFile::parse(br#"{"details": [{"match": "a", "preset": "fur"}]}"#).is_err());
    }

    #[test]
    fn glob_patterns() {
        assert!(glob("a*", "abc") && glob("*c", "abc") && glob("a*c", "abc") && glob("*", ""));
        assert!(glob("a*b*c", "a-b-c") && !glob("a*b*c", "a-c") && !glob("ab*ba", "aba"));
        assert!(glob("exact", "exact") && !glob("exact", "exactly"));
    }
}
