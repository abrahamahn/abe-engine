//! Library mode: a host page (a game with its own interface) drives the renderer instead of
//! the player's fly camera. `web/abe.js` wraps these exports as `createEngine`.
//!
//! The host loads a cooked world (or none: a scene of props only) and a Look, then each frame
//! sets the camera, the clock and the weather, moves its props and calls [`abe_rt_render`].
//! Props (meshes, textures, materials and placed instances) are the engine's
//! `Renderer::create_prop_*` objects; their handles are never 0, so 0 always means failure, and
//! the reason has been sent to the host's `log` import at level 2.
//!
//! Memory: buffers the host passes are read during the call only. Floats and words are read
//! byte by byte (little-endian), so pointers need no alignment. A world handed to
//! [`abe_rt_start`] comes from `abe_alloc` and becomes the engine's.

use crate::{give, install_panic_hook, say};
use abe_engine::anim::normalize_q;
use abe_engine::assets::model::{self, Model};
use abe_engine::gpu::Device;
use abe_engine::render::{
    Environment, LookFile, MaterialDesc, MeshData, Output, PROP_SHADER_PARAMS, PropInstance,
    PropMaterial, PropMesh, PropTexture, Quality, Renderer, TimeOfDay, View, Weather,
};
use abe_engine::render::{GameLight, LightDesc, ModelId, ModelInstance, SpotCone};
use abe_engine::world::World;
use std::cell::RefCell;

struct Runtime {
    device: Device<'static>,
    renderer: Renderer,
    view: View,
    time: TimeOfDay,
    weather: Weather,
    seconds: f32,
    frames: u32,
    /// Parsed models waiting for their textures (handle = index + 1).
    pending: Vec<Option<Model>>,
}

thread_local! {
    static RT: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Runs `f` on the runtime; errors (and a missing runtime) are logged and give `fail`.
fn with<T>(fail: T, f: impl FnOnce(&mut Runtime) -> Result<T, String>) -> T {
    RT.with(|cell| match cell.borrow_mut().as_mut() {
        None => {
            say(2, "the engine is not started (abe_rt_start)");
            fail
        }
        Some(rt) => match f(rt) {
            Ok(v) => v,
            Err(e) => {
                say(2, &e);
                fail
            }
        },
    })
}

/// `len` bytes at `ptr`, or nothing for a zero length.
///
/// # Safety
/// `ptr` must address `len` readable bytes for the duration of the call.
unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 || ptr.is_null() {
        &[]
    } else {
        // SAFETY: the caller guarantees the range is readable.
        unsafe { std::slice::from_raw_parts(ptr, len) }
    }
}

/// `count` little-endian f32 at `ptr` (any alignment).
///
/// # Safety
/// As [`bytes`] for `count * 4` bytes.
unsafe fn floats(ptr: *const u8, count: usize) -> Vec<f32> {
    // SAFETY: forwarded.
    unsafe { bytes(ptr, count * 4) }
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// `count` little-endian u32 at `ptr` (any alignment).
///
/// # Safety
/// As [`bytes`] for `count * 4` bytes.
unsafe fn words(ptr: *const u8, count: usize) -> Vec<u32> {
    // SAFETY: forwarded.
    unsafe { bytes(ptr, count * 4) }
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn triples(v: &[f32]) -> Vec<[f32; 3]> {
    v.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect()
}

fn matrix(v: &[f32]) -> Result<[f32; 16], String> {
    let m: [f32; 16] = v.try_into().map_err(|_| "a matrix needs 16 floats")?;
    if m.iter().all(|x| x.is_finite()) {
        Ok(m)
    } else {
        Err("the matrix is not finite".into())
    }
}

/// Starts the engine on the page's canvas with the `.abew` world at `world` (ownership passes
/// from `abe_alloc`; a zero length takes the world `abe_load_world` decoded, whose images the
/// host may have decoded with `abe_world_set_image`, or none) and the Look file at `look`.
/// `quality` 1 renders at still-image quality, 0 at real-time quality. Returns 1, or 0 after
/// logging why not.
///
/// # Safety
/// `world` must come from `abe_alloc(world_len)` fully written (or be null with a zero length);
/// `look` must address `look_len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_start(
    world: *mut u8,
    world_len: usize,
    look: *const u8,
    look_len: usize,
    quality: u32,
) -> u32 {
    install_panic_hook();
    let world = if world_len == 0 || world.is_null() {
        Ok(crate::WORLD
            .with(|cell| cell.borrow_mut().take())
            .unwrap_or_default())
    } else {
        // SAFETY: the caller hands over an initialized allocation of `world_len` bytes.
        let owned = unsafe { Vec::from_raw_parts(world, world_len, world_len) };
        abe_engine::assets::cooked::decode(&owned).map_err(|e| format!("world: {e}"))
    };
    // SAFETY: the caller passes a readable range.
    let look = unsafe { bytes(look, look_len) };
    match world.and_then(|w| start(&w, look, quality)) {
        Ok(rt) => {
            say(
                0,
                &format!(
                    "ABE engine | {} | {} triangles, {} lamps",
                    rt.device.name(),
                    rt.renderer.triangles(),
                    rt.renderer.lights()
                ),
            );
            RT.with(|cell| *cell.borrow_mut() = Some(rt));
            1
        }
        Err(e) => {
            say(2, &e);
            0
        }
    }
}

fn start(world: &World, look: &[u8], quality: u32) -> Result<Runtime, String> {
    let file = if look.is_empty() {
        LookFile::default()
    } else {
        LookFile::parse(look).map_err(|e| format!("Look file: {e}"))?
    };
    let mut device = Device::for_canvas()?;
    let quality = if quality == 1 {
        Quality::still()
    } else {
        Quality::realtime()
    };
    let (w, h) = device.surface_size();
    let format = device.surface_format();
    let renderer = Renderer::new(
        &mut device,
        world,
        file.look.clone(),
        quality,
        (w.max(1), h.max(1)),
        Output::Surface(format),
    )?;
    Ok(Runtime {
        device,
        renderer,
        view: file.views.first().cloned().unwrap_or_default(),
        time: file.time,
        weather: file.weather,
        seconds: 0.0,
        frames: 0,
        pending: vec![],
    })
}

/// Stops the engine and releases its GPU objects. Returns 1.
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_stop() -> u32 {
    RT.with(|cell| {
        if let Some(mut rt) = cell.borrow_mut().take() {
            rt.renderer.destroy_props(&mut rt.device);
        }
    });
    1
}

/// Replaces the Look (the file's look block; its views, time and weather are ignored).
///
/// # Safety
/// `ptr` must address `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_set_look(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: the caller passes a readable range.
    let text = unsafe { bytes(ptr, len) };
    with(0, |rt| {
        let file = LookFile::parse(text).map_err(|e| format!("Look file: {e}"))?;
        rt.renderer.set_look(file.look);
        Ok(1)
    })
}

/// Sets the camera from 13 floats at `ptr`: eye (3), target (3), up (3), vertical field of view
/// in degrees, lens shift (2, normalized device units, y up), near clip distance.
///
/// # Safety
/// `ptr` must address 52 readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_set_view(ptr: *const u8) -> u32 {
    // SAFETY: the caller passes 13 floats.
    let v = unsafe { floats(ptr, 13) };
    with(0, |rt| {
        if v.iter().any(|x| !x.is_finite()) {
            return Err("set_view: values must be finite".into());
        }
        if !(v[9] > 0.0 && v[9] < 180.0 && v[12] > 0.0) {
            return Err(format!(
                "set_view: fov {} or near {} out of range",
                v[9], v[12]
            ));
        }
        rt.view = View {
            name: "host".into(),
            eye: [v[0], v[1], v[2]],
            target: [v[3], v[4], v[5]],
            up: [v[6], v[7], v[8]],
            fov: v[9],
            shift: [v[10], v[11]],
            near: v[12],
        };
        Ok(1)
    })
}

/// Sets the time of day (`hours`, 0..24) and the animation clock (`seconds`: water, clouds,
/// grass and weather move with it).
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_set_clock(hours: f32, seconds: f32) -> u32 {
    with(0, |rt| {
        if !hours.is_finite() || !seconds.is_finite() {
            return Err("set_clock: values must be finite".into());
        }
        rt.time.hours = hours.rem_euclid(24.0);
        rt.seconds = seconds;
        Ok(1)
    })
}

/// Sets the weather to a preset (`clear`, `cloudy`, `overcast`, `mist`, `rain`, `storm`,
/// `petals`) named by the UTF-8 text at `ptr`.
///
/// # Safety
/// `ptr` must address `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_set_weather(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: the caller passes a readable range.
    let name = String::from_utf8_lossy(unsafe { bytes(ptr, len) }).into_owned();
    with(0, |rt| {
        rt.weather = Weather::preset(&name).ok_or_else(|| format!("unknown weather {name:?}"))?;
        Ok(1)
    })
}

/// Renders and presents a frame. Returns 1 when presented, 2 when the canvas had no image to
/// draw into (nothing to do), 0 after logging an error.
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_render() -> u32 {
    with(0, |rt| {
        let Some(target) = rt.device.acquire()? else {
            return Ok(2);
        };
        let size = rt.device.surface_size();
        if size != rt.renderer.size() && size.0 > 0 && size.1 > 0 {
            rt.renderer.resize(&mut rt.device, size)?;
        }
        let env = Environment::resolve(
            &rt.time,
            &rt.weather,
            &rt.renderer.look().palette,
            rt.seconds,
        );
        let mut enc = rt.device.encoder();
        rt.renderer.encode(
            &mut rt.device,
            &mut enc,
            &rt.view,
            &env,
            [0.0, 0.0],
            0,
            Some(target),
        )?;
        rt.device.present(enc)?;
        rt.frames = rt.frames.wrapping_add(1);
        Ok(1)
    })
}

/// Frames presented so far.
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_frames() -> u32 {
    RT.with(|cell| cell.borrow().as_ref().map_or(0, |rt| rt.frames))
}

/// Writes the size the renderer draws at (device pixels) as two words at `out`.
///
/// # Safety
/// `out` must address 8 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_size(out: *mut u8) -> u32 {
    with(0, |rt| {
        let (w, h) = rt.renderer.size();
        let mut b = [0u8; 8];
        b[..4].copy_from_slice(&w.to_le_bytes());
        b[4..].copy_from_slice(&h.to_le_bytes());
        // SAFETY: the caller passes 8 writable bytes.
        unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), out, 8) };
        Ok(1)
    })
}

/// A texture of `width` x `height` RGBA8 pixels (`len` = width * height * 4 bytes at `ptr`),
/// sRGB colour (`srgb` 1) or linear data such as normal maps (0), with a full mip chain.
///
/// # Safety
/// `ptr` must address `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_texture_create(
    width: u32,
    height: u32,
    ptr: *const u8,
    len: usize,
    srgb: u32,
) -> u32 {
    // SAFETY: the caller passes a readable range.
    let data = unsafe { bytes(ptr, len) };
    with(0, |rt| {
        if width == 0
            || height == 0
            || data.len() as u64 != u64::from(width) * u64::from(height) * 4
        {
            return Err(format!(
                "texture: {} bytes for {width}x{height} RGBA8",
                data.len()
            ));
        }
        let t =
            rt.renderer
                .create_prop_texture(&mut rt.device, (width, height), data, srgb != 0)?;
        Ok(t.raw())
    })
}

/// A texture from a PNG file (`len` bytes at `ptr`), decoded by the engine: sRGB colour
/// (`srgb` 1) or linear data (0), with a full mip chain.
///
/// # Safety
/// `ptr` must address `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_texture_png(ptr: *const u8, len: usize, srgb: u32) -> u32 {
    // SAFETY: the caller passes a readable range.
    let data = unsafe { bytes(ptr, len) };
    with(0, |rt| {
        let image = abe_engine::assets::png::decode(data).map_err(|e| format!("PNG: {e}"))?;
        let t = rt.renderer.create_prop_texture(
            &mut rt.device,
            (image.width, image.height),
            &image.rgba,
            srgb != 0,
        )?;
        Ok(t.raw())
    })
}

/// Replaces a texture's pixels (same size).
///
/// # Safety
/// `ptr` must address `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_texture_update(texture: u32, ptr: *const u8, len: usize) -> u32 {
    // SAFETY: the caller passes a readable range.
    let data = unsafe { bytes(ptr, len) };
    with(0, |rt| {
        rt.renderer
            .update_prop_texture(&mut rt.device, PropTexture::from_raw(texture), data)?;
        Ok(1)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_texture_destroy(texture: u32) -> u32 {
    with(0, |rt| {
        rt.renderer
            .destroy_prop_texture(&mut rt.device, PropTexture::from_raw(texture));
        Ok(1)
    })
}

/// A mesh: `vertices` positions (3 floats each) at `positions`, normals likewise at `normals`
/// (null for flat normals), uvs (2 floats each) at `uvs` (null for none), and `index_count`
/// u32 indices at `indices` (counter-clockwise triangles).
///
/// # Safety
/// Each non-null pointer must address the floats or words its count implies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_mesh_create(
    positions: *const u8,
    vertices: usize,
    normals: *const u8,
    uvs: *const u8,
    indices: *const u8,
    index_count: usize,
) -> u32 {
    // SAFETY: the caller passes readable arrays of the stated sizes.
    let (p, n, uv, idx) = unsafe {
        (
            triples(&floats(positions, vertices * 3)),
            if normals.is_null() {
                vec![]
            } else {
                triples(&floats(normals, vertices * 3))
            },
            if uvs.is_null() {
                vec![]
            } else {
                floats(uvs, vertices * 2)
                    .chunks_exact(2)
                    .map(|c| [c[0], c[1]])
                    .collect()
            },
            words(indices, index_count),
        )
    };
    with(0, |rt| {
        let m = rt.renderer.create_prop_mesh(
            &mut rt.device,
            &MeshData {
                positions: &p,
                normals: &n,
                uvs: &uv,
                indices: &idx,
            },
        )?;
        Ok(m.raw())
    })
}

/// New positions (and normals, or null for flat ones) for a mesh's `vertices` vertices.
///
/// # Safety
/// As [`abe_rt_mesh_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_mesh_update(
    mesh: u32,
    positions: *const u8,
    normals: *const u8,
    vertices: usize,
) -> u32 {
    // SAFETY: the caller passes readable arrays of the stated sizes.
    let (p, n) = unsafe {
        (
            triples(&floats(positions, vertices * 3)),
            if normals.is_null() {
                vec![]
            } else {
                triples(&floats(normals, vertices * 3))
            },
        )
    };
    with(0, |rt| {
        rt.renderer
            .update_prop_mesh(&mut rt.device, PropMesh::from_raw(mesh), &p, &n)?;
        Ok(1)
    })
}

/// Destroys a mesh and every instance of it.
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_mesh_destroy(mesh: u32) -> u32 {
    with(0, |rt| {
        rt.renderer
            .destroy_prop_mesh(&mut rt.device, PropMesh::from_raw(mesh));
        Ok(1)
    })
}

/// A material from its JSON description (see `MaterialDesc::from_json`).
///
/// # Safety
/// `ptr` must address `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_material_create(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: the caller passes a readable range.
    let text = unsafe { bytes(ptr, len) };
    with(0, |rt| {
        let desc = MaterialDesc::from_json(text)?;
        Ok(rt
            .renderer
            .create_prop_material(&mut rt.device, &desc)?
            .raw())
    })
}

/// Compiles a game's prop shader: WGSL defining `fn shade(in: ShadeInput) -> vec4f` (UTF-8,
/// `len` bytes at `ptr`). Returns its handle; 0 with the compiler's message on failure.
///
/// # Safety
/// `ptr` must address `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_shader_create(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: the caller passes a readable range.
    let text = unsafe { bytes(ptr, len) };
    with(0, |rt| {
        let source = std::str::from_utf8(text).map_err(|e| format!("shader: {e}"))?;
        Ok(rt
            .renderer
            .create_prop_shader(&mut rt.device, source)?
            .raw())
    })
}

/// A material's twenty prop-shader parameters (floats at `ptr`), without a rebuild.
///
/// # Safety
/// `ptr` must address 80 readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_material_params(material: u32, ptr: *const u8) -> u32 {
    // SAFETY: the caller passes twenty floats.
    let v = unsafe { floats(ptr, PROP_SHADER_PARAMS) };
    with(0, |rt| {
        let mut params = [0.0f32; PROP_SHADER_PARAMS];
        params.copy_from_slice(&v);
        rt.renderer
            .set_prop_material_params(PropMaterial::from_raw(material), &params)?;
        Ok(1)
    })
}

/// Changes a material in place (every instance that uses it changes with it).
///
/// # Safety
/// `ptr` must address `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_material_update(material: u32, ptr: *const u8, len: usize) -> u32 {
    // SAFETY: the caller passes a readable range.
    let text = unsafe { bytes(ptr, len) };
    with(0, |rt| {
        let desc = MaterialDesc::from_json(text)?;
        rt.renderer.update_prop_material(
            &mut rt.device,
            PropMaterial::from_raw(material),
            &desc,
        )?;
        Ok(1)
    })
}

/// Destroys a material and every instance that uses it.
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_material_destroy(material: u32) -> u32 {
    with(0, |rt| {
        rt.renderer
            .destroy_prop_material(&mut rt.device, PropMaterial::from_raw(material));
        Ok(1)
    })
}

/// Places `mesh` with `material` at the column-major model matrix (16 floats) at `matrix`.
///
/// # Safety
/// `matrix` must address 64 readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_instance_create(
    mesh: u32,
    material: u32,
    matrix: *const u8,
) -> u32 {
    // SAFETY: the caller passes 16 floats.
    let m = unsafe { floats(matrix, 16) };
    with(0, |rt| {
        let i = rt.renderer.create_prop_instance(
            PropMesh::from_raw(mesh),
            PropMaterial::from_raw(material),
            &self::matrix(&m)?,
        )?;
        Ok(i.raw())
    })
}

/// Moves an instance and shows (`visible` 1) or hides it.
///
/// # Safety
/// `matrix` must address 64 readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_instance_set(
    instance: u32,
    matrix: *const u8,
    visible: u32,
) -> u32 {
    // SAFETY: the caller passes 16 floats.
    let m = unsafe { floats(matrix, 16) };
    with(0, |rt| {
        rt.renderer.set_prop_instance(
            PropInstance::from_raw(instance),
            &self::matrix(&m)?,
            visible != 0,
        )?;
        Ok(1)
    })
}

/// Moves many instances at once: `count` records of 18 words at `ptr`, each the instance, 1 to
/// show it or 0 to hide it, and its 16-float model matrix. Stops at the first bad record.
///
/// # Safety
/// `ptr` must address `count * 72` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_instances_set(ptr: *const u8, count: usize) -> u32 {
    // SAFETY: the caller passes `count` records.
    let raw = unsafe { bytes(ptr, count * 72) };
    with(0, |rt| {
        for r in raw.chunks_exact(72) {
            let word = |i: usize| u32::from_le_bytes([r[i], r[i + 1], r[i + 2], r[i + 3]]);
            let m: Vec<f32> = r[8..]
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            rt.renderer.set_prop_instance(
                PropInstance::from_raw(word(0)),
                &matrix(&m)?,
                word(4) != 0,
            )?;
        }
        Ok(1)
    })
}

/// Gives an instance another material.
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_instance_material(instance: u32, material: u32) -> u32 {
    with(0, |rt| {
        rt.renderer.set_prop_instance_material(
            PropInstance::from_raw(instance),
            PropMaterial::from_raw(material),
        )?;
        Ok(1)
    })
}

/// Multiplies an instance's material colour and alpha by (`r`, `g`, `b`, `a`).
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_instance_tint(instance: u32, r: f32, g: f32, b: f32, a: f32) -> u32 {
    with(0, |rt| {
        rt.renderer
            .set_prop_instance_tint(PropInstance::from_raw(instance), [r, g, b, a])?;
        Ok(1)
    })
}

/// The pick layers (bits) an instance answers to; 0: never picked.
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_instance_pick(instance: u32, layers: u32) -> u32 {
    with(0, |rt| {
        rt.renderer
            .set_prop_instance_pick(PropInstance::from_raw(instance), layers)?;
        Ok(1)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_instance_destroy(instance: u32) -> u32 {
    with(0, |rt| {
        rt.renderer
            .destroy_prop_instance(PropInstance::from_raw(instance));
        Ok(1)
    })
}

/// A game light from 14 floats: position (3), range, linear colour (3), intensity, a spot's
/// direction (3, all zero for a point light), its inner and outer half-angles (radians), and 1 to
/// follow the lamps (lit only as the evening lights them).
fn light_desc(v: &[f32]) -> LightDesc {
    let at = |i: usize| v.get(i).copied().unwrap_or(0.0);
    let direction = [at(8), at(9), at(10)];
    LightDesc {
        position: [at(0), at(1), at(2)],
        range: at(3),
        color: [at(4), at(5), at(6)],
        intensity: at(7),
        spot: direction.iter().any(|d| *d != 0.0).then_some(SpotCone {
            direction,
            inner: at(11),
            outer: at(12),
        }),
        follows_lamps: at(13) > 0.5,
    }
}

/// Hangs a game light (see `light_desc` for the 14 floats at `ptr`).
///
/// # Safety
/// `ptr` must address 56 readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_light_create(ptr: *const u8) -> u32 {
    // SAFETY: the caller passes 14 floats.
    let v = unsafe { floats(ptr, 14) };
    with(0, |rt| Ok(rt.renderer.create_light(&light_desc(&v))?.raw()))
}

/// Moves, recolours or reshapes a game light (14 floats at `ptr`, as `abe_rt_light_create`).
///
/// # Safety
/// `ptr` must address 56 readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_light_set(light: u32, ptr: *const u8) -> u32 {
    // SAFETY: the caller passes 14 floats.
    let v = unsafe { floats(ptr, 14) };
    with(0, |rt| {
        rt.renderer
            .set_light(GameLight::from_raw(light), &light_desc(&v))?;
        Ok(1)
    })
}

/// Puts a game light out.
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_light_destroy(light: u32) -> u32 {
    with(0, |rt| {
        rt.renderer.destroy_light(GameLight::from_raw(light));
        Ok(1)
    })
}

/// Where the world point (`x`, `y`, `z`) lands in the image: writes pixel x, pixel y (device
/// pixels, top-left origin) and the depth in metres as 3 floats at `out`. Returns 0 (without
/// logging) when the point is behind the camera.
///
/// # Safety
/// `out` must address 12 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_project(x: f32, y: f32, z: f32, out: *mut u8) -> u32 {
    with(0, |rt| {
        let size = rt.renderer.size();
        let Some(p) = rt.renderer.project(&rt.view, size, [x, y, z]) else {
            return Ok(0);
        };
        // SAFETY: the caller passes 12 writable bytes.
        unsafe { write_floats(out, &p) };
        Ok(1)
    })
}

/// The visible prop under device pixel (`x`, `y`) on one of the pick `layers` (as
/// `Renderer::pick` chooses it): returns its instance and writes the distance in metres and
/// the world point (4 floats) at `out`; returns 0 (without logging) when no prop is there.
///
/// # Safety
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_pick(x: f32, y: f32, layers: u32, out: *mut u8) -> u32 {
    with(0, |rt| {
        let size = rt.renderer.size();
        let Some(hit) = rt.renderer.pick(&rt.view, size, (x, y), layers) else {
            return Ok(0);
        };
        let p = hit.point;
        // SAFETY: the caller passes 16 writable bytes.
        unsafe { write_floats(out, &[hit.distance, p[0], p[1], p[2]]) };
        Ok(hit.instance.raw())
    })
}

/// # Safety
/// `out` must address `v.len() * 4` writable bytes.
unsafe fn write_floats(out: *mut u8, v: &[f32]) {
    let b: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
    // SAFETY: forwarded.
    unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), out, b.len()) };
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A model's contents as JSON, for the host: images to decode (type, colour or data, size),
/// nodes (name, parent, mesh), meshes (name, morph target names), clips (name, duration and the
/// nodes they animate) and material names. Indices are positions in these lists.
fn describe(m: &Model) -> String {
    let list = |items: Vec<String>| format!("[{}]", items.join(","));
    let opt = |o: Option<usize>| o.map_or("null".to_string(), |v| v.to_string());
    let images = m
        .images
        .iter()
        .map(|i| {
            format!(
                r#"{{"name":{},"mime":{},"srgb":{},"size":{}}}"#,
                json_string(&i.name),
                json_string(&i.mime),
                i.srgb,
                i.data.len()
            )
        })
        .collect();
    let nodes = m
        .nodes
        .iter()
        .map(|n| {
            format!(
                r#"{{"name":{},"parent":{},"mesh":{},"skin":{}}}"#,
                json_string(&n.name),
                opt(n.parent),
                opt(n.mesh),
                opt(n.skin)
            )
        })
        .collect();
    let meshes = m
        .meshes
        .iter()
        .map(|me| {
            format!(
                r#"{{"name":{},"targets":{},"targetNames":{}}}"#,
                json_string(&me.name),
                me.weights.len(),
                list(me.target_names.iter().map(|t| json_string(t)).collect())
            )
        })
        .collect();
    let clips = m
        .clips
        .iter()
        .map(|c| {
            // Each node a channel moves, once, in the order first met.
            let mut nodes: Vec<usize> = Vec::new();
            for channel in &c.channels {
                if !nodes.contains(&channel.node) {
                    nodes.push(channel.node);
                }
            }
            format!(
                r#"{{"name":{},"duration":{},"nodes":{}}}"#,
                json_string(&c.name),
                c.duration,
                list(nodes.iter().map(usize::to_string).collect())
            )
        })
        .collect();
    let materials = m.materials.iter().map(|x| json_string(&x.name)).collect();
    format!(
        r#"{{"images":{},"nodes":{},"meshes":{},"clips":{},"materials":{}}}"#,
        list(images),
        list(nodes),
        list(meshes),
        list(clips),
        list(materials)
    )
}

fn pending(rt: &mut Runtime, id: u32) -> Result<&mut Model, String> {
    rt.pending
        .get_mut((id as usize).wrapping_sub(1))
        .and_then(Option::as_mut)
        .ok_or_else(|| format!("no parsed model {id}"))
}

/// Parses a GLB model (`len` bytes at `ptr`, read during the call). Returns a parsed-model
/// handle for [`abe_rt_model_describe`], [`abe_rt_model_image`] and [`abe_rt_model_create`].
///
/// # Safety
/// `ptr` must address `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_model_parse(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: the caller passes a readable range.
    let data = unsafe { bytes(ptr, len) };
    with(0, |rt| {
        let m = model::load_glb(data).map_err(|e| format!("model: {e}"))?;
        let slot = rt.pending.iter().position(Option::is_none);
        let i = match slot {
            Some(i) => {
                rt.pending[i] = Some(m);
                i
            }
            None => {
                rt.pending.push(Some(m));
                rt.pending.len() - 1
            }
        };
        Ok(i as u32 + 1)
    })
}

/// Writes a parsed model's description (JSON, see `describe`) to `out` if it fits in `cap`
/// bytes; returns its length.
///
/// # Safety
/// `out` must address `cap` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_model_describe(parsed: u32, out: *mut u8, cap: usize) -> u32 {
    with(0, |rt| {
        let text = describe(pending(rt, parsed)?);
        // SAFETY: forwarded.
        Ok(unsafe { give(text.as_bytes(), out, cap) })
    })
}

/// Copies image `index` of a parsed model (encoded PNG, JPEG or WebP) to `out` if it fits in
/// `cap` bytes; returns its length.
///
/// # Safety
/// `out` must address `cap` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_model_image(
    parsed: u32,
    index: u32,
    out: *mut u8,
    cap: usize,
) -> u32 {
    with(0, |rt| {
        let m = pending(rt, parsed)?;
        let image = m
            .images
            .get(index as usize)
            .ok_or_else(|| format!("no image {index}"))?;
        // SAFETY: forwarded.
        Ok(unsafe { give(&image.data, out, cap) })
    })
}

/// Uploads a parsed model with its textures (`count` texture handles at `textures`, one per
/// image, 0 where an image could not be decoded). Returns the model handle; the parsed handle
/// is spent either way.
///
/// # Safety
/// `textures` must address `count` words.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_model_create(
    parsed: u32,
    textures: *const u8,
    count: usize,
) -> u32 {
    // SAFETY: the caller passes `count` words.
    let tex: Vec<Option<PropTexture>> = unsafe { words(textures, count) }
        .into_iter()
        .map(|t| (t != 0).then(|| PropTexture::from_raw(t)))
        .collect();
    with(0, |rt| {
        let slot = (parsed as usize).wrapping_sub(1);
        let m = rt
            .pending
            .get_mut(slot)
            .and_then(Option::take)
            .ok_or_else(|| format!("no parsed model {parsed}"))?;
        Ok(rt.renderer.create_model(&mut rt.device, m, &tex)?.raw())
    })
}

/// The prop material made for the model's material `index` (to restyle it).
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_model_material(model: u32, index: u32) -> u32 {
    with(0, |rt| {
        Ok(rt
            .renderer
            .model_material(ModelId::from_raw(model), index as usize)?
            .raw())
    })
}

/// Destroys a model and its instances (textures stay the host's).
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_model_destroy(model: u32) -> u32 {
    with(0, |rt| {
        rt.renderer
            .destroy_model(&mut rt.device, ModelId::from_raw(model));
        Ok(1)
    })
}

/// Places a model at the column-major matrix (16 floats) at `matrix`, in its rest pose.
///
/// # Safety
/// `matrix` must address 64 readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_model_instance(model: u32, matrix: *const u8) -> u32 {
    // SAFETY: the caller passes 16 floats.
    let m = unsafe { floats(matrix, 16) };
    with(0, |rt| {
        let i = rt.renderer.create_model_instance(
            &mut rt.device,
            ModelId::from_raw(model),
            &self::matrix(&m)?,
        )?;
        Ok(i.raw())
    })
}

/// Moves a model instance and shows (`visible` 1) or hides it.
///
/// # Safety
/// `matrix` must address 64 readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_model_instance_set(
    instance: u32,
    matrix: *const u8,
    visible: u32,
) -> u32 {
    // SAFETY: the caller passes 16 floats.
    let m = unsafe { floats(matrix, 16) };
    with(0, |rt| {
        rt.renderer.set_model_instance(
            ModelInstance::from_raw(instance),
            &self::matrix(&m)?,
            visible != 0,
        )?;
        Ok(1)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_model_instance_destroy(instance: u32) -> u32 {
    with(0, |rt| {
        rt.renderer
            .destroy_model_instance(&mut rt.device, ModelInstance::from_raw(instance));
        Ok(1)
    })
}

/// Resets an instance's pose to the model's rest pose.
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_model_rest(instance: u32) -> u32 {
    with(0, |rt| {
        let (pose, model) = rt
            .renderer
            .model_pose_and_model(ModelInstance::from_raw(instance))?;
        *pose = abe_engine::anim::Pose::rest(model);
        Ok(1)
    })
}

/// Mixes clip `clip` at `time` seconds into the instance's pose with `weight` (1 replaces what
/// the clip animates).
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_model_clip(instance: u32, clip: u32, time: f32, weight: f32) -> u32 {
    with(0, |rt| {
        let (pose, model) = rt
            .renderer
            .model_pose_and_model(ModelInstance::from_raw(instance))?;
        if clip as usize >= model.clips.len() {
            return Err(format!("no clip {clip}"));
        }
        pose.apply(model, clip as usize, time, weight);
        Ok(1)
    })
}

/// Writes node `node`'s local transform as 10 floats at `out`: translation (3), rotation
/// quaternion x, y, z, w (4), scale (3).
///
/// # Safety
/// `out` must address 40 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_model_node_get(instance: u32, node: u32, out: *mut u8) -> u32 {
    with(0, |rt| {
        let (pose, _) = rt
            .renderer
            .model_pose_and_model(ModelInstance::from_raw(instance))?;
        let n = node as usize;
        if n >= pose.translation.len() {
            return Err(format!("no node {node}"));
        }
        let mut v = [0.0f32; 10];
        v[..3].copy_from_slice(&pose.translation[n]);
        v[3..7].copy_from_slice(&pose.rotation[n]);
        v[7..].copy_from_slice(&pose.scale[n]);
        // SAFETY: the caller passes 40 writable bytes.
        unsafe { write_floats(out, &v) };
        Ok(1)
    })
}

/// Sets node `node`'s local transform from 10 floats at `ptr` (as `abe_rt_model_node_get`
/// writes them; the rotation is normalized).
///
/// # Safety
/// `ptr` must address 40 readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_model_node_set(instance: u32, node: u32, ptr: *const u8) -> u32 {
    // SAFETY: the caller passes 10 floats.
    let v = unsafe { floats(ptr, 10) };
    with(0, |rt| {
        if v.iter().any(|x| !x.is_finite()) {
            return Err("model_node_set: values must be finite".into());
        }
        let (pose, _) = rt
            .renderer
            .model_pose_and_model(ModelInstance::from_raw(instance))?;
        let n = node as usize;
        if n >= pose.translation.len() {
            return Err(format!("no node {node}"));
        }
        pose.translation[n] = [v[0], v[1], v[2]];
        pose.rotation[n] = normalize_q([v[3], v[4], v[5], v[6]]);
        pose.scale[n] = [v[7], v[8], v[9]];
        Ok(1)
    })
}

/// Sets morph target `target` of node `node`'s mesh to `weight`.
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_model_morph(instance: u32, node: u32, target: u32, weight: f32) -> u32 {
    with(0, |rt| {
        let (pose, _) = rt
            .renderer
            .model_pose_and_model(ModelInstance::from_raw(instance))?;
        let w = pose
            .weights
            .get_mut(node as usize)
            .and_then(|w| w.get_mut(target as usize))
            .ok_or_else(|| format!("node {node} has no morph target {target}"))?;
        if !weight.is_finite() {
            return Err("model_morph: the weight must be finite".into());
        }
        *w = weight;
        Ok(1)
    })
}

/// Applies the instance's pose: node transforms, skinning and morphing.
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_model_update(instance: u32) -> u32 {
    with(0, |rt| {
        rt.renderer
            .update_model_instance(&mut rt.device, ModelInstance::from_raw(instance))?;
        Ok(1)
    })
}

/// Writes node `node`'s world matrix (16 floats, as of the last update) at `out`.
///
/// # Safety
/// `out` must address 64 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_model_node_world(instance: u32, node: u32, out: *mut u8) -> u32 {
    with(0, |rt| {
        let m = rt
            .renderer
            .model_node_world(ModelInstance::from_raw(instance), node as usize)?;
        // SAFETY: the caller passes 64 writable bytes.
        unsafe { write_floats(out, &m) };
        Ok(1)
    })
}

/// Shows (`visible` 1) or hides the meshes of node `node`.
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_model_node_visible(instance: u32, node: u32, visible: u32) -> u32 {
    with(0, |rt| {
        rt.renderer.set_model_node_visible(
            ModelInstance::from_raw(instance),
            node as usize,
            visible != 0,
        )?;
        Ok(1)
    })
}

/// Tints every part of a model instance.
#[unsafe(no_mangle)]
pub extern "C" fn abe_rt_model_tint(instance: u32, r: f32, g: f32, b: f32, a: f32) -> u32 {
    with(0, |rt| {
        rt.renderer
            .set_model_instance_tint(ModelInstance::from_raw(instance), [r, g, b, a])?;
        Ok(1)
    })
}

/// The model instance a picked prop instance belongs to (0 for none); writes the node it draws
/// as a word at `out`.
///
/// # Safety
/// `out` must address 4 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn abe_rt_model_owner(prop: u32, out: *mut u8) -> u32 {
    with(0, |rt| {
        let Some((i, node)) = rt.renderer.model_part_owner(PropInstance::from_raw(prop)) else {
            return Ok(0);
        };
        let b = (node as u32).to_le_bytes();
        // SAFETY: the caller passes 4 writable bytes.
        unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), out, 4) };
        Ok(i.raw())
    })
}
