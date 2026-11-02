//! Lights a game places while it runs (a table's key light over the cards, a lantern it hangs):
//! point lights and spot lights, shaded like the cooked world's lamps in every pass, binned into
//! the same screen tiles, and moved, recoloured or put out at any time. A game light shines at
//! every hour unless it asks to follow the lamps (lit only as the evening lights the lamps).
//!
//! The light buffer holds the world's lamps, then room for [`MAX_GAME_LIGHTS`] game lights; a
//! change rewrites the game lights' part before the next frame.
use super::math::V3;
use super::renderer::Renderer;
use crate::gpu::Result;
use crate::gpu::pool::Pool;

handle!(
    /// A light a game placed.
    GameLight
);

/// The most game lights alive at once.
pub const MAX_GAME_LIGHTS: usize = 64;

/// A spot light's cone: it shines along `direction`, fully inside `inner` radians of it, fading
/// to nothing at `outer`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotCone {
    pub direction: V3,
    pub inner: f32,
    pub outer: f32,
}

/// A game light: where it hangs, how far it reaches (metres), its linear colour and intensity
/// (as a cooked lamp's), a cone for a spot light, and whether it follows the lamps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightDesc {
    pub position: V3,
    pub range: f32,
    pub color: [f32; 3],
    pub intensity: f32,
    pub spot: Option<SpotCone>,
    /// Lit only as much as the time of day lights the world's lamps (none by day).
    pub follows_lamps: bool,
}

/// A light as the shaders read it (`Light` in scene.wgsl): position and range, colour and
/// intensity, the spot's direction and the cosine of its outer edge (below -1.5: all round), and
/// the cosine of its inner edge and whether it shines whatever the hour.
pub(super) type LightRecord = [f32; 16];

/// A cooked lamp's record: all round, following the time of day.
pub(super) fn lamp_record(
    position: V3,
    range: f32,
    color: [f32; 3],
    intensity: f32,
) -> LightRecord {
    let [x, y, z] = position;
    let [r, g, b] = color;
    [
        x, y, z, range, r, g, b, intensity, 0.0, 0.0, 0.0, -2.0, 0.0, 0.0, 0.0, 0.0,
    ]
}

fn record(desc: &LightDesc) -> Result<LightRecord> {
    let mut values = vec![desc.range, desc.intensity];
    values.extend(desc.position);
    values.extend(desc.color);
    if let Some(s) = desc.spot {
        values.extend(s.direction);
        values.extend([s.inner, s.outer]);
    }
    if values.iter().any(|v| !v.is_finite()) {
        return Err("light: every value must be finite".into());
    }
    if desc.range <= 0.0 || desc.intensity < 0.0 || desc.color.iter().any(|c| *c < 0.0) {
        return Err("light: a positive range, and no negative colour or intensity".into());
    }
    let mut r = lamp_record(desc.position, desc.range, desc.color, desc.intensity);
    if let Some(s) = desc.spot {
        let [dx, dy, dz] = s.direction;
        let length = (dx * dx + dy * dy + dz * dz).sqrt();
        if length < 1e-6 {
            return Err("light: a spot needs a direction".into());
        }
        if !(0.0..std::f32::consts::PI).contains(&s.outer) || s.inner < 0.0 || s.inner > s.outer {
            return Err("light: a spot's edges must be 0 <= inner <= outer < pi".into());
        }
        r[8..11].copy_from_slice(&[dx / length, dy / length, dz / length]);
        r[11] = s.outer.cos();
        // An inner edge on the outer one would leave no fade: keep a sliver.
        r[12] = s.inner.cos().max(r[11] + 1e-4);
    }
    r[13] = if desc.follows_lamps { 0.0 } else { 1.0 };
    Ok(r)
}

#[derive(Default)]
pub(super) struct GameLights {
    pool: Pool<LightRecord>,
    /// The live records in the order the light buffer holds them, rebuilt on any change.
    list: Vec<LightRecord>,
    /// The light buffer's game part is stale.
    pub(super) dirty: bool,
}

impl GameLights {
    /// The live game lights in buffer order.
    pub(super) fn records(&self) -> &[LightRecord] {
        &self.list
    }

    fn rebuild(&mut self) {
        self.list = self.pool.iter().map(|(_, r)| *r).collect();
        self.dirty = true;
    }
}

impl Renderer {
    /// Hangs a light; it shines from the next frame.
    pub fn create_light(&mut self, desc: &LightDesc) -> Result<GameLight> {
        let r = record(desc)?;
        if self.game_lights.list.len() >= MAX_GAME_LIGHTS {
            return Err(format!(
                "create_light: at most {MAX_GAME_LIGHTS} game lights"
            ));
        }
        let id = self.game_lights.pool.insert(r)?;
        self.game_lights.rebuild();
        Ok(GameLight(id))
    }

    /// Moves, recolours or reshapes a light.
    pub fn set_light(&mut self, light: GameLight, desc: &LightDesc) -> Result<()> {
        let r = record(desc)?;
        let slot = self
            .game_lights
            .pool
            .get_mut(light.0)
            .ok_or_else(|| format!("{light:?} was put out"))?;
        if *slot != r {
            *slot = r;
            self.game_lights.rebuild();
        }
        Ok(())
    }

    /// Puts a light out.
    pub fn destroy_light(&mut self, light: GameLight) {
        if self.game_lights.pool.remove(light.0).is_some() {
            self.game_lights.rebuild();
        }
    }

    /// Live game lights.
    pub fn game_lights(&self) -> usize {
        self.game_lights.list.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lamp() -> LightDesc {
        LightDesc {
            position: [0.0, 3.0, 0.0],
            range: 6.0,
            color: [1.0, 0.8, 0.6],
            intensity: 2.0,
            spot: None,
            follows_lamps: false,
        }
    }

    #[test]
    fn records_say_how_a_light_shines() {
        let r = record(&lamp()).expect("lamp");
        assert_eq!(&r[..8], &[0.0, 3.0, 0.0, 6.0, 1.0, 0.8, 0.6, 2.0]);
        assert!(r[11] < -1.5, "all round");
        assert_eq!(r[13], 1.0, "whatever the hour");
        let spot = LightDesc {
            spot: Some(SpotCone {
                direction: [0.0, -2.0, 0.0],
                inner: 0.2,
                outer: 0.5,
            }),
            follows_lamps: true,
            ..lamp()
        };
        let r = record(&spot).expect("spot");
        assert_eq!(&r[8..11], &[0.0, -1.0, 0.0]);
        assert!((r[11] - 0.5f32.cos()).abs() < 1e-6 && (r[12] - 0.2f32.cos()).abs() < 1e-6);
        assert_eq!(r[13], 0.0, "with the lamps");
    }

    #[test]
    fn refuses_lights_that_cannot_shine() {
        for bad in [
            LightDesc {
                range: 0.0,
                ..lamp()
            },
            LightDesc {
                intensity: f32::NAN,
                ..lamp()
            },
            LightDesc {
                spot: Some(SpotCone {
                    direction: [0.0; 3],
                    inner: 0.1,
                    outer: 0.2,
                }),
                ..lamp()
            },
            LightDesc {
                spot: Some(SpotCone {
                    direction: [0.0, -1.0, 0.0],
                    inner: 0.4,
                    outer: 0.2,
                }),
                ..lamp()
            },
        ] {
            assert!(record(&bad).is_err(), "{bad:?}");
        }
    }
}
