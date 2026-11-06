// Ganbate Studio — Material & Surface Lab (`mountMaterialStudio`).
// Self-contained ES module aligned with `StudioWorkbenchApi` (`editor/plugin.d.ts`).
// Bridges art-directed stylized cel/character shading and physically-based GGX, ClearCoat,
// Charlie cloth, and Subsurface skin BRDFs with EV100 physical camera & AgX / ACES tonemapping.

export const REFERENCE_EV100 = 13.0;

export const SURFACE_MODELS = Object.freeze([
  {
    id: 'stylized_ramp',
    label: 'Stylized Ramp (Soft Cel)',
    summary: 'Art-directed light-to-shadow ramp with controllable band center, softness, and silhouette rim light for stylized 3D environments.',
    fields: ['rampCenter', 'rampSoftness', 'rimStrength'],
  },
  {
    id: 'stylized_character',
    label: 'Stylized Character (Anime / Banded)',
    summary: 'Expressive character shader combining cel shadow banding, warm subsurface skin scatter, specular gloss, and rim lighting for 3D cast models.',
    fields: ['rampCenter', 'rampSoftness', 'rimStrength', 'subsurface', 'subsurfaceColorHex', 'clearcoat', 'clearcoatRoughness'],
  },
  {
    id: 'standard_ggx',
    label: 'Standard GGX (Photoreal PBR)',
    summary: 'Physically-based microfacet surface for metals, stone, wood, and painted props using Base Albedo, Roughness, Metallic, and Normal Strength.',
    fields: [],
  },
  {
    id: 'clearcoat',
    label: 'ClearCoat (Lacquer / Ceramic)',
    summary: 'Dual-layer surface with a glossy transparent varnish coat over a base material — ideal for lacquered table rails, polished wood, and ceramic tokens.',
    fields: ['clearcoat', 'clearcoatRoughness'],
  },
  {
    id: 'charlie_cloth',
    label: 'Charlie Cloth (Velvet / Woven)',
    summary: 'Micro-fiber sheen BRDF that produces soft grazing highlights across table baize/felt, velvet cushions, and woven garments.',
    fields: ['sheen', 'sheenTint'],
  },
  {
    id: 'subsurface_skin',
    label: 'Subsurface Skin (Organic / Jade)',
    summary: 'Translucent scattering surface where light wraps softly through skin, jade, wax, porcelain, or warm paper lanterns.',
    fields: ['subsurface', 'subsurfaceColorHex'],
  },
]);

export const TONEMAP_MODES = Object.freeze([
  { id: 'stylized', label: 'Stylized ACES' },
  { id: 'agx', label: 'AgX (Chroma-Preserving)' },
  { id: 'aces_fitted', label: 'ACES Fitted (RRT+ODT)' },
]);

export const MATERIAL_PRESETS = Object.freeze([
  {
    id: 'polished_lacquer',
    label: 'Polished Lacquer',
    usage: 'Table Rails, Wood & Ceramic',
    name: 'polished_lacquer',
    surface: 'clearcoat',
    baseColorHex: '#7a1612',
    roughness: 0.22,
    metallic: 0.05,
    normalScale: 1.0,
    emissiveHex: '#000000',
    clearcoat: 0.95,
    clearcoatRoughness: 0.06,
    sheen: 0.0,
    sheenTint: 0.0,
    subsurface: 0.0,
    subsurfaceColorHex: '#d9482e',
  },
  {
    id: 'brushed_brass',
    label: 'Brushed Brass',
    usage: 'Metal Trim, Coins & Lanterns',
    name: 'brushed_brass',
    surface: 'standard_ggx',
    baseColorHex: '#d8ad54',
    roughness: 0.28,
    metallic: 0.96,
    normalScale: 1.0,
    emissiveHex: '#000000',
    clearcoat: 0.0,
    clearcoatRoughness: 0.15,
    sheen: 0.0,
    sheenTint: 0.0,
    subsurface: 0.0,
    subsurfaceColorHex: '#d9482e',
  },
  {
    id: 'woven_velvet',
    label: 'Woven Velvet',
    usage: 'Table Baize, Felt & Cloth',
    name: 'woven_velvet',
    surface: 'charlie_cloth',
    baseColorHex: '#233a66',
    roughness: 0.58,
    metallic: 0.0,
    normalScale: 1.0,
    emissiveHex: '#000000',
    clearcoat: 0.0,
    clearcoatRoughness: 0.15,
    sheen: 0.88,
    sheenTint: 0.45,
    subsurface: 0.0,
    subsurfaceColorHex: '#d9482e',
  },
  {
    id: 'subsurface_porcelain',
    label: 'Subsurface Porcelain',
    usage: 'Skin, Jade & Paper Lanterns',
    name: 'subsurface_porcelain',
    surface: 'subsurface_skin',
    baseColorHex: '#ede3d5',
    roughness: 0.30,
    metallic: 0.0,
    normalScale: 0.8,
    emissiveHex: '#000000',
    clearcoat: 0.0,
    clearcoatRoughness: 0.12,
    sheen: 0.0,
    sheenTint: 0.0,
    subsurface: 0.78,
    subsurfaceColorHex: '#e66445',
  },
  {
    id: 'cel_character',
    label: 'Cel Character',
    usage: '3D Cast & Stylized Avatars',
    name: 'cel_character',
    surface: 'stylized_character',
    baseColorHex: '#e87a64',
    roughness: 0.34,
    metallic: 0.0,
    normalScale: 0.6,
    emissiveHex: '#000000',
    clearcoat: 0.65,
    clearcoatRoughness: 0.55,
    sheen: 0.0,
    sheenTint: 0.0,
    subsurface: 0.45,
    subsurfaceColorHex: '#f05a3e',
  },
  {
    id: 'painterly_stone',
    label: 'Painterly Stone',
    usage: 'Environment & Architecture',
    name: 'painterly_stone',
    surface: 'stylized_ramp',
    baseColorHex: '#7d8078',
    roughness: 0.82,
    metallic: 0.0,
    normalScale: 1.0,
    emissiveHex: '#000000',
    clearcoat: 0.0,
    clearcoatRoughness: 0.2,
    sheen: 0.0,
    sheenTint: 0.0,
    subsurface: 0.0,
    subsurfaceColorHex: '#d9482e',
  },
]);

const clamp = (x, lo, hi) => Math.min(hi, Math.max(lo, x));
const mix = (a, b, t) => a + (b - a) * t;
const mix3 = (a, b, t) => [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)];
const norm3 = (v) => {
  const len = Math.hypot(v[0], v[1], v[2]) || 1;
  return [v[0] / len, v[1] / len, v[2] / len];
};
const smoothstep = (e0, e1, x) => {
  const t = clamp((x - e0) / Math.max(e1 - e0, 1e-6), 0, 1);
  return t * t * (3 - 2 * t);
};

export function srgbChannelToLinear(c) {
  const u = clamp(c, 0, 1);
  return u <= 0.04045 ? u / 12.92 : ((u + 0.055) / 1.055) ** 2.4;
}

export function linearChannelToSrgb(l) {
  const u = clamp(l, 0, 1);
  return u <= 0.0031308 ? 12.92 * u : 1.055 * u ** (1 / 2.4) - 0.055;
}

export function srgbHexToLinear(hex) {
  const clean = typeof hex === 'string' && /^#[0-9a-f]{6}$/i.test(hex.trim())
    ? hex.trim().slice(1)
    : 'cccccc';
  return [0, 2, 4].map((i) => srgbChannelToLinear(parseInt(clean.slice(i, i + 2), 16) / 255));
}

export function linearToSrgbHex(rgb) {
  if (!Array.isArray(rgb) || rgb.length < 3) return '#cccccc';
  return '#' + [0, 1, 2].map((i) => {
    const byte = Math.round(clamp(linearChannelToSrgb(Number(rgb[i]) || 0), 0, 1) * 255);
    return byte.toString(16).padStart(2, '0');
  }).join('');
}

export function computeEv100({ aperture = 8.0, shutterSpeed = 1 / 125, iso = 100, compensation = 0 } = {}) {
  const n = Math.max(Number(aperture) || 8.0, 0.5);
  const t = Math.max(Number(shutterSpeed) || 1 / 125, 1e-6);
  const s = Math.max(Number(iso) || 100, 1);
  const comp = Number(compensation) || 0;
  return Math.log2((n * n) / t) - Math.log2(s / 100) - comp;
}

export function ev100ExposureScale(ev100) {
  return 2 ** clamp(REFERENCE_EV100 - Number(ev100), -16, 16);
}

function tonemapRgbInto(r0, g0, b0, mode, out3) {
  if (mode === 'agx') {
    const x0 = Math.max(r0, 1e-10);
    const x1 = Math.max(g0, 1e-10);
    const x2 = Math.max(b0, 1e-10);
    const i0 = 0.84247906 * x0 + 0.07843360 * x1 + 0.07922375 * x2;
    const i1 = 0.04232824 * x0 + 0.87846864 * x1 + 0.07916613 * x2;
    const i2 = 0.04237565 * x0 + 0.07843360 * x1 + 0.87914297 * x2;
    const curve = (v) => {
      const ev = clamp((Math.log2(Math.max(v, 1e-10)) + 12.47393) / 16.5, 0, 1);
      const e2 = ev * ev;
      const e4 = e2 * e2;
      const p = 15.5 * e4 * e2 - 40.14 * e4 * ev + 31.96 * e4 - 6.868 * e2 * ev + 0.4298 * e2 + 0.1191 * ev - 0.00232;
      return Math.max(p, 0) ** 2.2;
    };
    const l0 = curve(i0);
    const l1 = curve(i1);
    const l2 = curve(i2);
    out3[0] = clamp(1.19687901 * l0 - 0.09802088 * l1 - 0.09902974 * l2, 0, 1);
    out3[1] = clamp(-0.05289685 * l0 + 1.15190313 * l1 - 0.09896118 * l2, 0, 1);
    out3[2] = clamp(-0.05297164 * l0 - 0.09804345 * l1 + 1.15107367 * l2, 0, 1);
    return;
  }
  if (mode === 'aces_fitted') {
    const x0 = Math.max(r0, 0);
    const x1 = Math.max(g0, 0);
    const x2 = Math.max(b0, 0);
    const a0 = 0.59719 * x0 + 0.35458 * x1 + 0.04823 * x2;
    const a1 = 0.07600 * x0 + 0.90834 * x1 + 0.01566 * x2;
    const a2 = 0.02840 * x0 + 0.13383 * x1 + 0.83777 * x2;
    const rrt = (v) => (v * (v + 0.0245786) - 0.000090537) / (v * (0.983729 * v + 0.4329510) + 0.238081);
    const q0 = rrt(a0);
    const q1 = rrt(a1);
    const q2 = rrt(a2);
    out3[0] = clamp(1.60475 * q0 - 0.53108 * q1 - 0.07367 * q2, 0, 1);
    out3[1] = clamp(-0.10208 * q0 + 1.10813 * q1 - 0.00605 * q2, 0, 1);
    out3[2] = clamp(-0.00327 * q0 - 0.07276 * q1 + 1.07602 * q2, 0, 1);
    return;
  }
  const sty = (v) => {
    const x = Math.max(v * 0.8, 0);
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0, 1);
  };
  out3[0] = sty(r0);
  out3[1] = sty(g0);
  out3[2] = sty(b0);
}

export function tonemapLinearRgb(rgb, mode = 'stylized') {
  const out3 = [0, 0, 0];
  tonemapRgbInto(rgb[0], rgb[1], rgb[2], mode, out3);
  return out3;
}

function dGgx(nh, a2) {
  const d = nh * nh * (a2 - 1) + 1;
  return a2 / Math.max(Math.PI * d * d, 1e-6);
}

function vSmithGgx(nv, nl, a2) {
  const gv = nl * Math.sqrt(nv * nv * (1 - a2) + a2);
  const gl = nv * Math.sqrt(nl * nl * (1 - a2) + a2);
  return 0.5 / Math.max(gv + gl, 1e-5);
}

function fSchlick(vh, f0) {
  const m = clamp(1 - vh, 0, 1) ** 5;
  return [f0[0] + (1 - f0[0]) * m, f0[1] + (1 - f0[1]) * m, f0[2] + (1 - f0[2]) * m];
}

export function defaultLabState() {
  return {
    name: 'polished_lacquer',
    surface: 'clearcoat',
    baseColorHex: '#7a1612',
    alpha: 1.0,
    roughness: 0.22,
    metallic: 0.05,
    normalScale: 1.0,
    emissiveHex: '#000000',
    emissiveIntensity: 0.0,
    clearcoat: 0.95,
    clearcoatRoughness: 0.06,
    sheen: 0.0,
    sheenTint: 0.45,
    subsurface: 0.0,
    subsurfaceColorHex: '#e66445',
    rampCenter: 0.5,
    rampSoftness: 0.08,
    rimStrength: 0.35,
    tonemap: 'agx',
    aperture: 8.0,
    shutterSpeed: 1 / 125,
    iso: 100,
    compensation: 0.0,
  };
}

const PREVIEW_LIGHT = norm3([0.52, 0.64, 0.56]);
const PREVIEW_HALF = norm3([PREVIEW_LIGHT[0], PREVIEW_LIGHT[1], PREVIEW_LIGHT[2] + 1]);
const PREVIEW_VH = Math.max(PREVIEW_HALF[2], 0);
const PREVIEW_KEY = [2.85, 2.72, 2.52];
const PREVIEW_AMB_SKY = [0.22, 0.28, 0.42];
const PREVIEW_AMB_GND = [0.09, 0.08, 0.07];
const PREVIEW_COAT_SKY = [0.55, 0.65, 0.86];

function prepareBrdfParams(rawState = {}) {
  const s = { ...defaultLabState(), ...rawState };
  const albedo = srgbHexToLinear(s.baseColorHex);
  const emiGain = Math.max(0, Number(s.emissiveIntensity) || 0);
  const emissive = emiGain > 0 ? srgbHexToLinear(s.emissiveHex).map((c) => c * emiGain) : [0, 0, 0];
  const sssCol = srgbHexToLinear(s.subsurfaceColorHex);
  const r = clamp(Number(s.roughness) || 0.25, 0.045, 1.0);
  const metallic = clamp(Number(s.metallic) || 0, 0, 1);
  const exposure = ev100ExposureScale(computeEv100(s));
  return {
    surface: s.surface,
    tonemap: s.tonemap,
    albedo,
    emissive,
    sssCol,
    r,
    a2: (r * r) ** 2,
    metallic,
    f0: mix3([0.04, 0.04, 0.04], albedo, metallic),
    rampCenter: s.rampCenter,
    rampSoftness: s.rampSoftness,
    rimStrength: s.rimStrength,
    clearcoat: s.clearcoat,
    clearcoatRoughness: s.clearcoatRoughness,
    sheen: s.sheen,
    sheenTint: s.sheenTint,
    subsurface: s.subsurface,
    exposure,
  };
}

const SCRATCH_MAPPED = [0, 0, 0];

function evaluatePreparedBrdfPixel(nx, ny, nz, p, outBuf, offset) {
  const invLen = 1 / (Math.hypot(nx, ny, nz) || 1);
  const n0 = nx * invLen;
  const n1 = ny * invLen;
  const n2 = nz * invLen;
  const nlRaw = n0 * PREVIEW_LIGHT[0] + n1 * PREVIEW_LIGHT[1] + n2 * PREVIEW_LIGHT[2];
  const nl = Math.max(nlRaw, 0);
  const nv = Math.max(n2, 1e-4);
  const nh = Math.max(n0 * PREVIEW_HALF[0] + n1 * PREVIEW_HALF[1] + n2 * PREVIEW_HALF[2], 0);
  const vh = PREVIEW_VH;
  const hemiT = n1 * 0.5 + 0.5;
  const hemi0 = PREVIEW_AMB_GND[0] + (PREVIEW_AMB_SKY[0] - PREVIEW_AMB_GND[0]) * hemiT;
  const hemi1 = PREVIEW_AMB_GND[1] + (PREVIEW_AMB_SKY[1] - PREVIEW_AMB_GND[1]) * hemiT;
  const hemi2 = PREVIEW_AMB_GND[2] + (PREVIEW_AMB_SKY[2] - PREVIEW_AMB_GND[2]) * hemiT;
  const hemi = [hemi0, hemi1, hemi2];
  const { albedo, sssCol, r, a2, metallic } = p;

  const hdr = [0, 0, 0];
  if (p.surface === 'stylized_ramp' || p.surface === 'stylized_character') {
    const isChar = p.surface === 'stylized_character';
    const halfL = nlRaw * 0.5 + 0.5;
    const width = isChar ? Math.max(p.rampSoftness * 0.3, 0.008) : Math.max(p.rampSoftness, 0.005);
    const lit = smoothstep(p.rampCenter - width, p.rampCenter + width, halfL);
    const dark = [0.68, 0.72, 0.92];
    const sssBand = isChar
      ? smoothstep(p.rampCenter - 0.14, p.rampCenter, halfL)
        * (1 - smoothstep(p.rampCenter, p.rampCenter + 0.12, halfL))
        * Math.max(p.subsurface, 0.35)
      : 0;
    const gloss = 1 - r;
    const specEdge = 1 - (0.03 + 0.12 * r);
    const spec = smoothstep(specEdge - 0.012, specEdge + 0.012, nh) * 0.55 * gloss * gloss * lit;
    const rim = ((1 - nv) ** 3.2) * p.rimStrength * clamp(halfL, 0, 1);
    const specTint = mix3([1, 1, 1], albedo, metallic);
    for (let k = 0; k < 3; k++) {
      const diff = PREVIEW_KEY[k] * (lit + sssCol[k] * sssBand * 0.55) + hemi[k] * mix(dark[k], 1, lit);
      hdr[k] = albedo[k] * diff * (1 - 0.45 * metallic) + specTint[k] * PREVIEW_KEY[k] * spec + PREVIEW_AMB_SKY[k] * 1.5 * rim;
    }
  } else if (p.surface === 'charlie_cloth') {
    const invR = 1 / Math.max(r, 0.07);
    const dc = ((2 + invR) * Math.max(1 - nh * nh, 1e-5) ** (0.5 * invR)) / (2 * Math.PI);
    const vc = 1 / Math.max(4 * (nl + nv - nl * nv), 0.12);
    const lum = Math.max(0.2126 * albedo[0] + 0.7152 * albedo[1] + 0.0722 * albedo[2], 0.04);
    const sheenW = clamp(p.sheen > 0 ? p.sheen : 0.8, 0, 1);
    const tint = clamp(p.sheenTint, 0, 1);
    const wrapNl = clamp((nlRaw + 0.22) / 1.22, 0, 1);
    const graze = (1 - nv) ** 3;
    for (let k = 0; k < 3; k++) {
      const sc = mix(1, albedo[k] / lum, tint) * sheenW;
      hdr[k] = PREVIEW_KEY[k] * ((albedo[k] / Math.PI) * wrapNl + sc * dc * vc * nl) + (albedo[k] + sc * graze * 0.65) * hemi[k];
    }
  } else if (p.surface === 'subsurface_skin') {
    const f = fSchlick(vh, [0.028, 0.028, 0.028]);
    const spec = dGgx(nh, a2) * vSmithGgx(nv, Math.max(nl, 1e-4), a2) * nl;
    const sssW = clamp(p.subsurface > 0 ? p.subsurface : 0.7, 0, 1);
    const wraps = [0.52 * sssW, 0.24 * sssW, 0.10 * sssW];
    for (let k = 0; k < 3; k++) {
      const wNl = clamp((nlRaw + wraps[k]) / (1 + wraps[k]), 0, 1);
      const scatter = 1 + sssCol[k] * 0.32 * sssW * (1 - nl);
      hdr[k] = ((albedo[k] * (1 - f[k])) / Math.PI) * wNl * scatter * PREVIEW_KEY[k] + f[k] * spec * PREVIEW_KEY[k] + albedo[k] * hemi[k];
    }
  } else {
    const f = fSchlick(vh, p.f0);
    const d = dGgx(nh, a2);
    const vis = vSmithGgx(nv, Math.max(nl, 1e-4), a2);
    const rBlend = r * r * 0.82;
    const reflSky = [
      0.45 + (hemi0 - 0.45) * rBlend,
      0.56 + (hemi1 - 0.56) * rBlend,
      0.78 + (hemi2 - 0.78) * rBlend,
    ];
    for (let k = 0; k < 3; k++) {
      const kd = (1 - f[k]) * (1 - metallic);
      const spec = d * vis * f[k] * (1 + p.f0[k] * 0.35);
      hdr[k] = ((kd * albedo[k]) / Math.PI + spec) * nl * PREVIEW_KEY[k] + kd * albedo[k] * hemi[k] + reflSky[k] * f[k] * (1 - r * 0.5);
    }
    if (p.surface === 'clearcoat') {
      const coatW = clamp(p.clearcoat > 0 ? p.clearcoat : 0.85, 0, 1);
      const coatR = clamp(p.clearcoatRoughness || 0.08, 0.03, 1);
      const ac2 = (coatR * coatR) ** 2;
      const fc = fSchlick(vh, [0.04, 0.04, 0.04])[0] * coatW;
      const coatSpec = dGgx(nh, ac2) * vSmithGgx(nv, Math.max(nl, 1e-4), ac2) * fc * nl;
      for (let k = 0; k < 3; k++) {
        hdr[k] = hdr[k] * (1 - fc * 0.85) + PREVIEW_KEY[k] * coatSpec + PREVIEW_COAT_SKY[k] * fc * 0.65;
      }
    }
  }

  tonemapRgbInto(
    (hdr[0] + p.emissive[0]) * p.exposure,
    (hdr[1] + p.emissive[1]) * p.exposure,
    (hdr[2] + p.emissive[2]) * p.exposure,
    p.tonemap,
    SCRATCH_MAPPED,
  );
  outBuf[offset] = Math.round(clamp(linearChannelToSrgb(SCRATCH_MAPPED[0]), 0, 1) * 255);
  outBuf[offset + 1] = Math.round(clamp(linearChannelToSrgb(SCRATCH_MAPPED[1]), 0, 1) * 255);
  outBuf[offset + 2] = Math.round(clamp(linearChannelToSrgb(SCRATCH_MAPPED[2]), 0, 1) * 255);
  outBuf[offset + 3] = 255;
}

export function evaluateBrdfPreviewPixel(nx, ny, nz, rawState = {}) {
  const out = [0, 0, 0, 255];
  evaluatePreparedBrdfPixel(nx, ny, nz, prepareBrdfParams(rawState), out, 0);
  return [out[0], out[1], out[2]];
}

export function renderShaderBallRgba(width, height, state = {}) {
  const w = Math.max(1, Math.floor(width));
  const h = Math.max(1, Math.floor(height));
  const out = new Uint8ClampedArray(w * h * 4);
  const cx = w * 0.5;
  const cy = h * 0.5;
  const radius = Math.min(w, h) * 0.42;
  const prepared = prepareBrdfParams(state);

  for (let y = 0; y < h; y++) {
    const dy = (cy - (y + 0.5)) / radius;
    const dy2 = dy * dy;
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      const dx = (x + 0.5 - cx) / radius;
      const r2 = dx * dx + dy2;
      if (r2 <= 1.0) {
        const dz = Math.sqrt(Math.max(1 - r2, 0));
        evaluatePreparedBrdfPixel(dx, dy, dz, prepared, out, i);
      } else {
        const checker = ((Math.floor(x / 12) + Math.floor(y / 12)) & 1) === 0 ? 24 : 30;
        const glow = Math.exp(-Math.max(Math.sqrt(r2) - 1, 0) * 6.0) * 14;
        out[i] = Math.round(checker + glow * 0.7);
        out[i + 1] = Math.round(checker + glow * 0.8);
        out[i + 2] = Math.round(checker + glow);
        out[i + 3] = 255;
      }
    }
  }
  return out;
}

export function buildMaterialDocOps(rawState = {}) {
  const s = { ...defaultLabState(), ...rawState };
  const baseRgb = srgbHexToLinear(s.baseColorHex);
  const emiRgb = srgbHexToLinear(s.emissiveHex).map((c) => Number((c * Math.max(0, Number(s.emissiveIntensity) || 0)).toFixed(5)));
  const ops = [
    { set: ['name'], value: String(s.name || 'material') },
    { set: ['color'], value: [...baseRgb.map((c) => Number(c.toFixed(5))), Number(clamp(s.alpha ?? 1, 0, 1).toFixed(4))] },
    { set: ['roughness'], value: Number(clamp(s.roughness, 0, 1).toFixed(4)) },
    { set: ['metallic'], value: Number(clamp(s.metallic, 0, 1).toFixed(4)) },
    { set: ['normalScale'], value: Number(Math.max(0, Number(s.normalScale) || 0).toFixed(4)) },
    { set: ['emissive'], value: emiRgb },
    { set: ['shading'], value: 'standard' },
    { set: ['surface'], value: s.surface },
  ];
  const optional = [
    ['clearcoat', s.surface === 'clearcoat' || s.clearcoat > 0 ? Number(clamp(s.clearcoat, 0, 1).toFixed(4)) : null],
    ['clearcoatRoughness', s.surface === 'clearcoat' || s.clearcoat > 0 ? Number(clamp(s.clearcoatRoughness, 0.02, 1).toFixed(4)) : null],
    ['sheen', s.surface === 'charlie_cloth' || s.sheen > 0 ? Number(clamp(s.sheen, 0, 1).toFixed(4)) : null],
    ['sheenTint', s.surface === 'charlie_cloth' || s.sheen > 0 ? Number(clamp(s.sheenTint, 0, 1).toFixed(4)) : null],
    ['subsurface', s.surface === 'subsurface_skin' || s.subsurface > 0 ? Number(clamp(s.subsurface, 0, 1).toFixed(4)) : null],
  ];
  for (const [key, val] of optional) {
    if (val !== null) ops.push({ set: [key], value: val });
    else ops.push({ remove: [key] });
  }
  return ops;
}

export function buildLookPreviewPatch(rawState = {}) {
  const s = { ...defaultLabState(), ...rawState };
  const ev = Number(computeEv100(s).toFixed(3));
  return {
    shading: {
      model: s.surface,
      clearcoat: Number(clamp(s.clearcoat, 0, 1).toFixed(4)),
      clearcoat_roughness: Number(clamp(s.clearcoatRoughness, 0.02, 1).toFixed(4)),
      sheen: Number(clamp(s.sheen, 0, 1).toFixed(4)),
      subsurface: Number(clamp(s.subsurface, 0, 1).toFixed(4)),
      subsurface_color: s.subsurfaceColorHex,
      ramp_center: Number(clamp(s.rampCenter, 0, 1).toFixed(4)),
      ramp_softness: Number(clamp(s.rampSoftness, 0.002, 1).toFixed(4)),
      rim_strength: Number(clamp(s.rimStrength, 0, 2).toFixed(4)),
    },
    post: {
      tonemap: s.tonemap,
      ev100: ev,
      camera: {
        aperture: Number(s.aperture),
        shutter_speed: Number(s.shutterSpeed),
        iso: Number(s.iso),
        compensation: Number(s.compensation),
      },
    },
  };
}

export function stateFromMaterialRoot(root = {}, fallback = {}) {
  const base = { ...defaultLabState(), ...fallback };
  if (!root || typeof root !== 'object') return base;
  if (typeof root.name === 'string' && root.name) base.name = root.name;
  if (Array.isArray(root.color) && root.color.length >= 3) {
    base.baseColorHex = linearToSrgbHex(root.color);
    if (typeof root.color[3] === 'number') base.alpha = clamp(root.color[3], 0, 1);
  }
  if (typeof root.roughness === 'number') base.roughness = clamp(root.roughness, 0, 1);
  if (typeof root.metallic === 'number') base.metallic = clamp(root.metallic, 0, 1);
  if (typeof root.normalScale === 'number') base.normalScale = Math.max(0, root.normalScale);
  if (Array.isArray(root.emissive) && root.emissive.length === 3) {
    const peak = Math.max(root.emissive[0] || 0, root.emissive[1] || 0, root.emissive[2] || 0);
    if (peak > 1e-4) {
      base.emissiveIntensity = Number(peak.toFixed(3));
      base.emissiveHex = linearToSrgbHex(root.emissive.map((c) => c / peak));
    } else {
      base.emissiveIntensity = 0;
    }
  }
  if (typeof root.surface === 'string' && SURFACE_MODELS.some((m) => m.id === root.surface)) {
    base.surface = root.surface;
  }
  if (typeof root.clearcoat === 'number') base.clearcoat = clamp(root.clearcoat, 0, 1);
  if (typeof root.clearcoatRoughness === 'number') base.clearcoatRoughness = clamp(root.clearcoatRoughness, 0.02, 1);
  if (typeof root.sheen === 'number') base.sheen = clamp(root.sheen, 0, 1);
  if (typeof root.sheenTint === 'number') base.sheenTint = clamp(root.sheenTint, 0, 1);
  if (typeof root.subsurface === 'number') base.subsurface = clamp(root.subsurface, 0, 1);
  return base;
}

const LABELS = Object.freeze({
  materialStudioTitle: 'Material & Surface Lab',
  materialDoc: 'Material Asset',
  newMaterial: 'New Material',
  presets: 'Surface Presets (3D Game Surfaces)',
  surfaceModel: 'Surface Response (BRDF)',
  baseColor: 'Base Albedo',
  roughness: 'Roughness',
  metallic: 'Metallic',
  normalScale: 'Normal Strength',
  clearcoat: 'ClearCoat Weight',
  clearcoatRoughness: 'ClearCoat Roughness',
  sheen: 'Cloth Sheen',
  sheenTint: 'Sheen Tint',
  subsurface: 'Subsurface Scatter',
  subsurfaceColor: 'Subsurface Tint',
  rampCenter: 'Cel Ramp Center',
  rampSoftness: 'Cel Ramp Softness',
  rimStrength: 'Rim Light',
  cameraSection: 'Physical Camera & Tone Mapping',
  tonemap: 'Tone Mapper',
  aperture: 'Aperture (f/)',
  shutterInv: 'Shutter (1/s)',
  iso: 'Sensor ISO',
  compensation: 'EV Comp',
  undo: 'Undo',
  redo: 'Redo',
  save: 'Save',
});

const MATERIAL_STUDIO_CSS = `
.ganbate-material-studio {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px 14px 20px;
  height: 100%;
  overflow-y: auto;
  background: #14161b;
  color: var(--text, #d9dce3);
}
.ganbate-material-studio .material-studio-header {
  margin: 0;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--line, #2c2f36);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
}
.ganbate-material-studio .material-studio-title-group {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.ganbate-material-studio .material-studio-subtitle {
  font-size: 11.5px;
  font-weight: 400;
  color: var(--dim, #8a909c);
}
.ganbate-material-studio .material-studio-actions {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
}
.ganbate-material-studio .material-studio-card {
  background: var(--panel, #1e2025);
  border: 1px solid var(--line, #2c2f36);
  border-radius: 7px;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.ganbate-material-studio .material-studio-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  padding-bottom: 6px;
}
.ganbate-material-studio .material-studio-card-head strong {
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: #f3f5f8;
}
.ganbate-material-studio .material-studio-badge {
  font-size: 11px;
  padding: 2px 7px;
  border-radius: 999px;
  background: rgba(232, 180, 74, 0.14);
  border: 1px solid rgba(232, 180, 74, 0.42);
  color: var(--accent, #e8b44a);
  font-weight: 600;
}
.ganbate-material-studio .material-studio-hero {
  display: grid;
  grid-template-columns: 140px minmax(0, 1fr);
  gap: 12px;
  align-items: stretch;
}
@media (max-width: 640px) {
  .ganbate-material-studio .material-studio-hero {
    grid-template-columns: minmax(0, 1fr);
  }
}
.ganbate-material-studio .material-studio-preview-wrap {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 10px;
  border-radius: 7px;
  background: #0f1116;
  border: 1px solid var(--line, #2c2f36);
}
.ganbate-material-studio .material-studio-ball {
  width: 108px;
  height: 108px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.45);
}
.ganbate-material-studio .material-studio-ev {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 11px;
  color: var(--accent, #e8b44a);
  white-space: nowrap;
}
.ganbate-material-studio .material-studio-light-pills {
  display: flex;
  gap: 3px;
  flex-wrap: wrap;
  justify-content: center;
}
.ganbate-material-studio .material-studio-light-pills button.mini {
  margin: 0;
  font-size: 10.5px;
  padding: 2px 5px;
}
.ganbate-material-studio .material-studio-presets {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(148px, 1fr));
  gap: 6px;
}
.ganbate-material-studio .material-preset-btn {
  margin: 0;
  padding: 6px 8px;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  text-align: left;
  border-radius: 6px;
  background: #16181e;
  border: 1px solid var(--line, #2c2f36);
  transition: border-color 0.12s ease, background 0.12s ease;
}
.ganbate-material-studio .material-preset-btn.on {
  background: rgba(232, 180, 74, 0.14);
  border-color: var(--accent, #e8b44a);
  color: #fff;
}
.ganbate-material-studio .material-preset-top {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
}
.ganbate-material-studio .material-preset-swatch {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  border: 1px solid rgba(255, 255, 255, 0.35);
  flex: none;
}
.ganbate-material-studio .material-preset-usage {
  font-size: 10.5px;
  color: var(--dim, #8a909c);
  font-weight: 400;
}
.ganbate-material-studio .material-studio-guide {
  padding: 7px 10px;
  border-radius: 5px;
  background: rgba(122, 162, 247, 0.08);
  border: 1px solid rgba(122, 162, 247, 0.28);
  font-size: 11.5px;
  color: #c8d6f5;
  line-height: 1.4;
}
.ganbate-material-studio .field {
  display: grid;
  grid-template-columns: 145px minmax(0, 1fr) 54px;
  gap: 8px;
  align-items: center;
  margin: 2px 0;
}
.ganbate-material-studio .field.field-select,
.ganbate-material-studio .field.field-color {
  grid-template-columns: 145px minmax(0, 1fr);
}
.ganbate-material-studio .field.is-hidden {
  display: none !important;
}
.ganbate-material-studio .field input[type="range"] {
  width: 100%;
  accent-color: var(--accent, #e8b44a);
}
.ganbate-material-studio .material-studio-val {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 11px;
  color: var(--dim, #8a909c);
  text-align: right;
}
.ganbate-material-studio .material-studio-scene-entities {
  display: flex;
  align-items: center;
  gap: 5px;
  flex-wrap: wrap;
}
.ganbate-material-studio .material-studio-scene-entities button.mini {
  margin: 0;
}
`;

function makeEl(doc, tag, attrs = {}, ...children) {
  const node = doc.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined) continue;
    if (k.startsWith('on') && typeof v === 'function') node.addEventListener(k.slice(2).toLowerCase(), v);
    else if (k === 'class') node.className = v;
    else if (k === 'disabled') node.disabled = Boolean(v);
    else node.setAttribute(k, String(v));
  }
  for (const child of children.flat()) {
    if (child === null || child === undefined || child === false) continue;
    node.append(typeof child === 'string' || typeof child === 'number' ? doc.createTextNode(String(child)) : child);
  }
  return node;
}

function ensureMaterialStudioStyles(ownerDoc) {
  if (!ownerDoc?.head || typeof ownerDoc.querySelector !== 'function') return;
  if (ownerDoc.querySelector('style[data-material-studio-style="1"]')) return;
  const styleEl = makeEl(ownerDoc, 'style', { 'data-material-studio-style': '1' }, MATERIAL_STUDIO_CSS);
  ownerDoc.head.append(styleEl);
}

export function mountMaterialStudio(container, studioApi = {}) {
  if (!container) throw new Error('mountMaterialStudio requires a container element.');
  const ownerDoc = container.ownerDocument || globalThis.document;
  ensureMaterialStudioStyles(ownerDoc);
  const tr = (key) => {
    const fromHost = studioApi?.t?.(key);
    if (fromHost && fromHost !== key) return fromHost;
    return LABELS[key] || key;
  };

  let state = defaultLabState();
  let activeDocId = null;
  let activeDocPath = null;
  let showAllChannels = false;
  const unsubs = [];
  const controls = new Map();
  const fieldRows = new Map();
  const presetBtns = new Map();

  const root = makeEl(ownerDoc, 'div', { class: 'ganbate-material-studio', 'data-workbench': 'material-studio' });
  const previewCanvas = makeEl(ownerDoc, 'canvas', {
    width: 96,
    height: 96,
    class: 'material-studio-ball',
    'aria-label': 'Live BRDF Shader Ball Preview',
  });
  const evReadout = makeEl(ownerDoc, 'span', { class: 'material-studio-ev', 'data-key': 'ev100-readout' });
  const docSelect = makeEl(ownerDoc, 'select', { 'aria-label': tr('materialDoc'), class: 'material-studio-doc-select' });
  const activeTargetBadge = makeEl(ownerDoc, 'span', { class: 'material-studio-badge', 'data-key': 'target-badge' });
  const surfaceGuideBox = makeEl(ownerDoc, 'div', { class: 'material-studio-guide', 'data-key': 'surface-guide' });
  const sceneEntitiesRow = makeEl(ownerDoc, 'div', { class: 'material-studio-scene-entities', 'data-key': 'scene-entities' });

  function paintPreview() {
    const w = Number(previewCanvas.width) || 96;
    const h = Number(previewCanvas.height) || 96;
    const rgba = renderShaderBallRgba(w, h, state);
    previewCanvas._lastRgba = rgba;
    const ctx = typeof previewCanvas.getContext === 'function' ? previewCanvas.getContext('2d') : null;
    if (ctx && typeof ctx.putImageData === 'function') {
      const img = typeof ctx.createImageData === 'function'
        ? ctx.createImageData(w, h)
        : { width: w, height: h, data: rgba };
      if (img.data !== rgba && img.data?.set) img.data.set(rgba);
      ctx.putImageData(img, 0, 0);
    }
    const ev = computeEv100(state);
    const scale = ev100ExposureScale(ev);
    evReadout.textContent = `EV100 ${ev.toFixed(2)} (${scale.toFixed(2)}x)`;
  }

  function syncContextualUi() {
    const modelMeta = SURFACE_MODELS.find((m) => m.id === state.surface) || SURFACE_MODELS[0];
    surfaceGuideBox.textContent = modelMeta.summary || '';
    activeTargetBadge.textContent = activeDocPath
      ? `Editing Asset: ${activeDocPath}`
      : 'Live 3D Scene Look + Shader Preview';

    const activeFields = new Set(modelMeta.fields || []);
    const conditionalKeys = [
      'clearcoat',
      'clearcoatRoughness',
      'sheen',
      'sheenTint',
      'subsurface',
      'subsurfaceColorHex',
      'rampCenter',
      'rampSoftness',
      'rimStrength',
    ];
    for (const key of conditionalKeys) {
      const row = fieldRows.get(key);
      if (!row) continue;
      const visible = showAllChannels || activeFields.has(key);
      row.hidden = !visible;
      row.className = `field${key.endsWith('Hex') ? ' field-color' : ''}${visible ? '' : ' is-hidden'}`;
    }

    for (const [presetId, btn] of presetBtns.entries()) {
      const p = MATERIAL_PRESETS.find((item) => item.id === presetId);
      const isMatch = Boolean(p && p.surface === state.surface && p.baseColorHex === state.baseColorHex);
      btn.className = `mini material-preset-btn${isMatch ? ' on' : ''}`;
    }
  }

  function pushLivePreviewAndEdit(label, merge = true) {
    paintPreview();
    syncContextualUi();
    const vp = studioApi?.getViewport?.();
    if (vp?.setPreviewLook) {
      vp.setPreviewLook(buildLookPreviewPatch(state));
    }
    if (activeDocId && typeof studioApi?.edit === 'function') {
      const ops = buildMaterialDocOps(state);
      Promise.resolve(studioApi.edit(activeDocId, ops, label || 'Edit Material', merge)).catch((err) => {
        studioApi?.log?.(String(err?.message || err), 'error');
      });
    }
  }

  function syncControlsFromState(repaint = true) {
    for (const [key, binding] of controls.entries()) {
      binding.sync(state[key]);
    }
    syncContextualUi();
    if (repaint) paintPreview();
  }

  function refreshSceneEntitiesBar() {
    while (sceneEntitiesRow.firstChild) sceneEntitiesRow.removeChild(sceneEntitiesRow.firstChild);
    const snap = studioApi?.getState?.();
    const entities = snap?.activeDocument?.root?.entities;
    if (!entities || typeof entities !== 'object') return;
    const entries = Object.entries(entities).slice(0, 6);
    if (!entries.length) return;
    sceneEntitiesRow.append(makeEl(ownerDoc, 'span', { class: 'material-studio-subtitle' }, 'Focus 3D Scene Entity:'));
    for (const [entityId, ent] of entries) {
      const label = ent?.name || entityId;
      sceneEntitiesRow.append(
        makeEl(ownerDoc, 'button', {
          type: 'button',
          class: 'mini',
          'data-scene-entity': entityId,
          onclick: () => {
            studioApi?.setSelection?.([entityId]);
            const trs = ent?.components?.transform?.translation;
            if (Array.isArray(trs) && trs.length >= 3) {
              studioApi?.getViewport?.()?.setCameraPose?.({
                target: [Number(trs[0]) || 0, Number(trs[1]) || 0, Number(trs[2]) || 0],
                distance: 8,
              });
            }
          },
        }, label),
      );
    }
  }

  function refreshDocList() {
    const docs = studioApi?.listDocuments?.({ suffix: '.material.json' }) || [];
    while (docSelect.firstChild) docSelect.removeChild(docSelect.firstChild);
    docSelect.append(
      makeEl(
        ownerDoc,
        'option',
        { value: '' },
        docs.length ? 'Select .material.json asset…' : 'Live 3D Scene Preview (Click "New Material" to save asset)',
      ),
    );
    for (const d of docs) {
      docSelect.append(makeEl(ownerDoc, 'option', { value: d.id }, d.path || d.name || d.id));
    }
    if (activeDocId) docSelect.value = activeDocId;
    refreshSceneEntitiesBar();
    syncContextualUi();
  }

  async function selectDocument(idOrPath) {
    if (!idOrPath) {
      activeDocId = null;
      activeDocPath = null;
      syncContextualUi();
      return;
    }
    const opened = await studioApi?.openDocument?.(idOrPath);
    if (opened) {
      activeDocId = opened.id;
      activeDocPath = opened.path;
      state = stateFromMaterialRoot(opened.root, state);
      refreshDocList();
      syncControlsFromState();
      studioApi?.getViewport?.()?.setPreviewLook?.(buildLookPreviewPatch(state));
    }
  }

  docSelect.addEventListener('change', (e) => {
    void selectDocument(e.target.value);
  });

  const btnNew = makeEl(ownerDoc, 'button', {
    type: 'button',
    class: 'mini on',
    'data-action': 'create-material',
    onclick: async () => {
      if (typeof studioApi?.createDocument !== 'function') return;
      const slug = (state.name || 'surface').toLowerCase().replace(/[^a-z0-9_-]+/g, '_');
      const path = `content/materials/${slug}.material.json`;
      const initialRoot = {};
      for (const op of buildMaterialDocOps(state)) {
        if (op.set?.length === 1) initialRoot[op.set[0]] = op.value;
      }
      const created = await studioApi.createDocument({ path, kind: 'material', initialRoot });
      if (created) {
        activeDocId = created.id;
        activeDocPath = created.path;
        refreshDocList();
      }
    },
  }, tr('newMaterial'));

  const btnUndo = makeEl(ownerDoc, 'button', {
    type: 'button',
    class: 'mini',
    'data-action': 'undo',
    onclick: async () => {
      if (!activeDocId || typeof studioApi?.undo !== 'function') return;
      await studioApi.undo(activeDocId);
      const snap = studioApi?.getDocument?.(activeDocId);
      if (snap?.root) {
        state = stateFromMaterialRoot(snap.root, state);
        syncControlsFromState();
      }
    },
  }, tr('undo'));

  const btnRedo = makeEl(ownerDoc, 'button', {
    type: 'button',
    class: 'mini',
    'data-action': 'redo',
    onclick: async () => {
      if (!activeDocId || typeof studioApi?.redo !== 'function') return;
      await studioApi.redo(activeDocId);
      const snap = studioApi?.getDocument?.(activeDocId);
      if (snap?.root) {
        state = stateFromMaterialRoot(snap.root, state);
        syncControlsFromState();
      }
    },
  }, tr('redo'));

  const btnSave = makeEl(ownerDoc, 'button', {
    type: 'button',
    class: 'mini',
    'data-action': 'save',
    onclick: async () => {
      if (!activeDocId || typeof studioApi?.save !== 'function') return;
      await studioApi.save(activeDocId);
    },
  }, tr('save'));

  const lightingPills = makeEl(
    ownerDoc,
    'div',
    { class: 'material-studio-light-pills' },
    [
      { id: 'day', label: 'Day 13:00' },
      { id: 'golden_hour', label: 'Sunset 17:30' },
      { id: 'night', label: 'Night 19:30' },
    ].map((lt) =>
      makeEl(ownerDoc, 'button', {
        type: 'button',
        class: 'mini',
        'data-lighting-preset': lt.id,
        onclick: () => {
          studioApi?.getViewport?.()?.setLightingPreset?.(lt.id);
        },
      }, lt.label),
    ),
  );

  const presetRow = makeEl(
    ownerDoc,
    'div',
    { class: 'material-studio-presets' },
    MATERIAL_PRESETS.map((preset) => {
      const btn = makeEl(
        ownerDoc,
        'button',
        {
          type: 'button',
          class: 'mini material-preset-btn',
          'data-preset': preset.id,
          onclick: () => applyPreset(preset.id),
        },
        makeEl(
          ownerDoc,
          'span',
          { class: 'material-preset-top' },
          makeEl(ownerDoc, 'span', {
            class: 'material-preset-swatch',
            style: `background:${preset.baseColorHex}`,
          }),
          preset.label,
        ),
        makeEl(ownerDoc, 'span', { class: 'material-preset-usage' }, preset.usage || ''),
      );
      presetBtns.set(preset.id, btn);
      return btn;
    }),
  );

  function addSelect(key, labelKey, options) {
    const id = `ms-${key}`;
    const sel = makeEl(
      ownerDoc,
      'select',
      {
        id,
        'data-field': key,
        onchange: (e) => {
          state[key] = e.target.value;
          pushLivePreviewAndEdit(`Set ${key}`, false);
        },
      },
      options.map((o) => makeEl(ownerDoc, 'option', { value: o.id }, o.label)),
    );
    controls.set(key, { sync: (v) => { sel.value = String(v ?? ''); } });
    const row = makeEl(ownerDoc, 'div', { class: 'field field-select' }, makeEl(ownerDoc, 'label', { for: id }, tr(labelKey)), sel);
    fieldRows.set(key, row);
    return row;
  }

  function addSlider(key, labelKey, min, max, step) {
    const id = `ms-${key}`;
    const valSpan = makeEl(ownerDoc, 'span', { class: 'material-studio-val' });
    const input = makeEl(ownerDoc, 'input', {
      id,
      type: 'range',
      min,
      max,
      step,
      'data-field': key,
      oninput: (e) => {
        state[key] = Number(e.target.value);
        valSpan.textContent = state[key].toFixed(2);
        pushLivePreviewAndEdit(`Adjust ${key}`, true);
      },
    });
    controls.set(key, {
      sync: (v) => {
        const n = Number(v ?? 0);
        input.value = String(n);
        valSpan.textContent = n.toFixed(2);
      },
    });
    const row = makeEl(ownerDoc, 'div', { class: 'field' }, makeEl(ownerDoc, 'label', { for: id }, tr(labelKey)), input, valSpan);
    fieldRows.set(key, row);
    return row;
  }

  function addColor(key, labelKey) {
    const id = `ms-${key}`;
    const input = makeEl(ownerDoc, 'input', {
      id,
      type: 'color',
      'data-field': key,
      oninput: (e) => {
        state[key] = e.target.value;
        pushLivePreviewAndEdit(`Tint ${key}`, true);
      },
    });
    controls.set(key, { sync: (v) => { input.value = typeof v === 'string' ? v : '#cccccc'; } });
    const row = makeEl(ownerDoc, 'div', { class: 'field field-color' }, makeEl(ownerDoc, 'label', { for: id }, tr(labelKey)), input);
    fieldRows.set(key, row);
    return row;
  }

  const shutterId = 'ms-shutterInv';
  const shutterInput = makeEl(ownerDoc, 'input', {
    id: shutterId,
    type: 'range',
    min: 15,
    max: 2000,
    step: 5,
    'data-field': 'shutterInv',
    oninput: (e) => {
      const denom = Math.max(1, Number(e.target.value) || 125);
      state.shutterSpeed = 1 / denom;
      shutterVal.textContent = `1/${Math.round(denom)}s`;
      pushLivePreviewAndEdit('Adjust Shutter Speed', true);
    },
  });
  const shutterVal = makeEl(ownerDoc, 'span', { class: 'material-studio-val' });
  controls.set('shutterSpeed', {
    sync: (v) => {
      const denom = Math.round(1 / Math.max(Number(v) || 1 / 125, 1e-4));
      shutterInput.value = String(clamp(denom, 15, 2000));
      shutterVal.textContent = `1/${denom}s`;
    },
  });

  const btnToggleAllChannels = makeEl(ownerDoc, 'button', {
    type: 'button',
    class: 'mini',
    'data-action': 'toggle-all-channels',
    onclick: () => {
      showAllChannels = !showAllChannels;
      btnToggleAllChannels.textContent = showAllChannels ? 'Filter by Surface Model' : 'Show All Channels';
      syncContextualUi();
    },
  }, 'Show All Channels');

  root.append(
    makeEl(ownerDoc, 'div', { class: 'component material-studio-header' },
      makeEl(ownerDoc, 'div', { class: 'material-studio-title-group' },
        makeEl(ownerDoc, 'strong', {}, tr('materialStudioTitle')),
        makeEl(ownerDoc, 'span', { class: 'material-studio-subtitle' },
          'Tune 3D surface finishes (lacquer, baize, metal, skin, cel shading) and physical camera exposure alongside the live 3D viewport.',
        ),
      ),
      makeEl(ownerDoc, 'div', { class: 'material-studio-actions' }, activeTargetBadge, btnNew, btnUndo, btnRedo, btnSave),
    ),
    makeEl(ownerDoc, 'div', { class: 'material-studio-card' },
      makeEl(ownerDoc, 'div', { class: 'field field-select' }, makeEl(ownerDoc, 'label', {}, tr('materialDoc')), docSelect),
      sceneEntitiesRow,
      makeEl(ownerDoc, 'div', { class: 'material-studio-hero' },
        makeEl(ownerDoc, 'div', { class: 'material-studio-preview-wrap' }, previewCanvas, evReadout, lightingPills),
        makeEl(ownerDoc, 'div', { style: 'display:flex;flex-direction:column;gap:6px;min-width:0;' },
          makeEl(ownerDoc, 'strong', { style: 'font-size:11.5px;color:#cfd6e3;' }, tr('presets')),
          presetRow,
        ),
      ),
    ),
    makeEl(ownerDoc, 'div', { class: 'material-studio-card' },
      makeEl(ownerDoc, 'div', { class: 'material-studio-card-head' },
        makeEl(ownerDoc, 'strong', {}, 'Surface Shader & Finish'),
        btnToggleAllChannels,
      ),
      addSelect('surface', 'surfaceModel', SURFACE_MODELS),
      surfaceGuideBox,
      addColor('baseColorHex', 'baseColor'),
      addSlider('roughness', 'roughness', 0.04, 1.0, 0.01),
      addSlider('metallic', 'metallic', 0.0, 1.0, 0.01),
      addSlider('normalScale', 'normalScale', 0.0, 2.0, 0.05),
      addSlider('clearcoat', 'clearcoat', 0.0, 1.0, 0.01),
      addSlider('clearcoatRoughness', 'clearcoatRoughness', 0.03, 1.0, 0.01),
      addSlider('sheen', 'sheen', 0.0, 1.0, 0.01),
      addSlider('sheenTint', 'sheenTint', 0.0, 1.0, 0.01),
      addSlider('subsurface', 'subsurface', 0.0, 1.0, 0.01),
      addColor('subsurfaceColorHex', 'subsurfaceColor'),
      addSlider('rampCenter', 'rampCenter', 0.1, 0.9, 0.01),
      addSlider('rampSoftness', 'rampSoftness', 0.01, 0.5, 0.01),
      addSlider('rimStrength', 'rimStrength', 0.0, 1.5, 0.02),
    ),
    makeEl(ownerDoc, 'div', { class: 'material-studio-card' },
      makeEl(ownerDoc, 'div', { class: 'material-studio-card-head' },
        makeEl(ownerDoc, 'strong', { class: 'material-studio-subhead' }, tr('cameraSection')),
        makeEl(ownerDoc, 'span', { class: 'material-studio-subtitle' }, 'Live 3D viewport exposure & highlight rolloff'),
      ),
      addSelect('tonemap', 'tonemap', TONEMAP_MODES),
      addSlider('aperture', 'aperture', 1.4, 22.0, 0.2),
      makeEl(ownerDoc, 'div', { class: 'field' }, makeEl(ownerDoc, 'label', { for: shutterId }, tr('shutterInv')), shutterInput, shutterVal),
      addSlider('iso', 'iso', 50, 3200, 50),
      addSlider('compensation', 'compensation', -3.0, 3.0, 0.1),
    ),
  );

  container.append(root);

  function applyPreset(presetId) {
    const preset = MATERIAL_PRESETS.find((p) => p.id === presetId);
    if (!preset) return false;
    state = { ...state, ...preset };
    syncControlsFromState(false);
    pushLivePreviewAndEdit(`Apply preset ${preset.label}`, false);
    return true;
  }

  if (typeof studioApi?.on === 'function') {
    const offDoc = studioApi.on('doc', (ev) => {
      refreshDocList();
      if (ev?.doc && ev.doc.id === activeDocId && ev.doc.root) {
        state = stateFromMaterialRoot(ev.doc.root, state);
        syncControlsFromState();
      }
    });
    if (typeof offDoc === 'function') unsubs.push(offDoc);
  }

  const initialActive = studioApi?.getActiveDocument?.();
  if (initialActive && (initialActive.kind === 'material' || initialActive.path?.endsWith('.material.json'))) {
    activeDocId = initialActive.id;
    activeDocPath = initialActive.path;
    state = stateFromMaterialRoot(initialActive.root, state);
  }
  refreshDocList();
  syncControlsFromState();

  function destroy() {
    while (unsubs.length) {
      const fn = unsubs.pop();
      try { fn(); } catch { /* ignore */ }
    }
    if (root.parentNode) root.parentNode.removeChild(root);
  }

  return {
    destroy,
    unmount: destroy,
    refresh(snapshot) {
      refreshDocList();
      if (snapshot?.root) {
        state = stateFromMaterialRoot(snapshot.root, state);
        syncControlsFromState();
      }
    },
    setDocument(pathOrId, doc) {
      if (doc && typeof doc === 'object') {
        activeDocId = doc.id || pathOrId || null;
        activeDocPath = doc.path || pathOrId || null;
        state = stateFromMaterialRoot(doc.root || doc, state);
        refreshDocList();
        syncControlsFromState();
        return;
      }
      return selectDocument(pathOrId);
    },
    applyPreset,
    getState() {
      return { ...state, activeDocId, activeDocPath };
    },
    setState(patch) {
      state = { ...state, ...patch };
      syncControlsFromState(false);
      pushLivePreviewAndEdit('Update Material State', true);
    },
    onActivate() {
      refreshDocList();
      paintPreview();
    },
    onDeactivate() {},
  };
}
