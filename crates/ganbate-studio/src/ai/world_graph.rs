//! Native Rust Tri-Modal Semantic World Graph (`ganbate://context`).
//!
//! Persists a `<400`-token canonical `SemanticWorldGraph` at `<project_root>/world.semantic.json`,
//! enforces `120Hz` (`8.33ms`) performance guardrails and `server-authoritative-readonly` security
//! boundaries, and deterministically compiles bilingual (EN/KO) natural-language prompts into
//! validated patches and `Outbox` broadcast payloads.

use std::collections::BTreeMap;
use std::path::Path;

use ganbate_core::json::{self, Value};

use crate::session::{ClientId, Outbox};

pub const WORLD_GRAPH_REL_PATH: &str = "world.semantic.json";
pub const WORLD_GRAPH_SCHEMA: &str = "ganbate.semantic-world-graph.v1";
pub const WORLD_GRAPH_URI: &str = "ganbate://context";
pub const AUTHORITY_MODE_READONLY: &str = "server-authoritative-readonly";

pub const TARGET_HZ_120: u32 = 120;
pub const FRAME_BUDGET_120HZ_MS: f32 = 8.33;
pub const MAX_DYNAMIC_LIGHTS_120HZ: u32 = 8;
pub const MAX_DRAW_CALLS_120HZ: u32 = 500;
pub const MAX_STAGE_ENTITIES: usize = 16;

/// Living Canvas runtime mode (`"play"` vs `"edit"`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayMode {
    #[default]
    Play,
    Edit,
}

impl PlayMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Play => "play",
            Self::Edit => "edit",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "play" | "live" | "running" => Some(Self::Play),
            "edit" | "paused" | "pause" | "inspect" => Some(Self::Edit),
            _ => None,
        }
    }
}

/// Performance guardrails locked to `120Hz` (`8.33ms`) comfort limits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldGuardrails {
    pub target_hz: u32,
    pub frame_budget_ms: f32,
    pub max_dynamic_lights: u32,
    pub max_draw_calls: u32,
}

impl Default for WorldGuardrails {
    fn default() -> Self {
        Self {
            target_hz: TARGET_HZ_120,
            frame_budget_ms: FRAME_BUDGET_120HZ_MS,
            max_dynamic_lights: MAX_DYNAMIC_LIGHTS_120HZ,
            max_draw_calls: 256,
        }
    }
}

/// Genre-agnostic environment lighting, atmosphere, and enclosure state.
#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentState {
    pub preset: String,
    pub time_hours: f32,
    pub weather: String,
    pub ambient_particles: bool,
    pub light_boost: f32,
    pub enclosure_open: bool,
}

impl Default for EnvironmentState {
    fn default() -> Self {
        Self {
            preset: "daylight".into(),
            time_hours: 15.5,
            weather: "clear".into(),
            ambient_particles: true,
            light_boost: 1.0,
            enclosure_open: false,
        }
    }
}

/// Genre-agnostic active viewport camera state.
#[derive(Clone, Debug, PartialEq)]
pub struct CameraState {
    pub preset: String,
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub fov_y_deg: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            preset: "default".into(),
            position: [0.0, 1.65, 3.2],
            target: [0.0, 1.0, 0.0],
            fov_y_deg: 50.0,
        }
    }
}

/// Named stage entity exposed to Tri-Modal Living Canvas inspection and safe cosmetic overrides.
#[derive(Clone, Debug, PartialEq)]
pub struct StageEntity {
    pub id: String,
    pub role: String,
    pub offset_metres: [f32; 3],
    pub yaw_deg: f32,
    pub visible: bool,
    pub style_preset: String,
    pub tint_rgba: [f32; 4],
    pub speed_scale: f32,
}

impl StageEntity {
    pub fn default_with(id: &str, role: &str, offset_metres: [f32; 3]) -> Self {
        Self {
            id: id.into(),
            role: role.into(),
            offset_metres,
            yaw_deg: 0.0,
            visible: true,
            style_preset: "default".into(),
            tint_rgba: [1.0, 1.0, 1.0, 1.0],
            speed_scale: 1.0,
        }
    }

    pub fn to_json_value(&self) -> Value {
        Value::Object(BTreeMap::from([
            ("id".into(), Value::String(self.id.clone())),
            ("role".into(), Value::String(self.role.clone())),
            (
                "offset_metres".into(),
                Value::Array(
                    self.offset_metres
                        .iter()
                        .map(|&v| Value::Number(f64::from(v)))
                        .collect(),
                ),
            ),
            ("yaw_deg".into(), Value::Number(f64::from(self.yaw_deg))),
            ("visible".into(), Value::Bool(self.visible)),
            (
                "style_preset".into(),
                Value::String(self.style_preset.clone()),
            ),
            (
                "tint_rgba".into(),
                Value::Array(
                    self.tint_rgba
                        .iter()
                        .map(|&v| Value::Number(f64::from(v)))
                        .collect(),
                ),
            ),
            (
                "speed_scale".into(),
                Value::Number(f64::from(self.speed_scale)),
            ),
        ]))
    }
}

/// `<400`-token canonical Semantic World Graph (`ganbate://context`).
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticWorldGraph {
    pub schema: String,
    pub uri: String,
    pub authority_mode: String,
    pub play_mode: PlayMode,
    pub guardrails: WorldGuardrails,
    pub environment: EnvironmentState,
    pub camera: CameraState,
    pub entities: Vec<StageEntity>,
}

impl Default for SemanticWorldGraph {
    fn default() -> Self {
        Self {
            schema: WORLD_GRAPH_SCHEMA.into(),
            uri: WORLD_GRAPH_URI.into(),
            authority_mode: AUTHORITY_MODE_READONLY.into(),
            play_mode: PlayMode::Play,
            guardrails: WorldGuardrails::default(),
            environment: EnvironmentState::default(),
            camera: CameraState::default(),
            entities: vec![
                StageEntity::default_with("primary_actor", "host", [0.0, 0.0, -1.2]),
                StageEntity::default_with("stage_center", "surface", [0.0, 0.0, 0.0]),
            ],
        }
    }
}

fn clamp_with_notice(
    val: &mut f32,
    min: f32,
    max: f32,
    fallback: f32,
    field: &str,
    notices: &mut Vec<String>,
) {
    if !val.is_finite() {
        *val = fallback;
        notices.push(format!("reset non-finite {field} to default {fallback:.2}"));
        return;
    }
    if *val < min || *val > max {
        let orig = *val;
        *val = orig.clamp(min, max);
        notices.push(format!(
            "clamped {field} from {orig:.2} to {:.2} (allowed {min:.2}..={max:.2})",
            *val
        ));
    }
}

fn sanitize_string_len(s: &mut String, fallback: &str, max_len: usize) {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        *s = fallback.into();
    } else if trimmed.len() > max_len {
        let mut end = max_len;
        while !trimmed.is_char_boundary(end) {
            end -= 1;
        }
        *s = trimmed[..end].to_string();
    } else if trimmed.len() != s.len() {
        *s = trimmed.to_string();
    }
}

impl SemanticWorldGraph {
    /// Validates and clamps all graph parameters to `120Hz` (`8.33ms`) and safe range guardrails,
    /// returning structured notices for every clamped field.
    pub fn validate_and_clamp(&mut self) -> Vec<String> {
        let mut notices = Vec::new();

        if self.schema != WORLD_GRAPH_SCHEMA {
            self.schema = WORLD_GRAPH_SCHEMA.into();
        }
        if self.uri != WORLD_GRAPH_URI {
            self.uri = WORLD_GRAPH_URI.into();
        }
        if self.authority_mode != AUTHORITY_MODE_READONLY {
            self.authority_mode = AUTHORITY_MODE_READONLY.into();
            notices.push(format!("enforced authority_mode={AUTHORITY_MODE_READONLY}"));
        }

        // 120Hz / 8.33ms performance budget guardrails.
        if self.guardrails.target_hz < TARGET_HZ_120 {
            let requested = self.guardrails.target_hz;
            self.guardrails.target_hz = TARGET_HZ_120;
            notices.push(format!(
                "enforced guardrails.target_hz=120Hz minimum (requested {requested}Hz)"
            ));
        } else if self.guardrails.target_hz > 240 {
            let requested = self.guardrails.target_hz;
            self.guardrails.target_hz = 240;
            notices.push(format!(
                "capped guardrails.target_hz from {requested}Hz to 240Hz"
            ));
        }

        if !self.guardrails.frame_budget_ms.is_finite() || self.guardrails.frame_budget_ms <= 0.0 {
            self.guardrails.frame_budget_ms = FRAME_BUDGET_120HZ_MS;
            notices.push("reset invalid guardrails.frame_budget_ms to 8.33ms".into());
        } else if self.guardrails.frame_budget_ms > FRAME_BUDGET_120HZ_MS {
            let requested = self.guardrails.frame_budget_ms;
            self.guardrails.frame_budget_ms = FRAME_BUDGET_120HZ_MS;
            notices.push(format!(
                "capped guardrails.frame_budget_ms from {requested:.2}ms to 8.33ms (120Hz budget)"
            ));
        } else if self.guardrails.frame_budget_ms < 4.0 {
            let requested = self.guardrails.frame_budget_ms;
            self.guardrails.frame_budget_ms = 4.0;
            notices.push(format!(
                "clamped guardrails.frame_budget_ms from {requested:.2}ms to 4.00ms"
            ));
        }

        if self.guardrails.max_dynamic_lights > MAX_DYNAMIC_LIGHTS_120HZ {
            let requested = self.guardrails.max_dynamic_lights;
            self.guardrails.max_dynamic_lights = MAX_DYNAMIC_LIGHTS_120HZ;
            notices.push(format!(
                "capped guardrails.max_dynamic_lights from {requested} to {MAX_DYNAMIC_LIGHTS_120HZ} (8.33ms 120Hz budget)"
            ));
        }

        if self.guardrails.max_draw_calls == 0 {
            self.guardrails.max_draw_calls = 1;
            notices.push("clamped guardrails.max_draw_calls from 0 to 1".into());
        } else if self.guardrails.max_draw_calls > MAX_DRAW_CALLS_120HZ {
            let requested = self.guardrails.max_draw_calls;
            self.guardrails.max_draw_calls = MAX_DRAW_CALLS_120HZ;
            notices.push(format!(
                "capped guardrails.max_draw_calls from {requested} to {MAX_DRAW_CALLS_120HZ} (8.33ms 120Hz budget)"
            ));
        }

        // Environment guardrails.
        sanitize_string_len(&mut self.environment.preset, "daylight", 32);
        sanitize_string_len(&mut self.environment.weather, "clear", 32);
        clamp_with_notice(
            &mut self.environment.time_hours,
            0.0,
            24.0,
            15.5,
            "environment.time_hours",
            &mut notices,
        );
        clamp_with_notice(
            &mut self.environment.light_boost,
            0.0,
            2.0,
            1.0,
            "environment.light_boost",
            &mut notices,
        );

        // Camera guardrails.
        sanitize_string_len(&mut self.camera.preset, "default", 32);
        for (idx, axis) in ["x", "y", "z"].iter().enumerate() {
            clamp_with_notice(
                &mut self.camera.position[idx],
                -500.0,
                500.0,
                CameraState::default().position[idx],
                &format!("camera.position.{axis}"),
                &mut notices,
            );
            clamp_with_notice(
                &mut self.camera.target[idx],
                -500.0,
                500.0,
                CameraState::default().target[idx],
                &format!("camera.target.{axis}"),
                &mut notices,
            );
        }
        let dx = self.camera.target[0] - self.camera.position[0];
        let dy = self.camera.target[1] - self.camera.position[1];
        let dz = self.camera.target[2] - self.camera.position[2];
        if dx * dx + dy * dy + dz * dz < 1e-6 {
            self.camera.position[2] += 1.0;
            notices.push("offset degenerate camera.position from camera.target".into());
        }
        clamp_with_notice(
            &mut self.camera.fov_y_deg,
            20.0,
            110.0,
            50.0,
            "camera.fov_y_deg",
            &mut notices,
        );

        // Stage entity guardrails.
        if self.entities.len() > MAX_STAGE_ENTITIES {
            let orig = self.entities.len();
            self.entities.truncate(MAX_STAGE_ENTITIES);
            notices.push(format!(
                "capped entities count from {orig} to {MAX_STAGE_ENTITIES} (<400-token budget)"
            ));
        }
        for ent in &mut self.entities {
            sanitize_string_len(&mut ent.id, "stage_entity", 40);
            sanitize_string_len(&mut ent.role, "prop", 32);
            sanitize_string_len(&mut ent.style_preset, "default", 32);
            let ent_id = ent.id.clone();
            for (idx, axis) in ["x", "y", "z"].iter().enumerate() {
                clamp_with_notice(
                    &mut ent.offset_metres[idx],
                    -100.0,
                    100.0,
                    0.0,
                    &format!("entities[{ent_id}].offset_metres.{axis}"),
                    &mut notices,
                );
            }
            clamp_with_notice(
                &mut ent.yaw_deg,
                -360.0,
                360.0,
                0.0,
                &format!("entities[{ent_id}].yaw_deg"),
                &mut notices,
            );
            for (idx, ch) in ["r", "g", "b", "a"].iter().enumerate() {
                clamp_with_notice(
                    &mut ent.tint_rgba[idx],
                    0.0,
                    1.0,
                    1.0,
                    &format!("entities[{ent_id}].tint_rgba.{ch}"),
                    &mut notices,
                );
            }
            clamp_with_notice(
                &mut ent.speed_scale,
                0.25,
                3.0,
                1.0,
                &format!("entities[{ent_id}].speed_scale"),
                &mut notices,
            );
        }

        notices
    }

    pub fn to_json_value(&self) -> Value {
        let guardrails = Value::Object(BTreeMap::from([
            (
                "target_hz".into(),
                Value::Number(f64::from(self.guardrails.target_hz)),
            ),
            (
                "frame_budget_ms".into(),
                Value::Number(f64::from(self.guardrails.frame_budget_ms)),
            ),
            (
                "max_dynamic_lights".into(),
                Value::Number(f64::from(self.guardrails.max_dynamic_lights)),
            ),
            (
                "max_draw_calls".into(),
                Value::Number(f64::from(self.guardrails.max_draw_calls)),
            ),
        ]));

        let environment = Value::Object(BTreeMap::from([
            (
                "preset".into(),
                Value::String(self.environment.preset.clone()),
            ),
            (
                "time_hours".into(),
                Value::Number(f64::from(self.environment.time_hours)),
            ),
            (
                "weather".into(),
                Value::String(self.environment.weather.clone()),
            ),
            (
                "ambient_particles".into(),
                Value::Bool(self.environment.ambient_particles),
            ),
            (
                "light_boost".into(),
                Value::Number(f64::from(self.environment.light_boost)),
            ),
            (
                "enclosure_open".into(),
                Value::Bool(self.environment.enclosure_open),
            ),
        ]));

        let camera = Value::Object(BTreeMap::from([
            ("preset".into(), Value::String(self.camera.preset.clone())),
            (
                "position".into(),
                Value::Array(
                    self.camera
                        .position
                        .iter()
                        .map(|&v| Value::Number(f64::from(v)))
                        .collect(),
                ),
            ),
            (
                "target".into(),
                Value::Array(
                    self.camera
                        .target
                        .iter()
                        .map(|&v| Value::Number(f64::from(v)))
                        .collect(),
                ),
            ),
            (
                "fov_y_deg".into(),
                Value::Number(f64::from(self.camera.fov_y_deg)),
            ),
        ]));

        let entities = Value::Array(
            self.entities
                .iter()
                .map(StageEntity::to_json_value)
                .collect(),
        );

        Value::Object(BTreeMap::from([
            ("schema".into(), Value::String(self.schema.clone())),
            ("uri".into(), Value::String(self.uri.clone())),
            (
                "authority_mode".into(),
                Value::String(self.authority_mode.clone()),
            ),
            (
                "play_mode".into(),
                Value::String(self.play_mode.as_str().into()),
            ),
            ("guardrails".into(), guardrails),
            ("environment".into(), environment),
            ("camera".into(), camera),
            ("entities".into(), entities),
        ]))
    }

    pub fn to_canonical_json(&self) -> Result<String, String> {
        json::to_canonical(&self.to_json_value())
    }

    pub fn to_compact_json(&self) -> Result<String, String> {
        json::to_compact(&self.to_json_value())
    }

    /// Estimated LLM token count (`~4 chars/token` on canonical JSON).
    pub fn estimated_tokens(&self) -> usize {
        self.to_canonical_json()
            .map(|s| s.len().div_ceil(4))
            .unwrap_or(0)
    }

    pub fn from_json_value(v: &Value) -> Result<(Self, Vec<String>), String> {
        if !matches!(v, Value::Object(_)) {
            return Err("SemanticWorldGraph expected a JSON object".into());
        }
        let mut graph = Self::default();
        let mut parse_notices = parse_merge_into(&mut graph, v)?;
        let clamp_notices = graph.validate_and_clamp();
        parse_notices.extend(clamp_notices);
        Ok((graph, parse_notices))
    }
}

fn read_f32_lossy(v: &Value, field: &str) -> Result<f32, String> {
    match v {
        Value::Number(n) => Ok(*n as f32),
        _ => Err(format!("{field}: expected number")),
    }
}

fn read_u32_lossy(v: &Value, field: &str) -> Result<u32, String> {
    match v {
        Value::Number(n) if n.is_finite() => {
            if *n <= 0.0 {
                Ok(0)
            } else if *n >= f64::from(u32::MAX) {
                Ok(u32::MAX)
            } else {
                Ok(n.round() as u32)
            }
        }
        _ => Err(format!("{field}: expected integer")),
    }
}

fn read_f32_arr<const N: usize>(v: &Value, field: &str) -> Result<[f32; N], String> {
    let arr = v
        .array()
        .map_err(|_| format!("{field}: expected array of {N} numbers"))?;
    if arr.len() != N {
        return Err(format!("{field}: expected {N} numbers, got {}", arr.len()));
    }
    let mut out = [0.0f32; N];
    for (i, item) in arr.iter().enumerate() {
        out[i] = read_f32_lossy(item, field)?;
    }
    Ok(out)
}

fn apply_entity_patch(ent: &mut StageEntity, v: &Value) -> Result<(), String> {
    if !matches!(v, Value::Object(_)) {
        return Err("entity entry must be a JSON object".into());
    }
    if let Some(id) = v.get("id") {
        ent.id = id.string()?.to_string();
    }
    if let Some(role) = v.get("role") {
        ent.role = role.string()?.to_string();
    }
    if let Some(off) = v.get("offset_metres") {
        ent.offset_metres = read_f32_arr::<3>(off, "offset_metres")?;
    }
    if let Some(yaw) = v.get("yaw_deg") {
        ent.yaw_deg = read_f32_lossy(yaw, "yaw_deg")?;
    }
    if let Some(vis) = v.get("visible") {
        match vis {
            Value::Bool(b) => ent.visible = *b,
            _ => return Err("visible: expected boolean".into()),
        }
    }
    if let Some(sp) = v.get("style_preset") {
        ent.style_preset = sp.string()?.to_string();
    }
    if let Some(tint) = v.get("tint_rgba") {
        ent.tint_rgba = read_f32_arr::<4>(tint, "tint_rgba")?;
    }
    if let Some(spd) = v.get("speed_scale") {
        ent.speed_scale = read_f32_lossy(spd, "speed_scale")?;
    }
    Ok(())
}

fn parse_merge_into(graph: &mut SemanticWorldGraph, patch: &Value) -> Result<Vec<String>, String> {
    let notices = Vec::new();
    let Value::Object(map) = patch else {
        return Err("world graph patch must be a JSON object".into());
    };

    if let Some(pm) = map.get("play_mode") {
        let s = pm.string().map_err(|_| "play_mode: expected string")?;
        graph.play_mode = PlayMode::parse(s)
            .ok_or_else(|| format!("play_mode: expected \"play\" or \"edit\", got {s:?}"))?;
    }

    if let Some(g) = map.get("guardrails") {
        if !matches!(g, Value::Object(_)) {
            return Err("guardrails: expected object".into());
        }
        if let Some(hz) = g.get("target_hz") {
            graph.guardrails.target_hz = read_u32_lossy(hz, "guardrails.target_hz")?;
        }
        if let Some(fb) = g.get("frame_budget_ms") {
            graph.guardrails.frame_budget_ms = read_f32_lossy(fb, "guardrails.frame_budget_ms")?;
        }
        if let Some(ml) = g.get("max_dynamic_lights") {
            graph.guardrails.max_dynamic_lights =
                read_u32_lossy(ml, "guardrails.max_dynamic_lights")?;
        }
        if let Some(md) = g.get("max_draw_calls") {
            graph.guardrails.max_draw_calls = read_u32_lossy(md, "guardrails.max_draw_calls")?;
        }
    }

    if let Some(env) = map.get("environment") {
        if !matches!(env, Value::Object(_)) {
            return Err("environment: expected object".into());
        }
        if let Some(pr) = env.get("preset") {
            graph.environment.preset = pr.string()?.to_string();
        }
        if let Some(th) = env.get("time_hours") {
            graph.environment.time_hours = read_f32_lossy(th, "environment.time_hours")?;
        }
        if let Some(wt) = env.get("weather") {
            graph.environment.weather = wt.string()?.to_string();
        }
        if let Some(ap) = env.get("ambient_particles") {
            match ap {
                Value::Bool(b) => graph.environment.ambient_particles = *b,
                _ => return Err("environment.ambient_particles: expected boolean".into()),
            }
        }
        if let Some(lb) = env.get("light_boost") {
            graph.environment.light_boost = read_f32_lossy(lb, "environment.light_boost")?;
        }
        if let Some(eo) = env.get("enclosure_open") {
            match eo {
                Value::Bool(b) => graph.environment.enclosure_open = *b,
                _ => return Err("environment.enclosure_open: expected boolean".into()),
            }
        }
    }

    if let Some(cam) = map.get("camera") {
        if !matches!(cam, Value::Object(_)) {
            return Err("camera: expected object".into());
        }
        if let Some(pr) = cam.get("preset") {
            graph.camera.preset = pr.string()?.to_string();
        }
        if let Some(pos) = cam.get("position") {
            graph.camera.position = read_f32_arr::<3>(pos, "camera.position")?;
        }
        if let Some(tgt) = cam.get("target") {
            graph.camera.target = read_f32_arr::<3>(tgt, "camera.target")?;
        }
        if let Some(fov) = cam.get("fov_y_deg") {
            graph.camera.fov_y_deg = read_f32_lossy(fov, "camera.fov_y_deg")?;
        }
    }

    if let Some(ents) = map.get("entities") {
        match ents {
            Value::Array(arr) => {
                // When loading a full document (has "schema"), replace entities list;
                // when applying a partial patch, upsert by id.
                if map.contains_key("schema") {
                    let mut parsed = Vec::with_capacity(arr.len());
                    for item in arr {
                        let mut ent = StageEntity::default_with("stage_entity", "prop", [0.0; 3]);
                        apply_entity_patch(&mut ent, item)?;
                        parsed.push(ent);
                    }
                    graph.entities = parsed;
                } else {
                    for item in arr {
                        let id = item
                            .get("id")
                            .and_then(|v| v.string().ok())
                            .ok_or("entities patch array element requires 'id'")?;
                        if let Some(existing) = graph.entities.iter_mut().find(|e| e.id == id) {
                            apply_entity_patch(existing, item)?;
                        } else {
                            let mut fresh = StageEntity::default_with(id, "prop", [0.0; 3]);
                            apply_entity_patch(&mut fresh, item)?;
                            graph.entities.push(fresh);
                        }
                    }
                }
            }
            Value::Object(ent_map) => {
                for (key_id, item) in ent_map {
                    if let Some(existing) = graph.entities.iter_mut().find(|e| e.id == *key_id) {
                        apply_entity_patch(existing, item)?;
                    } else {
                        let mut fresh = StageEntity::default_with(key_id, "prop", [0.0; 3]);
                        apply_entity_patch(&mut fresh, item)?;
                        fresh.id = key_id.clone();
                        graph.entities.push(fresh);
                    }
                }
            }
            _ => return Err("entities: expected array or object map".into()),
        }
    }

    Ok(notices)
}

const FORBIDDEN_AUTHORITY_KEYS: &[&str] = &[
    "rng",
    "rng_seed",
    "random_seed",
    "seed",
    "seeds",
    "prng",
    "entropy",
    "secret",
    "secrets",
    "secret_state",
    "hidden_state",
    "private_state",
    "server_state",
    "balance",
    "balances",
    "account_balance",
    "ledger",
    "ledgers",
    "wallet",
    "wallets",
    "credits",
    "funds",
    "disbursement",
    "cheat",
    "rig",
    "rigged",
    "tamper",
    "outcome",
    "outcomes",
    "winner",
    "force_win",
    "override_authority",
    "bypass_authority",
];

fn normalize_key(k: &str) -> String {
    let mut out = String::with_capacity(k.len() + 4);
    for (i, ch) in k.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == ' ' {
            out.push('_');
        } else {
            out.push(ch.to_ascii_lowercase());
        }
    }
    out
}

/// Inspects a JSON patch recursively for any attempt to mutate authoritative server state.
pub fn detect_authority_violation_patch(patch: &Value) -> Option<String> {
    fn walk(v: &Value, path: &str) -> Option<String> {
        match v {
            Value::Object(map) => {
                for (k, child) in map {
                    let nk = normalize_key(k);
                    let child_path = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    if nk == "authority_mode"
                        && child.string().ok() != Some(AUTHORITY_MODE_READONLY)
                    {
                        return Some(format!(
                            "blocked by {AUTHORITY_MODE_READONLY} policy: cannot weaken or override authority_mode"
                        ));
                    }
                    if FORBIDDEN_AUTHORITY_KEYS.contains(&nk.as_str())
                        || nk.split('_').any(|seg| {
                            matches!(
                                seg,
                                "rng"
                                    | "seed"
                                    | "prng"
                                    | "entropy"
                                    | "secret"
                                    | "ledger"
                                    | "balance"
                                    | "balances"
                                    | "wallet"
                                    | "cheat"
                                    | "rig"
                                    | "rigged"
                                    | "tamper"
                                    | "outcome"
                                    | "outcomes"
                            )
                        })
                    {
                        return Some(format!(
                            "blocked by {AUTHORITY_MODE_READONLY} policy: patch cannot mutate authoritative server state at '{child_path}'"
                        ));
                    }
                    if let Some(reason) = walk(child, &child_path) {
                        return Some(reason);
                    }
                }
                None
            }
            Value::Array(arr) => {
                for (idx, item) in arr.iter().enumerate() {
                    let child_path = format!("{path}[{idx}]");
                    if let Some(reason) = walk(item, &child_path) {
                        return Some(reason);
                    }
                }
                None
            }
            _ => None,
        }
    }
    walk(patch, "")
}

fn ascii_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            cur.push(ch.to_ascii_lowercase());
        } else if !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

/// Inspects a natural-language prompt (EN/KO) for attempts to mutate authoritative server state.
pub fn detect_authority_violation_prompt(prompt: &str) -> Option<String> {
    let words = ascii_words(prompt);
    let has_word = |w: &str| words.iter().any(|tok| tok == w);
    let has_pair = |a: &str, b: &str| words.windows(2).any(|w| w[0] == a && w[1] == b);

    // 1. RNG / Seed tampering (EN + KO)
    if has_word("rng")
        || has_word("prng")
        || has_word("seed")
        || has_word("seeds")
        || has_word("entropy")
        || prompt.contains("난수")
        || prompt.contains("시드")
    {
        return Some(format!(
            "blocked by {AUTHORITY_MODE_READONLY} policy: authoritative RNG and seed state cannot be modified"
        ));
    }

    // 2. Secret / hidden server state (EN + KO)
    if has_word("secret")
        || has_pair("hidden", "state")
        || has_pair("private", "state")
        || has_pair("server", "state")
        || has_pair("leak", "state")
        || prompt.contains("비밀 상태")
        || prompt.contains("숨겨진 상태")
        || prompt.contains("비공개 상태")
        || prompt.contains("숨긴 정보")
    {
        return Some(format!(
            "blocked by {AUTHORITY_MODE_READONLY} policy: authoritative secret server state is read-only and unexposed"
        ));
    }

    // 3. Balances / Ledgers / Accounts (EN + KO)
    if has_word("balance")
        || has_word("balances")
        || has_word("ledger")
        || has_word("ledgers")
        || has_word("wallet")
        || has_word("wallets")
        || has_pair("add", "credits")
        || has_pair("set", "credits")
        || has_pair("infinite", "credits")
        || has_pair("infinite", "money")
        || has_pair("modify", "funds")
        || prompt.contains("잔액")
        || prompt.contains("원장")
        || prompt.contains("지갑")
        || prompt.contains("무한 재화")
        || prompt.contains("포인트 조작")
    {
        return Some(format!(
            "blocked by {AUTHORITY_MODE_READONLY} policy: authoritative account balances and ledgers cannot be modified"
        ));
    }

    // 4. Cheat / Rig / Outcome tampering / Authority bypass (EN + KO)
    if has_word("cheat")
        || has_word("cheats")
        || has_word("cheating")
        || has_word("rig")
        || has_word("rigged")
        || has_word("rigging")
        || has_word("tamper")
        || has_word("outcome")
        || has_word("outcomes")
        || has_pair("force", "win")
        || has_pair("guaranteed", "win")
        || has_pair("set", "winner")
        || has_pair("override", "authority")
        || has_pair("bypass", "authority")
        || has_pair("disable", "authority")
        || has_pair("client", "authoritative")
        || prompt.contains("치트")
        || prompt.contains("승부 조작")
        || prompt.contains("승부조작")
        || prompt.contains("결과 조작")
        || prompt.contains("결과조작")
        || prompt.contains("확률 조작")
        || prompt.contains("확률조작")
        || prompt.contains("승패 조작")
        || prompt.contains("강제 승리")
        || prompt.contains("무조건 승리")
        || prompt.contains("무조건 이기")
        || prompt.contains("권한 해제")
    {
        return Some(format!(
            "blocked by {AUTHORITY_MODE_READONLY} policy: authoritative outcomes and security rules cannot be rigged or bypassed"
        ));
    }

    None
}

/// Structured result of applying a JSON patch or compiled natural-language prompt to [`SemanticWorldGraph`].
#[derive(Clone, Debug)]
pub struct WorldGraphMutationOutcome {
    pub applied: bool,
    pub blocked_by_authority: bool,
    pub guardrail_notices: Vec<String>,
    pub patch: Value,
    pub graph: SemanticWorldGraph,
    pub broadcast_event: Option<Value>,
}

impl WorldGraphMutationOutcome {
    /// Materializes the [`Outbox`] messages for all connected Living Canvas clients.
    pub fn build_outbox(&self, recipients: impl IntoIterator<Item = ClientId>) -> Outbox {
        let Some(ref ev) = self.broadcast_event else {
            return Vec::new();
        };
        recipients
            .into_iter()
            .map(|cid| (cid, ev.clone()))
            .collect()
    }

    /// Structured JSON payload returned by MCP tools (`world_graph_patch` / `world_graph_prompt`)
    /// and Studio WebSocket handlers.
    pub fn to_json_value(&self) -> Value {
        let notices_arr = Value::Array(
            self.guardrail_notices
                .iter()
                .map(|n| Value::String(n.clone()))
                .collect(),
        );
        Value::Object(BTreeMap::from([
            ("ok".into(), Value::Bool(!self.blocked_by_authority)),
            ("uri".into(), Value::String(WORLD_GRAPH_URI.into())),
            ("applied".into(), Value::Bool(self.applied)),
            (
                "blocked_by_authority".into(),
                Value::Bool(self.blocked_by_authority),
            ),
            ("guardrail_notices".into(), notices_arr),
            ("patch".into(), self.patch.clone()),
            (
                "estimated_tokens".into(),
                Value::Number(self.graph.estimated_tokens() as f64),
            ),
            ("graph".into(), self.graph.to_json_value()),
        ]))
    }
}

fn make_broadcast_event(graph: &SemanticWorldGraph, patch: &Value, notices: &[String]) -> Value {
    Value::Object(BTreeMap::from([
        ("type".into(), Value::String("world-graph-updated".into())),
        ("uri".into(), Value::String(WORLD_GRAPH_URI.into())),
        ("patch".into(), patch.clone()),
        (
            "guardrail_notices".into(),
            Value::Array(notices.iter().map(|n| Value::String(n.clone())).collect()),
        ),
        ("graph".into(), graph.to_json_value()),
    ]))
}

/// Applies a JSON patch to `current` under `120Hz` (`8.33ms`) and `server-authoritative-readonly`
/// guardrails without mutating `current` in place.
pub fn apply_world_graph_patch(
    current: &SemanticWorldGraph,
    patch: &Value,
) -> Result<WorldGraphMutationOutcome, String> {
    if !matches!(patch, Value::Object(_)) {
        return Err("world graph patch expected a JSON object".into());
    }
    if let Some(reason) = detect_authority_violation_patch(patch) {
        return Ok(WorldGraphMutationOutcome {
            applied: false,
            blocked_by_authority: true,
            guardrail_notices: vec![reason],
            patch: patch.clone(),
            graph: current.clone(),
            broadcast_event: None,
        });
    }

    let mut next = current.clone();
    let mut notices = parse_merge_into(&mut next, patch)?;
    notices.extend(next.validate_and_clamp());

    let empty_patch = matches!(patch, Value::Object(m) if m.is_empty());
    let applied = !empty_patch && (next != *current || !notices.is_empty());
    let broadcast_event = applied.then(|| make_broadcast_event(&next, patch, &notices));

    Ok(WorldGraphMutationOutcome {
        applied,
        blocked_by_authority: false,
        guardrail_notices: notices,
        patch: patch.clone(),
        graph: next,
        broadcast_event,
    })
}

/// Extracts the first floating-point number appearing after any of `prefixes` in `text`.
fn extract_number_after(text: &str, prefixes: &[&str]) -> Option<f32> {
    let lower = text.to_lowercase();
    for prefix in prefixes {
        let mut search_from = 0usize;
        while let Some(rel_idx) = lower[search_from..].find(prefix) {
            let after_idx = search_from + rel_idx + prefix.len();
            let rest = &lower[after_idx..];
            let trimmed = rest.trim_start_matches([' ', ':', '=', '\t']);
            let mut num_str = String::new();
            for (i, ch) in trimmed.char_indices() {
                if ch.is_ascii_digit() || ch == '.' || (i == 0 && (ch == '-' || ch == '+')) {
                    num_str.push(ch);
                } else if ch == ':' && !num_str.is_empty() && !num_str.contains('.') {
                    // Support HH:MM time notation like "18:30".
                    let mins_part = &trimmed[i + 1..];
                    let mins_digits: String = mins_part
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    if let (Ok(h), Ok(m)) = (num_str.parse::<f32>(), mins_digits.parse::<f32>()) {
                        return Some(h + (m / 60.0));
                    }
                    break;
                } else {
                    break;
                }
            }
            if let Ok(val) = num_str.parse::<f32>() {
                return Some(val);
            }
            search_from = after_idx;
        }
    }
    None
}

/// Extracts `<number>x` or `<number>배속` speed multipliers from `text`.
fn extract_multiplier(text: &str) -> Option<f32> {
    let lower = text.to_lowercase();
    for suffix in ["배속", "x"] {
        let mut end_search = 0usize;
        while let Some(rel) = lower[end_search..].find(suffix) {
            let pos = end_search + rel;
            let after = &lower[pos + suffix.len()..];
            // Ensure `x` is not inside a word like `max` or `next` or `texture`.
            let boundary_after = after
                .chars()
                .next()
                .is_none_or(|c| !c.is_ascii_alphabetic());
            if boundary_after && pos > 0 {
                let before = &lower[..pos];
                let rev_digits: String = before
                    .chars()
                    .rev()
                    .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                    .collect();
                if !rev_digits.is_empty() && rev_digits.chars().any(|c| c.is_ascii_digit()) {
                    let forward: String = rev_digits.chars().rev().collect();
                    if let Ok(v) = forward.parse::<f32>() {
                        return Some(v);
                    }
                }
            }
            end_search = pos + suffix.len();
        }
    }
    extract_number_after(text, &["speed_scale", "speed", "속도", "배속"])
}

/// Deterministically compiles a genre-agnostic bilingual (EN/KO) natural-language prompt into a
/// validated [`SemanticWorldGraph`] patch and [`Outbox`] broadcast payloads.
pub fn compile_world_graph_prompt(
    current: &SemanticWorldGraph,
    prompt: &str,
    recipients: &[ClientId],
) -> (WorldGraphMutationOutcome, Outbox) {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return (
            WorldGraphMutationOutcome {
                applied: false,
                blocked_by_authority: false,
                guardrail_notices: vec![
                    "empty prompt: no world graph directives recognized".into(),
                ],
                patch: Value::Object(BTreeMap::new()),
                graph: current.clone(),
                broadcast_event: None,
            },
            Vec::new(),
        );
    }

    if let Some(reason) = detect_authority_violation_prompt(trimmed) {
        return (
            WorldGraphMutationOutcome {
                applied: false,
                blocked_by_authority: true,
                guardrail_notices: vec![reason],
                patch: Value::Object(BTreeMap::new()),
                graph: current.clone(),
                broadcast_event: None,
            },
            Vec::new(),
        );
    }

    let lower = trimmed.to_lowercase();
    let words = ascii_words(trimmed);
    let has_word = |w: &str| words.iter().any(|tok| tok == w);
    let has_any_word = |ws: &[&str]| ws.iter().any(|w| has_word(w));
    let contains_any = |subs: &[&str]| subs.iter().any(|s| lower.contains(s));

    let mut patch_map: BTreeMap<String, Value> = BTreeMap::new();
    let mut env_map: BTreeMap<String, Value> = BTreeMap::new();
    let mut cam_map: BTreeMap<String, Value> = BTreeMap::new();
    let mut guard_map: BTreeMap<String, Value> = BTreeMap::new();
    let mut ent_patches: Vec<Value> = Vec::new();

    // 1. Living Canvas Play <-> Edit state
    if has_any_word(&["pause", "paused", "freeze", "edit"])
        || contains_any(&["편집 모드", "에디트 모드", "일시정지", "멈춰", "정지"])
    {
        patch_map.insert("play_mode".into(), Value::String("edit".into()));
    } else if has_any_word(&["resume", "unpause"])
        || contains_any(&[
            "play mode",
            "live mode",
            "플레이 모드",
            "실행 모드",
            "다시 시작",
            "재생",
        ])
    {
        patch_map.insert("play_mode".into(), Value::String("play".into()));
    }

    // 2. Environment lighting presets + time of day
    if has_any_word(&["sunset", "dusk"])
        || contains_any(&["golden hour", "노을", "석양", "황혼", "해질녘"])
    {
        env_map.insert("preset".into(), Value::String("sunset".into()));
        env_map.insert("time_hours".into(), Value::Number(18.25));
    } else if has_any_word(&["night", "midnight", "moonlight"])
        || contains_any(&["밤", "야간", "자정", "달빛"])
    {
        env_map.insert("preset".into(), Value::String("night".into()));
        env_map.insert("time_hours".into(), Value::Number(22.5));
    } else if has_any_word(&["dawn", "sunrise", "morning"])
        || contains_any(&["새벽", "일출", "아침"])
    {
        env_map.insert("preset".into(), Value::String("dawn".into()));
        env_map.insert("time_hours".into(), Value::Number(6.5));
    } else if has_any_word(&["daylight", "noon", "midday", "sunny"])
        || contains_any(&["정오", "한낮", "낮 조명", "맑은 낮"])
    {
        env_map.insert("preset".into(), Value::String("daylight".into()));
        env_map.insert("time_hours".into(), Value::Number(13.0));
    } else if contains_any(&[
        "studio light",
        "studio preset",
        "neutral light",
        "스튜디오 조명",
        "기본 조명",
    ]) {
        env_map.insert("preset".into(), Value::String("studio".into()));
        env_map.insert("time_hours".into(), Value::Number(14.0));
    }

    if let Some(explicit_hours) =
        extract_number_after(trimmed, &["time_hours", "time to", "time", "시간"])
    {
        env_map.insert(
            "time_hours".into(),
            Value::Number(f64::from(explicit_hours)),
        );
    }

    // 3. Light boost / brightness
    if let Some(boost) = extract_number_after(
        trimmed,
        &["light_boost", "light boost", "brightness", "밝기"],
    ) {
        env_map.insert("light_boost".into(), Value::Number(f64::from(boost)));
    } else if has_any_word(&["brighter", "brighten"])
        || contains_any(&["더 밝게", "조명 밝게", "밝기 올려"])
    {
        env_map.insert(
            "light_boost".into(),
            Value::Number(f64::from(current.environment.light_boost + 0.35)),
        );
    } else if has_any_word(&["dimmer", "darken", "darker"])
        || contains_any(&["어둡게", "조명 낮춰", "밝기 줄여"])
    {
        env_map.insert(
            "light_boost".into(),
            Value::Number(f64::from(current.environment.light_boost - 0.35)),
        );
    }

    // 4. Enclosure / ceiling / roof / skylight open & close
    let mentions_enclosure = has_any_word(&["ceiling", "roof", "skylight", "enclosure", "canopy"])
        || contains_any(&["천장", "지붕", "루프", "개방"]);
    if mentions_enclosure {
        if has_any_word(&["close", "closed", "shut"]) || contains_any(&["닫아", "닫고", "폐쇄"])
        {
            env_map.insert("enclosure_open".into(), Value::Bool(false));
        } else if has_any_word(&["open", "retract", "unlock"])
            || contains_any(&["열어", "열고", "개방"])
        {
            env_map.insert("enclosure_open".into(), Value::Bool(true));
        }
    }

    // 5. Weather & ambient particles
    if has_any_word(&["rain", "rainy", "storm"]) || contains_any(&["비 내리", "우천", "빗방울"])
    {
        env_map.insert("weather".into(), Value::String("rain".into()));
        env_map.insert("ambient_particles".into(), Value::Bool(true));
    } else if has_any_word(&["snow", "snowy"]) || contains_any(&["눈 내리", "설경", "함박눈"])
    {
        env_map.insert("weather".into(), Value::String("snow".into()));
        env_map.insert("ambient_particles".into(), Value::Bool(true));
    } else if has_any_word(&["mist", "fog", "foggy"]) || contains_any(&["안개"]) {
        env_map.insert("weather".into(), Value::String("mist".into()));
    } else if has_any_word(&["overcast", "cloudy"]) || contains_any(&["흐림", "구름"]) {
        env_map.insert("weather".into(), Value::String("overcast".into()));
    } else if has_any_word(&["petals", "blossom"]) || contains_any(&["꽃잎", "벚꽃"]) {
        env_map.insert("weather".into(), Value::String("petals".into()));
        env_map.insert("ambient_particles".into(), Value::Bool(true));
    } else if contains_any(&["clear weather", "clear sky", "맑은 날씨", "맑게"]) {
        env_map.insert("weather".into(), Value::String("clear".into()));
    }

    if contains_any(&[
        "disable particles",
        "particles off",
        "no particles",
        "hide particles",
        "파티클 꺼",
        "파티클 끄",
        "입자 꺼",
        "파티클 비활성",
    ]) {
        env_map.insert("ambient_particles".into(), Value::Bool(false));
    } else if contains_any(&[
        "enable particles",
        "particles on",
        "show particles",
        "파티클 켜",
        "입자 켜",
        "파티클 활성",
    ]) {
        env_map.insert("ambient_particles".into(), Value::Bool(true));
    }

    // 6. Camera presets & FOV
    if contains_any(&[
        "reset camera",
        "camera reset",
        "default camera",
        "카메라 초기화",
        "카메라 리셋",
        "기본 카메라",
    ]) {
        let def_cam = CameraState::default();
        cam_map.insert("preset".into(), Value::String(def_cam.preset));
        cam_map.insert(
            "position".into(),
            Value::Array(
                def_cam
                    .position
                    .iter()
                    .map(|&v| Value::Number(f64::from(v)))
                    .collect(),
            ),
        );
        cam_map.insert(
            "target".into(),
            Value::Array(
                def_cam
                    .target
                    .iter()
                    .map(|&v| Value::Number(f64::from(v)))
                    .collect(),
            ),
        );
        cam_map.insert(
            "fov_y_deg".into(),
            Value::Number(f64::from(def_cam.fov_y_deg)),
        );
    } else if has_word("closeup")
        || contains_any(&["close-up", "zoom in", "클로즈업", "카메라 확대"])
    {
        cam_map.insert("preset".into(), Value::String("closeup".into()));
        cam_map.insert(
            "position".into(),
            Value::Array(vec![
                Value::Number(0.0),
                Value::Number(1.45),
                Value::Number(1.8),
            ]),
        );
        cam_map.insert(
            "target".into(),
            Value::Array(vec![
                Value::Number(0.0),
                Value::Number(1.15),
                Value::Number(-0.4),
            ]),
        );
        cam_map.insert("fov_y_deg".into(), Value::Number(38.0));
    } else if contains_any(&[
        "wide camera",
        "wide shot",
        "wide angle",
        "zoom out",
        "와이드",
        "카메라 축소",
    ]) {
        cam_map.insert("preset".into(), Value::String("wide".into()));
        cam_map.insert(
            "position".into(),
            Value::Array(vec![
                Value::Number(0.0),
                Value::Number(2.2),
                Value::Number(5.0),
            ]),
        );
        cam_map.insert(
            "target".into(),
            Value::Array(vec![
                Value::Number(0.0),
                Value::Number(0.9),
                Value::Number(0.0),
            ]),
        );
        cam_map.insert("fov_y_deg".into(), Value::Number(68.0));
    } else if contains_any(&[
        "overhead camera",
        "top-down",
        "top down",
        "탑뷰",
        "위에서 내려다",
    ]) {
        cam_map.insert("preset".into(), Value::String("overhead".into()));
        cam_map.insert(
            "position".into(),
            Value::Array(vec![
                Value::Number(0.0),
                Value::Number(4.5),
                Value::Number(0.8),
            ]),
        );
        cam_map.insert(
            "target".into(),
            Value::Array(vec![
                Value::Number(0.0),
                Value::Number(0.5),
                Value::Number(0.0),
            ]),
        );
        cam_map.insert("fov_y_deg".into(), Value::Number(55.0));
    }

    if let Some(fov) = extract_number_after(trimmed, &["fov_y_deg", "fov", "시야각"]) {
        cam_map.insert("fov_y_deg".into(), Value::Number(f64::from(fov)));
    }

    // 7. Guardrail budget requests (e.g. "max lights 24", capped to 120Hz limits by validator)
    if let Some(lights) = extract_number_after(
        trimmed,
        &[
            "max_dynamic_lights",
            "max lights",
            "dynamic lights",
            "동적 조명",
        ],
    ) {
        guard_map.insert(
            "max_dynamic_lights".into(),
            Value::Number(f64::from(lights.max(0.0).round())),
        );
    }
    if let Some(draws) =
        extract_number_after(trimmed, &["max_draw_calls", "draw calls", "드로우콜"])
    {
        guard_map.insert(
            "max_draw_calls".into(),
            Value::Number(f64::from(draws.max(0.0).round())),
        );
    }

    // 8. Stage entity speed_scale & visibility
    let speed_opt = extract_multiplier(trimmed).or_else(|| {
        if contains_any(&["slow motion", "슬로우 모션", "느리게"]) {
            Some(0.5)
        } else if contains_any(&["normal speed", "기본 속도"]) {
            Some(1.0)
        } else {
            None
        }
    });

    if let Some(spd) = speed_opt {
        let targeted: Vec<&StageEntity> = current
            .entities
            .iter()
            .filter(|e| {
                lower.contains(&e.id.to_lowercase()) || lower.contains(&e.role.to_lowercase())
            })
            .collect();
        let targets: Vec<&StageEntity> = if targeted.is_empty() {
            current.entities.iter().collect()
        } else {
            targeted
        };
        for ent in targets {
            ent_patches.push(Value::Object(BTreeMap::from([
                ("id".into(), Value::String(ent.id.clone())),
                ("speed_scale".into(), Value::Number(f64::from(spd))),
            ])));
        }
    }

    for ent in &current.entities {
        let id_low = ent.id.to_lowercase();
        if lower.contains(&id_low) {
            if has_word("hide") || contains_any(&["숨겨", "숨기", "비표시"]) {
                ent_patches.push(Value::Object(BTreeMap::from([
                    ("id".into(), Value::String(ent.id.clone())),
                    ("visible".into(), Value::Bool(false)),
                ])));
            } else if has_word("show") || contains_any(&["보여", "표시"]) {
                ent_patches.push(Value::Object(BTreeMap::from([
                    ("id".into(), Value::String(ent.id.clone())),
                    ("visible".into(), Value::Bool(true)),
                ])));
            }
        }
    }

    if !guard_map.is_empty() {
        patch_map.insert("guardrails".into(), Value::Object(guard_map));
    }
    if !env_map.is_empty() {
        patch_map.insert("environment".into(), Value::Object(env_map));
    }
    if !cam_map.is_empty() {
        patch_map.insert("camera".into(), Value::Object(cam_map));
    }
    if !ent_patches.is_empty() {
        patch_map.insert("entities".into(), Value::Array(ent_patches));
    }

    if patch_map.is_empty() {
        return (
            WorldGraphMutationOutcome {
                applied: false,
                blocked_by_authority: false,
                guardrail_notices: vec![
                    "no matching world graph directives recognized in prompt".into(),
                ],
                patch: Value::Object(BTreeMap::new()),
                graph: current.clone(),
                broadcast_event: None,
            },
            Vec::new(),
        );
    }

    let patch = Value::Object(patch_map);
    let outcome = match apply_world_graph_patch(current, &patch) {
        Ok(o) => o,
        Err(e) => WorldGraphMutationOutcome {
            applied: false,
            blocked_by_authority: false,
            guardrail_notices: vec![e],
            patch,
            graph: current.clone(),
            broadcast_event: None,
        },
    };
    let outbox = outcome.build_outbox(recipients.iter().copied());
    (outcome, outbox)
}

/// Reads `<project_root>/world.semantic.json`, creating it on disk with clean `<400`-token
/// `120Hz` defaults on first read if absent.
pub fn load_or_init_world_graph(
    project_root: &Path,
) -> Result<(SemanticWorldGraph, Vec<String>), String> {
    let path = project_root.join(WORLD_GRAPH_REL_PATH);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let parsed = json::parse(&bytes).map_err(|e| format!("{WORLD_GRAPH_REL_PATH}: {e}"))?;
            SemanticWorldGraph::from_json_value(&parsed)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut graph = SemanticWorldGraph::default();
            let notices = graph.validate_and_clamp();
            let _ = save_world_graph(project_root, &graph)?;
            Ok((graph, notices))
        }
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Writes `graph` canonically to `<project_root>/world.semantic.json`.
pub fn save_world_graph(project_root: &Path, graph: &SemanticWorldGraph) -> Result<String, String> {
    let mut clean = graph.clone();
    let _ = clean.validate_and_clamp();
    let text = clean.to_canonical_json()?;
    let path = project_root.join(WORLD_GRAPH_REL_PATH);
    ganbate_assets::write_asset(&path, text.as_bytes())
        .map_err(|e| format!("{WORLD_GRAPH_REL_PATH}: {e}"))?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ganbate_core::id::Ids;

    struct TempDir(std::path::PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn default_world_graph_persists_under_400_tokens_and_roundtrips_json() {
        let dir =
            TempDir(std::env::temp_dir().join(format!("ganbate-wg-{}", Ids::from_os().entity())));
        std::fs::create_dir_all(&dir.0).unwrap();

        let (graph, notices) = load_or_init_world_graph(&dir.0).unwrap();
        assert!(notices.is_empty(), "{notices:?}");
        assert!(dir.0.join(WORLD_GRAPH_REL_PATH).is_file());
        assert_eq!(graph.schema, WORLD_GRAPH_SCHEMA);
        assert_eq!(graph.uri, WORLD_GRAPH_URI);
        assert_eq!(graph.authority_mode, AUTHORITY_MODE_READONLY);
        assert_eq!(graph.play_mode, PlayMode::Play);
        assert_eq!(graph.guardrails.target_hz, 120);
        assert!((graph.guardrails.frame_budget_ms - 8.33).abs() < 1e-5);
        assert!(
            graph.estimated_tokens() < 400,
            "expected <400 tokens, got {}",
            graph.estimated_tokens()
        );

        let canonical = graph.to_canonical_json().unwrap();
        let reparsed_val = json::parse(canonical.as_bytes()).unwrap();
        let (decoded, dec_notices) = SemanticWorldGraph::from_json_value(&reparsed_val).unwrap();
        assert!(dec_notices.is_empty());
        assert_eq!(decoded, graph);
    }

    #[test]
    fn guardrail_validator_clamps_ranges_and_caps_120hz_budgets_with_notices() {
        let base = SemanticWorldGraph::default();
        let patch = json::parse(
            br#"{
                "guardrails": {
                    "target_hz": 60,
                    "frame_budget_ms": 16.67,
                    "max_dynamic_lights": 32,
                    "max_draw_calls": 4096
                },
                "environment": {
                    "time_hours": 29.5,
                    "light_boost": 4.2
                },
                "camera": {
                    "fov_y_deg": 145.0
                },
                "entities": [
                    {
                        "id": "primary_actor",
                        "speed_scale": 9.0
                    }
                ]
            }"#,
        )
        .unwrap();

        let outcome = apply_world_graph_patch(&base, &patch).unwrap();
        assert!(outcome.applied);
        assert!(!outcome.blocked_by_authority);
        assert_eq!(outcome.graph.guardrails.target_hz, 120);
        assert!((outcome.graph.guardrails.frame_budget_ms - 8.33).abs() < 1e-5);
        assert_eq!(
            outcome.graph.guardrails.max_dynamic_lights,
            MAX_DYNAMIC_LIGHTS_120HZ
        );
        assert_eq!(
            outcome.graph.guardrails.max_draw_calls,
            MAX_DRAW_CALLS_120HZ
        );
        assert_eq!(outcome.graph.environment.time_hours, 24.0);
        assert_eq!(outcome.graph.environment.light_boost, 2.0);
        assert_eq!(outcome.graph.camera.fov_y_deg, 110.0);
        assert_eq!(outcome.graph.entities[0].speed_scale, 3.0);
        assert!(
            outcome.guardrail_notices.len() >= 8,
            "{:?}",
            outcome.guardrail_notices
        );
    }

    #[test]
    fn server_authority_blocks_rng_secret_balance_and_cheat_prompts_and_patches() {
        let base = SemanticWorldGraph::default();
        let clients = [1u64, 2u64];

        for forbidden_prompt in [
            "set rng seed to 12345",
            "reveal secret state of the server",
            "update player balance ledger to 999999",
            "cheat and rig the next outcome to force win",
            "서버 난수 시드 변경해줘",
            "계정 잔액 원장 수정해줘",
            "승부 조작해서 무조건 이기게 해줘",
        ] {
            let (outcome, outbox) = compile_world_graph_prompt(&base, forbidden_prompt, &clients);
            assert!(
                outcome.blocked_by_authority,
                "expected authority block for {forbidden_prompt:?}"
            );
            assert!(!outcome.applied);
            assert_eq!(outcome.graph, base);
            assert!(outbox.is_empty());
            assert!(!outcome.guardrail_notices.is_empty());
        }

        for forbidden_patch in [
            r#"{"authority_mode": "client-authoritative"}"#,
            r#"{"rng_seed": 42}"#,
            r#"{"environment": {"preset": "sunset"}, "secret_state": {"next": 1}}"#,
            r#"{"ledger": {"balance": 1000}}"#,
            r#"{"entities": [{"id": "primary_actor", "rigged": true}]}"#,
        ] {
            let pval = json::parse(forbidden_patch.as_bytes()).unwrap();
            let outcome = apply_world_graph_patch(&base, &pval).unwrap();
            assert!(
                outcome.blocked_by_authority,
                "expected patch block for {forbidden_patch}"
            );
            assert!(!outcome.applied);
            assert_eq!(outcome.graph, base);
            assert!(outcome.broadcast_event.is_none());
        }

        // Ensure legitimate word "bright" is never falsely flagged by whole-word "rig" check!
        let (bright_out, bright_box) =
            compile_world_graph_prompt(&base, "make lighting brighter", &clients);
        assert!(!bright_out.blocked_by_authority);
        assert!(bright_out.applied);
        assert_eq!(bright_box.len(), 2);
    }

    #[test]
    fn bilingual_en_ko_prompt_compiler_deterministically_patches_and_broadcasts() {
        let base = SemanticWorldGraph::default();
        let clients = [10u64, 20u64];

        // 1. EN sunset
        let (s1, box1) = compile_world_graph_prompt(&base, "set lighting to sunset", &clients);
        assert!(s1.applied && !s1.blocked_by_authority);
        assert_eq!(s1.graph.environment.preset, "sunset");
        assert!((s1.graph.environment.time_hours - 18.25).abs() < 1e-5);
        assert_eq!(box1.len(), 2);
        assert_eq!(
            box1[0].1.get("type"),
            Some(&Value::String("world-graph-updated".into()))
        );

        // 2. KO compound prompt: sunset + open ceiling
        let (s2, box2) =
            compile_world_graph_prompt(&base, "노을 조명으로 바꾸고 천장 열어줘", &clients);
        assert!(s2.applied && !s2.blocked_by_authority);
        assert_eq!(s2.graph.environment.preset, "sunset");
        assert!(s2.graph.environment.enclosure_open);
        assert_eq!(box2.len(), 2);

        // 3. Play <-> Edit mode
        let (s3, _) = compile_world_graph_prompt(&s2.graph, "pause into edit mode", &clients);
        assert_eq!(s3.graph.play_mode, PlayMode::Edit);
        let (s3_play, _) = compile_world_graph_prompt(&s3.graph, "플레이 모드로 재생", &clients);
        assert_eq!(s3_play.graph.play_mode, PlayMode::Play);

        // 4. Speed 1.5x + clamped speed 9x
        let (s4, _) = compile_world_graph_prompt(&s3_play.graph, "speed 1.5x", &clients);
        assert!((s4.graph.entities[0].speed_scale - 1.5).abs() < 1e-5);
        let (s4_clamp, _) = compile_world_graph_prompt(&s4.graph, "9배속으로 설정", &clients);
        assert!((s4_clamp.graph.entities[0].speed_scale - 3.0).abs() < 1e-5);
        assert!(!s4_clamp.guardrail_notices.is_empty());

        // 5. Camera closeup -> reset camera
        let (s5_close, _) = compile_world_graph_prompt(&s4.graph, "closeup camera", &clients);
        assert_eq!(s5_close.graph.camera.preset, "closeup");
        assert!((s5_close.graph.camera.fov_y_deg - 38.0).abs() < 1e-5);
        let (s5_reset, _) = compile_world_graph_prompt(&s5_close.graph, "reset camera", &clients);
        assert_eq!(s5_reset.graph.camera, CameraState::default());
    }

    #[test]
    fn entity_map_patch_hhmm_time_weather_and_unrecognized_prompts_behave_cleanly() {
        let base = SemanticWorldGraph::default();
        let map_patch = json::parse(
            br#"{
                "entities": {
                    "primary_actor": {
                        "visible": false,
                        "tint_rgba": [0.4, 0.8, 1.0, 1.0]
                    },
                    "accent_lamp": {
                        "role": "light_prop",
                        "offset_metres": [1.2, 0.8, -0.5]
                    }
                }
            }"#,
        )
        .unwrap();
        let patched = apply_world_graph_patch(&base, &map_patch).unwrap();
        assert!(patched.applied);
        assert!(!patched.graph.entities[0].visible);
        assert_eq!(patched.graph.entities.len(), 3);
        assert_eq!(patched.graph.entities[2].id, "accent_lamp");

        let (weather_out, _) = compile_world_graph_prompt(
            &patched.graph,
            "set time to 19:30, rain weather, show primary_actor",
            &[1],
        );
        assert!(weather_out.applied);
        assert!((weather_out.graph.environment.time_hours - 19.5).abs() < 1e-5);
        assert_eq!(weather_out.graph.environment.weather, "rain");
        assert!(weather_out.graph.entities[0].visible);

        let (noop_out, noop_box) = compile_world_graph_prompt(&base, "   ", &[1]);
        assert!(!noop_out.applied && !noop_out.blocked_by_authority);
        assert!(noop_box.is_empty());
    }
}
