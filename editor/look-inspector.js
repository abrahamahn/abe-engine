// Look values stay in their renderer-native form. Missing keys use the renderer's default.
const fields = {
  shading: {
    number: 'ramp_center ramp_softness shadow_strength rim_strength rim_power specular specular_size foliage_wrap foliage_translucency texture_softness normal_map_strength lamp_light lamp_glow lamp_foliage water_reflection water_ripples tide brush brush_scale foam sparkle bounce',
    color: 'shadow_tint water_color water_shallow tide_color',
  },
  post: {
    number: 'ambient_occlusion ao_radius outline painterly painterly_far bloom bloom_threshold exposure saturation contrast vignette fog fog_height fog_falloff sharpen sun_shafts',
    color: 'ao_tint outline_color lift gain',
  },
  sky: { number: 'cloud_scale star_density moon_size moon_intensity dusk_warmth dawn_mist', color: 'moon_color', array: 'keys' },
  time: { number: 'hours day_of_year latitude moon_lag_hours moon_declination', vector: 'north' },
  weather: { preset: 'preset', number: 'clouds rain mist wind petals' },
  grass: { number: 'density height width flowers sway', color: 'base tip', array: 'flower_colors' },
  terrain: { number: 'beach patches patch_size', pair: 'rock_slope', color: 'sand wet_sand rock meadow' },
  details: {},
  views: {},
};
const weather = ['clear', 'cloudy', 'overcast', 'mist', 'rain', 'storm', 'petals'];
const title = (key) => (key[0].toUpperCase() + key.slice(1)).replaceAll('_', ' ');

function fieldValue(value, type) {
  if (value === undefined) return '';
  if (typeof value === 'string' || typeof value === 'number') return String(value);
  return JSON.stringify(value, null, type === 'array' ? 2 : 0);
}

// The native picker shows sRGB; the Look may instead store unclamped linear RGB.
function colorSwatch(stored) {
  if (typeof stored === 'string' && /^#[0-9a-f]{6}$/i.test(stored)) return stored;
  if (!Array.isArray(stored) || stored.length !== 3 || !stored.every(Number.isFinite)) return '#ffffff';
  return '#' + stored.map(channel => {
    const linear = Math.min(1, Math.max(0, channel));
    const srgb = linear <= 0.0031308 ? 12.92 * linear : 1.055 * linear ** (1 / 2.4) - 0.055;
    return Math.round(255 * srgb).toString(16).padStart(2, '0');
  }).join('');
}

export function parseLookInput(text, type) {
  const input = text.trim();
  if (!input) return undefined;
  if (type === 'number') {
    const n = Number(input);
    if (!Number.isFinite(n)) throw new Error('Enter a finite number.');
    return n;
  }
  if (type === 'preset' || (type === 'color' && /^#[0-9a-f]{6}$/i.test(input))) return input;
  let value;
  try { value = JSON.parse(input); } catch { throw new Error(type === 'color' ? 'Enter #rrggbb or three linear RGB values in brackets.' : 'Enter an array in brackets.'); }
  if (!Array.isArray(value)) throw new Error('Enter an array in brackets.');
  if (type !== 'array') {
    const count = type === 'pair' ? 2 : 3;
    if (value.length !== count || !value.every(Number.isFinite)) throw new Error(`Enter ${count} finite numbers in brackets.`);
  }
  return value;
}

// The inspected scene keeps rendering behind this asset inspector.
export function drawLookInspector(box, { doc, environment, section, retry, changeSection, change, back, el, t }) {
  box.append(el('div', { class: 'component', 'data-key': 'look-header' },
    el('strong', {}, doc?.path.split('/').pop() || 'Look'),
    el('button', { type: 'button', class: 'mini', onclick: back }, t('backToScene'))));
  if (!doc) { box.append(el('p', {class: 'hint'}, t('lookLoading'))); return; }
  if (doc.error) {
    box.append(el('p', {class: 'look-error'}, doc.error), el('button', {type: 'button', onclick: retry}, t('lookRetry')));
    return;
  }
  for (const issue of doc.issues) box.append(el('p', {class: 'look-error'}, issue.message));
  const sections = el('select', { id: 'look-section', 'aria-label': t('lookSection'), onchange: (e) => changeSection(e.target.value) },
    Object.keys(fields).map(name => el('option', { value: name }, t('look_' + name))));
  sections.value = section;
  box.append(el('div', { class: 'field', 'data-key': 'look-section' }, el('label', { for: sections.id }, t('lookSection')), sections));
  const makeRow = (keys, type) => {
    const key = keys.at(-1);
    const stored = keys.reduce((v, k) => v?.[k], doc.root);
    const id = 'look-field-' + keys.join('-');
    const row = el('div', { class: 'look-field', 'data-key': id });
    const input = type === 'preset'
      ? el('select', {id}, el('option', {value: ''}, t('lookDefault')), weather.map(v => el('option', {value: v}, v)))
      : type === 'array' ? el('textarea', {id, rows: 5, spellcheck: false, placeholder: '[]'})
        : el('input', {id, type: type === 'number' ? 'number' : 'text', step: type === 'number' ? 'any' : null,
          placeholder: type === 'color' ? '#rrggbb / [r, g, b]' : type === 'vector' ? '[x, y, z]' : type === 'pair' ? '[start, full]' : t('lookDefault')});
    input.value = fieldValue(stored, type);
    const error = el('small', { class: 'look-error', 'aria-live': 'polite' });
    input.addEventListener('input', () => { input.setCustomValidity(''); error.textContent = ''; });
    input.addEventListener('change', async () => {
      try {
        if (input.validity.badInput) throw new Error('Enter a finite number.');
        const value = parseLookInput(input.value, type);
        if (JSON.stringify(value) === JSON.stringify(stored)) return;
        const reply = await change(keys, value);
        if (reply?.type === 'error') throw new Error(reply.message);
      } catch (e) {
        error.textContent = e.message;
        input.setCustomValidity(e.message);
        input.reportValidity();
      }
    });
    row.append(el('label', {for: id}, title(key)), input);
    if (type === 'color') {
      const swatch = el('input', {type: 'color', 'aria-label': title(key) + ' (sRGB)',
        onchange: (e) => change(keys, e.target.value)});
      swatch.value = colorSwatch(stored);
      row.append(swatch);
    }
    row.append(el('button', {type: 'button', class: 'mini', disabled: stored === undefined,
      title: t('resetDefault'), 'aria-label': t('resetDefault') + ' · ' + title(key), onclick: () => change(keys, undefined)}, '↶'), error);
    box.append(row);
  };
  if (section === 'time' && environment?.time !== undefined) {
    box.append(el('p', {class: 'hint'}, t('lookSceneTime', environment.time)));
  }
  if (section === 'weather' && environment?.weather) {
    box.append(el('p', {class: 'hint'}, t('lookSceneWeather', environment.weather)));
  }
  if (section === 'details' || section === 'views') makeRow([section], 'array');
  else for (const [type, names] of Object.entries(fields[section])) {
    for (const key of names.split(' ')) makeRow([section, key], type);
  }
}
