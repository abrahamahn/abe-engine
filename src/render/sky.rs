//! Time of day and weather, resolved into the lighting environment one frame uses.
//!
//! The sun and moon follow the clock, the latitude and the day of the year. Sky, light, fog and
//! exposure come from art-directed keyframes over the sun's altitude (a game's Look file may
//! replace them), with sunrise kept cooler and misty and sunset warmer. Weather layers on top:
//! cloud cover dims and softens the key light, rain wets surfaces and thickens the air, mist
//! fills the low ground, wind moves clouds, rain and petals. Lamps (emission marked `Lamp`)
//! come on as the sun sets.

use super::math::{V3, cross, hex, lerp, lerp3, normalize, scale, smoothstep};

/// Where and when: `hours` is local solar time (0..24), `north` the world direction of north.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeOfDay {
    pub hours: f32,
    pub day_of_year: f32,
    pub latitude_deg: f32,
    pub north: V3,
    /// Hours the moon trails the point opposite the sun (0 = full moon, highest at midnight).
    pub moon_lag_hours: f32,
    /// Art-directed moon declination in degrees (`None`: opposite the sun's, as a full moon).
    pub moon_declination: Option<f32>,
}
impl Default for TimeOfDay {
    fn default() -> Self {
        Self {
            hours: 21.0,
            day_of_year: 95.0,
            latitude_deg: 35.0,
            north: [0.0, 0.0, -1.0],
            moon_lag_hours: 0.0,
            moon_declination: None,
        }
    }
}

/// Weather amounts in 0..1, except `wind` which is a speed in metres per second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weather {
    pub clouds: f32,
    pub rain: f32,
    pub mist: f32,
    pub wind: f32,
    pub petals: f32,
}
impl Weather {
    pub const PRESETS: [&'static str; 7] = [
        "clear", "cloudy", "overcast", "mist", "rain", "storm", "petals",
    ];
    pub fn preset(name: &str) -> Option<Self> {
        let w = |clouds, rain, mist, wind, petals| Self {
            clouds,
            rain,
            mist,
            wind,
            petals,
        };
        Some(match name {
            "clear" => w(0.18, 0.0, 0.0, 1.5, 0.15),
            "cloudy" => w(0.6, 0.0, 0.05, 3.0, 0.1),
            "overcast" => w(0.92, 0.0, 0.15, 3.0, 0.0),
            "mist" => w(0.3, 0.0, 0.85, 0.6, 0.05),
            "rain" => w(0.9, 0.75, 0.3, 4.0, 0.0),
            "storm" => w(1.0, 1.0, 0.45, 9.0, 0.0),
            "petals" => w(0.22, 0.0, 0.05, 3.5, 1.0),
            _ => return None,
        })
    }
}
impl Default for Weather {
    fn default() -> Self {
        Self::preset("clear").expect("preset")
    }
}

/// One art-directed sky state at a sun altitude. Colours are linear RGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyKey {
    pub altitude: f32,
    pub zenith: V3,
    pub horizon: V3,
    /// Horizon glow on the side of the sun (or moon at night).
    pub glow: V3,
    pub ambient_sky: V3,
    pub ambient_ground: V3,
    pub fog: V3,
    pub cloud_lit: V3,
    pub cloud_shadow: V3,
    /// Sun colour; the moon has its own colour in [`Palette`].
    pub sun: V3,
    pub sun_intensity: f32,
    pub ambient: f32,
    pub stars: f32,
    pub exposure: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    /// Sorted by ascending altitude.
    pub keys: Vec<SkyKey>,
    pub moon: V3,
    pub moon_intensity: f32,
    /// Warmer sunsets, cooler and mistier sunrises (0 disables the asymmetry).
    pub dusk_warmth: f32,
    pub dawn_mist: f32,
}

fn c(s: &str) -> V3 {
    hex(s).unwrap_or([1.0, 0.0, 1.0])
}

impl Default for Palette {
    /// A stylized natural sky: saturated blue days, gold and rose twilight, readable blue nights.
    fn default() -> Self {
        #[allow(clippy::too_many_arguments)]
        fn k(
            altitude: f32,
            [zenith, horizon, glow, sky, ground, fog, lit, shadow, sun]: [&str; 9],
            sun_intensity: f32,
            ambient: f32,
            stars: f32,
            exposure: f32,
        ) -> SkyKey {
            SkyKey {
                altitude,
                zenith: c(zenith),
                horizon: c(horizon),
                glow: c(glow),
                ambient_sky: c(sky),
                ambient_ground: c(ground),
                fog: c(fog),
                cloud_lit: c(lit),
                cloud_shadow: c(shadow),
                sun: c(sun),
                sun_intensity,
                ambient,
                stars,
                exposure,
            }
        }
        let night = [
            "#050d24", "#18305c", "#24406e", "#3d5a92", "#141a2c", "#1a2c54", "#4a6098", "#141c3a",
            "#ff9a50",
        ];
        Self {
            keys: vec![
                k(-90.0, night, 0.0, 1.0, 1.0, 1.0),
                k(-14.0, night, 0.0, 1.0, 1.0, 1.0),
                k(
                    -8.0,
                    [
                        "#0c1a44", "#3a4c88", "#7a5c96", "#4a5c98", "#1c1c30", "#2e3c6c",
                        "#7a72a8", "#1e2450", "#ff9a50",
                    ],
                    0.0,
                    1.0,
                    0.55,
                    1.0,
                ),
                k(
                    -3.0,
                    [
                        "#1a2c66", "#9a7aa8", "#f08a6a", "#5a6aa8", "#2e2634", "#7a6890",
                        "#f0a090", "#3e3468", "#ff8a50",
                    ],
                    0.0,
                    1.0,
                    0.12,
                    1.0,
                ),
                k(
                    0.5,
                    [
                        "#2c4c90", "#f4a878", "#ff8448", "#7080b8", "#4a3a34", "#d89478",
                        "#ffb488", "#6c4c70", "#ff9a52",
                    ],
                    1.2,
                    1.0,
                    0.0,
                    1.0,
                ),
                k(
                    6.0,
                    [
                        "#3866b4", "#f6c490", "#ffb070", "#7c9cd4", "#5a4c3c", "#e8bc9c",
                        "#ffe2c4", "#8a7c9e", "#ffc27e",
                    ],
                    1.9,
                    1.0,
                    0.0,
                    1.0,
                ),
                k(
                    16.0,
                    [
                        "#3a76cc", "#bcdcf0", "#ffe8c8", "#8ab6ea", "#6a6248", "#bcd6ec",
                        "#ffffff", "#a2b2ca", "#fff0d6",
                    ],
                    2.5,
                    1.0,
                    0.0,
                    1.0,
                ),
                k(
                    35.0,
                    [
                        "#2e6ad0", "#c4e4f6", "#fff6e8", "#92bef0", "#726c52", "#c2dcf2",
                        "#ffffff", "#aebccf", "#fff8ee",
                    ],
                    2.8,
                    1.0,
                    0.0,
                    1.0,
                ),
                k(
                    90.0,
                    [
                        "#2a62cc", "#c8e6f8", "#fff8f0", "#94c0f2", "#746e54", "#c6e0f4",
                        "#ffffff", "#b0bed2", "#fffaf2",
                    ],
                    2.9,
                    1.0,
                    0.0,
                    1.0,
                ),
            ],
            moon: c("#b4c8f4"),
            moon_intensity: 0.75,
            dusk_warmth: 1.0,
            dawn_mist: 1.0,
        }
    }
}

impl Palette {
    fn sample(&self, altitude: f32) -> SkyKey {
        let keys = &self.keys;
        if altitude <= keys[0].altitude {
            return keys[0];
        }
        for w in keys.windows(2) {
            let (a, b) = (w[0], w[1]);
            if altitude <= b.altitude {
                let t = smoothstep(
                    0.0,
                    1.0,
                    (altitude - a.altitude) / (b.altitude - a.altitude),
                );
                let m = |x: V3, y: V3| lerp3(x, y, t);
                return SkyKey {
                    altitude,
                    zenith: m(a.zenith, b.zenith),
                    horizon: m(a.horizon, b.horizon),
                    glow: m(a.glow, b.glow),
                    ambient_sky: m(a.ambient_sky, b.ambient_sky),
                    ambient_ground: m(a.ambient_ground, b.ambient_ground),
                    fog: m(a.fog, b.fog),
                    cloud_lit: m(a.cloud_lit, b.cloud_lit),
                    cloud_shadow: m(a.cloud_shadow, b.cloud_shadow),
                    sun: m(a.sun, b.sun),
                    sun_intensity: lerp(a.sun_intensity, b.sun_intensity, t),
                    ambient: lerp(a.ambient, b.ambient, t),
                    stars: lerp(a.stars, b.stars, t),
                    exposure: lerp(a.exposure, b.exposure, t),
                };
            }
        }
        *keys.last().expect("palette has keys")
    }
}

/// Everything the renderer needs about light and air for one frame. Directions point from the
/// scene towards the light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    pub sun_dir: V3,
    pub moon_dir: V3,
    pub sun_altitude: f32,
    pub key_dir: V3,
    pub key_color: V3,
    pub key_intensity: f32,
    pub zenith: V3,
    pub horizon: V3,
    pub glow: V3,
    pub glow_dir: V3,
    pub ambient_sky: V3,
    pub ambient_ground: V3,
    pub ambient: f32,
    pub fog_color: V3,
    pub fog_density: f32,
    pub stars: f32,
    pub sun_disc: f32,
    pub moon_disc: f32,
    pub clouds: f32,
    pub cloud_lit: V3,
    pub cloud_shadow: V3,
    pub lamp_level: f32,
    pub exposure: f32,
    /// Multiplies the shadow filter radius: overcast light has soft shadows.
    pub shadow_softness: f32,
    pub rain: f32,
    pub wetness: f32,
    pub mist: f32,
    pub wind: [f32; 2],
    pub petals: f32,
    pub seconds: f32,
}

/// Direction of a body at `hour_angle_deg` (0 = due south at its highest) and `declination_deg`,
/// seen from `latitude_deg`, as (world direction, altitude in degrees).
fn body(time: &TimeOfDay, hour_angle_deg: f32, declination_deg: f32) -> (V3, f32) {
    let (phi, dec, h) = (
        time.latitude_deg.to_radians(),
        declination_deg.to_radians(),
        hour_angle_deg.to_radians(),
    );
    let sin_alt = phi.sin() * dec.sin() + phi.cos() * dec.cos() * h.cos();
    let alt = sin_alt.clamp(-1.0, 1.0).asin();
    // Local east/north/up components.
    let east = -dec.cos() * h.sin();
    let north = dec.sin() * phi.cos() - dec.cos() * h.cos() * phi.sin();
    let up = sin_alt;
    let n = normalize([time.north[0], 0.0, time.north[2]]);
    let e = cross(n, [0.0, 1.0, 0.0]);
    let dir = normalize([e[0] * east + n[0] * north, up, e[2] * east + n[2] * north]);
    (dir, alt.to_degrees())
}

impl Environment {
    pub fn resolve(time: &TimeOfDay, weather: &Weather, palette: &Palette, seconds: f32) -> Self {
        let hours = time.hours.rem_euclid(24.0);
        let declination =
            23.44 * (std::f32::consts::TAU * (284.0 + time.day_of_year) / 365.0).sin();
        let (sun_dir, sun_alt) = body(time, 15.0 * (hours - 12.0), declination);
        let (moon_dir, moon_alt) = body(
            time,
            15.0 * (hours - 12.0 + 12.0 - time.moon_lag_hours),
            time.moon_declination.unwrap_or(-declination * 0.85),
        );
        let mut key = palette.sample(sun_alt);
        let afternoon = hours >= 12.0;
        // Twilight band where the sunrise/sunset asymmetry applies.
        let twilight = smoothstep(-12.0, -4.0, sun_alt) * (1.0 - smoothstep(6.0, 16.0, sun_alt));
        let mut mist = weather.mist;
        if afternoon {
            let w = twilight * palette.dusk_warmth * 0.35;
            let warm = |c: V3| {
                [
                    c[0] * (1.0 + w),
                    c[1] * (1.0 + w * 0.15),
                    c[2] * (1.0 - w * 0.45),
                ]
            };
            key.glow = warm(key.glow);
            key.horizon = warm(key.horizon);
            key.cloud_lit = warm(key.cloud_lit);
            key.sun = warm(key.sun);
        } else {
            let w = twilight * palette.dawn_mist;
            let cool = |c: V3| {
                let s = 0.18 * w;
                [
                    c[0] * (1.0 - s * 0.2),
                    c[1] * (1.0 - s * 0.1),
                    c[2] * (1.0 + s * 0.3),
                ]
            };
            key.glow = cool(key.glow);
            key.horizon = cool(key.horizon);
            mist = (mist + 0.45 * w).min(1.0);
        }
        let sun_vis = smoothstep(-2.5, 3.0, sun_alt);
        let night = 1.0 - smoothstep(-7.0, -1.0, sun_alt);
        let moon_vis = smoothstep(-3.0, 8.0, moon_alt) * night;
        let clouds = weather.clouds.clamp(0.0, 1.0);
        let rain = weather.rain.clamp(0.0, 1.0);
        let dim = (1.0 - 0.8 * clouds.powf(1.4)) * (1.0 - 0.45 * rain);
        let (key_dir, key_color, key_intensity) = if sun_vis > 0.001 || moon_vis < 0.001 {
            (sun_dir, key.sun, key.sun_intensity * sun_vis * dim)
        } else {
            (
                moon_dir,
                palette.moon,
                palette.moon_intensity * moon_vis * dim,
            )
        };
        // Cloud and rain turn the sky grey and lift relative ambient.
        let grey = |c: V3, amount: f32| {
            let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            lerp3(c, [l, l * 1.01, l * 1.06], amount)
        };
        let overcast = (clouds * 0.75 + rain * 0.35).min(1.0);
        let darken = 1.0 - 0.45 * rain - 0.15 * clouds;
        let fog_density = 0.0028 + 0.009 * mist * mist + 0.004 * rain + 0.0012 * clouds;
        let glow_dir = if sun_alt > -10.0 { sun_dir } else { moon_dir };
        Self {
            sun_dir,
            moon_dir,
            sun_altitude: sun_alt,
            key_dir,
            key_color,
            key_intensity,
            zenith: scale(grey(key.zenith, overcast), darken),
            horizon: scale(grey(key.horizon, overcast * 0.85), darken),
            glow: scale(grey(key.glow, overcast), (1.0 - 0.7 * overcast) * darken),
            glow_dir,
            ambient_sky: grey(key.ambient_sky, overcast * 0.6),
            ambient_ground: key.ambient_ground,
            ambient: key.ambient * (1.0 + 0.25 * clouds) * (1.0 - 0.2 * rain),
            fog_color: scale(grey(key.fog, overcast * 0.7), darken),
            fog_density,
            stars: key.stars * (1.0 - clouds).powf(2.0),
            sun_disc: sun_vis * (1.0 - clouds * 0.9),
            moon_disc: moon_vis * (1.0 - clouds * 0.8),
            clouds,
            cloud_lit: scale(grey(key.cloud_lit, rain * 0.7), 1.0 - 0.5 * rain),
            cloud_shadow: scale(grey(key.cloud_shadow, rain * 0.5), 1.0 - 0.4 * rain),
            lamp_level: 1.0 - smoothstep(-5.0, 7.0, sun_alt) * (1.0 - 0.6 * overcast),
            exposure: key.exposure,
            shadow_softness: 1.0 + 3.5 * clouds * clouds,
            rain,
            wetness: (rain * 1.2).min(1.0),
            mist,
            wind: [weather.wind * 0.8, weather.wind * 0.6],
            petals: weather.petals,
            seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sun_rises_in_the_east_and_peaks_south() {
        let mut t = TimeOfDay {
            north: [0.0, 0.0, -1.0],
            ..TimeOfDay::default()
        };
        t.hours = 12.0;
        let w = Weather::default();
        let p = Palette::default();
        let noon = Environment::resolve(&t, &w, &p, 0.0);
        assert!(noon.sun_altitude > 50.0, "{}", noon.sun_altitude);
        // North is -Z, so south is +Z.
        assert!(noon.sun_dir[2] > 0.0);
        t.hours = 7.0;
        let morning = Environment::resolve(&t, &w, &p, 0.0);
        // East is +X when north is -Z.
        assert!(morning.sun_dir[0] > 0.3, "{:?}", morning.sun_dir);
        t.hours = 0.0;
        let midnight = Environment::resolve(&t, &w, &p, 0.0);
        assert!(midnight.sun_altitude < -20.0);
        assert!(midnight.lamp_level > 0.99);
        assert!(midnight.moon_dir[1] > 0.5, "full moon high at midnight");
        assert_eq!(midnight.key_dir, midnight.moon_dir);
    }
}
