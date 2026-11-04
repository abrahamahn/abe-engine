// Ganbate Studio — Event-Driven Sound & Foley DAW Workbench (`editor/audio-daw.js`).
//
// Zero-framework ES module implementing `mountAudioDaw(container, studioApi)` against the
// canonical `StudioWorkbenchApi` contract (`editor/plugin.d.ts`). All buses (`master` + project
// channel buses), SoundBank assets, multi-take round-robin pools, event action sheets, sequences,
// presets, and sidechain ducking rules are 100% data-driven from `.soundbank.json` documents.

import { extractWavPeaks } from './asset-previews.js';
import { applyOps } from './protocol.js';

export const SOUNDBANK_KIND = 'soundbank';
export const SOUNDBANK_VERSION = 1;
export const MASTER_BUS_ID = 'master';
export const DEFAULT_SAMPLE_RATE = 48000;
export const MIN_METER_DB = -72;

const SVG_NS = 'http://www.w3.org/2000/svg';

const I18N_FALLBACK = {
  en: {
    dawTitle: 'Sound & Foley DAW',
    newSoundbank: '+ SoundBank',
    undo: 'Undo',
    redo: 'Redo',
    save: 'Save',
    applyPreset: 'Apply Preset',
    browserTitle: 'SoundBank & Pools',
    timelineTitle: 'Event Action Sheet',
    consoleTitle: 'Mixing Console',
    dspTitle: 'DSP Insert Rack',
    triggerEvent: '▶ Trigger',
    stopAudio: '■ Stop',
    playSequence: '▶ Sequence',
    bounceWav: 'Bounce 48kHz WAV',
    placeAtContact: '+ Layer',
    registerAndPlace: '+ Register',
  },
  ko: {
    dawTitle: '사운드 & 폴리 DAW',
    newSoundbank: '+ 사운드뱅크',
    undo: '실행 취소',
    redo: '다시 실행',
    save: '저장',
    applyPreset: '프리셋 적용',
    browserTitle: '사운드뱅크 & 풀',
    timelineTitle: '이벤트 액션 시트',
    consoleTitle: '믹싱 콘솔',
    dspTitle: 'DSP 인서트 랙',
    triggerEvent: '▶ 재생',
    stopAudio: '■ 정지',
    playSequence: '▶ 시퀀스',
    bounceWav: '48kHz WAV 바운스',
    placeAtContact: '+ 레이어',
    registerAndPlace: '+ 등록',
  },
};

// ---- Pure Defaults, Schema Normalization & Validation ---------------------------------------

export function createDefaultStrip() {
  return {
    bypass: false,
    inputGainDb: 0,
    pan: 0,
    stereoMode: 'stereo',
    stereoWidthPct: 100,
    highPass: { enabled: false, frequencyHz: 20, q: 0.707 },
    lowPass: { enabled: false, frequencyHz: 20000, q: 0.707 },
    eq: {
      enabled: false,
      lowGainDb: 0,
      lowFreqHz: 120,
      midGainDb: 0,
      midFreqHz: 1200,
      midQ: 1.0,
      highGainDb: 0,
      highFreqHz: 6500,
    },
    compressor: {
      enabled: false,
      thresholdDb: -18,
      kneeDb: 6,
      ratio: 3,
      attackSec: 0.008,
      releaseSec: 0.14,
      makeupGainDb: 0,
      mix: 1,
    },
    overdrive: {
      enabled: false,
      drive: 0,
      toneHz: 6500,
      oversample: '2x',
      outputGainDb: 0,
      mix: 0,
    },
    delay: {
      enabled: false,
      timeMs: 140,
      feedback: 0.22,
      lowpassHz: 4200,
      mix: 0,
    },
    reverb: {
      enabled: false,
      roomSize: 0.35,
      damping: 0.45,
      preDelayMs: 12,
      mix: 0,
    },
  };
}

export function randomHexId(len = 32) {
  const chars = '0123456789abcdef';
  if (globalThis.crypto?.getRandomValues) {
    const bytes = new Uint8Array(len / 2);
    globalThis.crypto.getRandomValues(bytes);
    return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
  }
  let out = '';
  for (let i = 0; i < len; i++) out += chars[Math.floor(Math.random() * 16)];
  return out;
}

/** Builds a clean genre-neutral starter `.soundbank.json` document. */
export function createStarterSoundBank({
  id = '8b4f1d6209c3471aa520e374b8c9d012',
  name = 'Project SoundBank',
} = {}) {
  const neutralMaster = createDefaultStrip();
  const punchMaster = createDefaultStrip();
  punchMaster.compressor = {
    enabled: true,
    thresholdDb: -16,
    kneeDb: 6,
    ratio: 3.5,
    attackSec: 0.006,
    releaseSec: 0.12,
    makeupGainDb: 2.0,
    mix: 0.85,
  };
  punchMaster.eq = {
    enabled: true,
    lowGainDb: 1.5,
    lowFreqHz: 110,
    midGainDb: 1.0,
    midFreqHz: 1800,
    midQ: 1.1,
    highGainDb: 2.0,
    highFreqHz: 7500,
  };
  const warmMaster = createDefaultStrip();
  warmMaster.overdrive = {
    enabled: true,
    drive: 24,
    toneHz: 5800,
    oversample: '2x',
    outputGainDb: -0.5,
    mix: 0.32,
  };
  warmMaster.reverb = {
    enabled: true,
    roomSize: 0.42,
    damping: 0.5,
    preDelayMs: 14,
    mix: 0.14,
  };

  return {
    ganbate: SOUNDBANK_KIND,
    version: SOUNDBANK_VERSION,
    id,
    name,
    sampleRate: DEFAULT_SAMPLE_RATE,
    buses: {
      sfx: {
        order: 0,
        label: 'SFX',
        role: 'channel',
        color: '#60a5fa',
        volume: 1.0,
        muted: false,
        solo: false,
        strip: createDefaultStrip(),
      },
      foley: {
        order: 1,
        label: 'Foley',
        role: 'channel',
        color: '#4ade80',
        volume: 1.0,
        muted: false,
        solo: false,
        strip: createDefaultStrip(),
      },
      ui: {
        order: 2,
        label: 'UI',
        role: 'channel',
        color: '#38bdf8',
        volume: 0.9,
        muted: false,
        solo: false,
        strip: createDefaultStrip(),
      },
      music: {
        order: 3,
        label: 'Music',
        role: 'channel',
        color: '#f472b6',
        volume: 0.8,
        muted: false,
        solo: false,
        strip: createDefaultStrip(),
      },
      dialogue: {
        order: 4,
        label: 'Dialogue',
        role: 'channel',
        color: '#c084fc',
        volume: 1.0,
        muted: false,
        solo: false,
        strip: createDefaultStrip(),
      },
      ambience: {
        order: 5,
        label: 'Ambience',
        role: 'channel',
        color: '#2dd4bf',
        volume: 0.75,
        muted: false,
        solo: false,
        strip: createDefaultStrip(),
      },
      master: {
        order: 6,
        label: 'Master',
        role: 'master',
        color: '#e8b44a',
        volume: 1.0,
        muted: false,
        solo: false,
        strip: createDefaultStrip(),
      },
    },
    assets: {
      foley_impact_01: {
        order: 0,
        poolId: 'foley_impact',
        label: 'Surface Impact · Take 1',
        bus: 'foley',
        path: 'content/audio/foley_impact_01.wav',
        takeIndex: 1,
        takeCount: 2,
        defaultDurationMs: 220,
        transientOffsetMs: 18,
        tags: ['foley', 'impact'],
      },
      foley_impact_02: {
        order: 1,
        poolId: 'foley_impact',
        label: 'Surface Impact · Take 2',
        bus: 'foley',
        path: 'content/audio/foley_impact_02.wav',
        takeIndex: 2,
        takeCount: 2,
        defaultDurationMs: 230,
        transientOffsetMs: 16,
        tags: ['foley', 'impact'],
      },
      whoosh_pre_01: {
        order: 2,
        poolId: 'whoosh_pre',
        label: 'Motion Whoosh · Take 1',
        bus: 'sfx',
        path: 'content/audio/whoosh_pre_01.wav',
        takeIndex: 1,
        takeCount: 1,
        defaultDurationMs: 160,
        transientOffsetMs: 60,
        tags: ['motion', 'whoosh'],
      },
    },
    containers: {
      foley_impact: {
        order: 0,
        label: 'Surface Impact Pool',
        bus: 'foley',
        assetIds: ['foley_impact_01', 'foley_impact_02'],
        roundRobinMode: 'shuffle-no-repeat',
        pitchJitterCents: 35,
        gainJitterDb: 0.8,
      },
      whoosh_pre: {
        order: 1,
        label: 'Motion Whoosh Pool',
        bus: 'sfx',
        assetIds: ['whoosh_pre_01'],
        roundRobinMode: 'sequential',
        pitchJitterCents: 20,
        gainJitterDb: 0.5,
      },
    },
    events: {
      evt_surface_hit: {
        order: 0,
        cueId: 'surface_hit',
        label: 'Surface Impact',
        bus: 'foley',
        durationMs: 520,
        contactMarkerMs: 140,
        contactLabel: 'Surface Contact (F8)',
        description: 'Pre-impact motion whoosh leading into contact transient',
        layers: {
          lyr_pre_whoosh: {
            order: 0,
            label: 'Pre-Impact Whoosh',
            assetId: 'whoosh_pre_01',
            poolId: 'whoosh_pre',
            bus: 'sfx',
            enabled: true,
            solo: false,
            startMs: 40,
            durationMs: 140,
            trimStartMs: 0,
            fadeInMs: 25,
            fadeOutMs: 40,
            fadeCurve: 'cosine',
            gainDb: -4.5,
            pan: -0.1,
            pitchSemitones: 0,
            pitchJitterCents: 20,
            gainJitterDb: 0.5,
            color: '#60a5fa',
            strip: createDefaultStrip(),
          },
          lyr_contact: {
            order: 1,
            label: 'Contact Foley',
            poolId: 'foley_impact',
            bus: 'foley',
            enabled: true,
            solo: false,
            startMs: 122,
            durationMs: 210,
            trimStartMs: 0,
            fadeInMs: 4,
            fadeOutMs: 65,
            fadeCurve: 's-curve',
            gainDb: 0,
            pan: 0.05,
            pitchSemitones: 0,
            pitchJitterCents: 30,
            gainJitterDb: 0.8,
            color: '#4ade80',
            strip: createDefaultStrip(),
          },
        },
      },
    },
    sequences: {
      seq_double_hit: {
        order: 0,
        label: 'Double Impact Preview',
        steps: [
          { eventId: 'evt_surface_hit', atMs: 0, label: 'Primary Hit' },
          { eventId: 'evt_surface_hit', atMs: 320, label: 'Follow-up Hit' },
        ],
      },
    },
    presets: {
      'neutral-bypass': {
        order: 0,
        label: 'Neutral Reference',
        description: 'Flat 0 dB gain staging with insert processors bypassed.',
        buses: { master: { volume: 1.0, strip: neutralMaster } },
      },
      'broadcast-punch': {
        order: 1,
        label: 'Broadcast Punch',
        description: 'Tight bus glue compression and presence lift for small speakers.',
        buses: { master: { volume: 1.0, strip: punchMaster } },
      },
      'warm-analog': {
        order: 2,
        label: 'Warm Saturation & Room',
        description: 'Subtle 2x oversampled harmonic drive and short room ambience.',
        buses: { master: { volume: 0.98, strip: warmMaster } },
      },
    },
    dynamics: {
      enabled: true,
      tension: 0,
      gracefulExitMs: 180,
      ducking: {
        dialogue_priority: {
          sourceBus: 'dialogue',
          targetBuses: ['sfx', 'foley', 'music', 'ambience'],
          enabled: true,
          amount: 0.35,
          attackMs: 20,
          releaseMs: 200,
        },
      },
    },
  };
}

function toKeyedMap(collection) {
  const out = {};
  if (!collection) return out;
  if (Array.isArray(collection)) {
    collection.forEach((item, idx) => {
      if (item && typeof item === 'object' && typeof item.id === 'string' && item.id) {
        const clone = structuredClone(item);
        delete clone.id;
        if (typeof clone.order !== 'number') clone.order = idx;
        out[item.id] = clone;
      }
    });
    return out;
  }
  if (typeof collection === 'object') {
    let idx = 0;
    for (const [id, item] of Object.entries(collection)) {
      if (item && typeof item === 'object') {
        const clone = structuredClone(item);
        if (typeof clone.order !== 'number') clone.order = idx;
        out[id] = clone;
        idx += 1;
      }
    }
  }
  return out;
}

export function normalizeStrip(raw) {
  const d = createDefaultStrip();
  if (!raw || typeof raw !== 'object') return d;
  return {
    bypass: Boolean(raw.bypass),
    inputGainDb: finiteClamp(raw.inputGainDb, -60, 24, 0),
    pan: finiteClamp(raw.pan, -1, 1, 0),
    stereoMode: ['stereo', 'mono', 'left', 'right'].includes(raw.stereoMode)
      ? raw.stereoMode
      : 'stereo',
    stereoWidthPct: finiteClamp(raw.stereoWidthPct, 0, 200, 100),
    highPass: {
      enabled: Boolean(raw.highPass?.enabled),
      frequencyHz: finiteClamp(raw.highPass?.frequencyHz, 15, 22000, d.highPass.frequencyHz),
      q: finiteClamp(raw.highPass?.q, 0.1, 18, d.highPass.q),
    },
    lowPass: {
      enabled: Boolean(raw.lowPass?.enabled),
      frequencyHz: finiteClamp(raw.lowPass?.frequencyHz, 20, 24000, d.lowPass.frequencyHz),
      q: finiteClamp(raw.lowPass?.q, 0.1, 18, d.lowPass.q),
    },
    eq: {
      enabled: Boolean(raw.eq?.enabled),
      lowGainDb: finiteClamp(raw.eq?.lowGainDb, -24, 24, 0),
      lowFreqHz: finiteClamp(raw.eq?.lowFreqHz, 20, 1000, 120),
      midGainDb: finiteClamp(raw.eq?.midGainDb, -24, 24, 0),
      midFreqHz: finiteClamp(raw.eq?.midFreqHz, 120, 12000, 1200),
      midQ: finiteClamp(raw.eq?.midQ, 0.1, 18, 1.0),
      highGainDb: finiteClamp(raw.eq?.highGainDb, -24, 24, 0),
      highFreqHz: finiteClamp(raw.eq?.highFreqHz, 1000, 20000, 6500),
    },
    compressor: {
      enabled: Boolean(raw.compressor?.enabled),
      thresholdDb: finiteClamp(raw.compressor?.thresholdDb, -60, 0, -18),
      kneeDb: finiteClamp(raw.compressor?.kneeDb, 0, 40, 6),
      ratio: finiteClamp(raw.compressor?.ratio, 1, 20, 3),
      attackSec: finiteClamp(raw.compressor?.attackSec, 0.0001, 1, 0.008),
      releaseSec: finiteClamp(raw.compressor?.releaseSec, 0.005, 2.5, 0.14),
      makeupGainDb: finiteClamp(raw.compressor?.makeupGainDb, -12, 24, 0),
      mix: finiteClamp(raw.compressor?.mix, 0, 1, 1),
    },
    overdrive: {
      enabled: Boolean(raw.overdrive?.enabled),
      drive: finiteClamp(raw.overdrive?.drive, 0, 100, 0),
      toneHz: finiteClamp(raw.overdrive?.toneHz, 400, 20000, 6500),
      oversample: ['none', '2x', '4x'].includes(raw.overdrive?.oversample)
        ? raw.overdrive.oversample
        : '2x',
      outputGainDb: finiteClamp(raw.overdrive?.outputGainDb, -24, 12, 0),
      mix: finiteClamp(raw.overdrive?.mix, 0, 1, 0),
    },
    delay: {
      enabled: Boolean(raw.delay?.enabled),
      timeMs: finiteClamp(raw.delay?.timeMs, 1, 2000, 140),
      feedback: finiteClamp(raw.delay?.feedback, 0, 0.95, 0.22),
      lowpassHz: finiteClamp(raw.delay?.lowpassHz, 300, 20000, 4200),
      mix: finiteClamp(raw.delay?.mix, 0, 1, 0),
    },
    reverb: {
      enabled: Boolean(raw.reverb?.enabled),
      roomSize: finiteClamp(raw.reverb?.roomSize, 0, 1, 0.35),
      damping: finiteClamp(raw.reverb?.damping, 0, 1, 0.45),
      preDelayMs: finiteClamp(raw.reverb?.preDelayMs, 0, 250, 12),
      mix: finiteClamp(raw.reverb?.mix, 0, 1, 0),
    },
  };
}

/** Normalizes any raw `.soundbank.json` object (keyed maps or arrays) into canonical form. */
export function normalizeSoundBankRoot(raw) {
  if (!raw || typeof raw !== 'object') return createStarterSoundBank();
  const busesRaw = toKeyedMap(raw.buses);
  const buses = {};
  for (const [id, b] of Object.entries(busesRaw)) {
    buses[id] = {
      order: typeof b.order === 'number' ? b.order : 0,
      label: String(b.label || id),
      role: id === MASTER_BUS_ID ? 'master' : 'channel',
      color: String(b.color || (id === MASTER_BUS_ID ? '#e8b44a' : '#60a5fa')),
      volume: finiteClamp(b.volume, 0, 2, 1),
      muted: Boolean(b.muted),
      solo: Boolean(b.solo),
      strip: normalizeStrip(b.strip),
    };
  }
  if (!buses[MASTER_BUS_ID]) {
    buses[MASTER_BUS_ID] = {
      order: Object.keys(buses).length,
      label: 'Master',
      role: 'master',
      color: '#e8b44a',
      volume: 1,
      muted: false,
      solo: false,
      strip: createDefaultStrip(),
    };
  }
  const firstChannelBus =
    Object.keys(buses).find((id) => id !== MASTER_BUS_ID) || MASTER_BUS_ID;

  const assets = {};
  for (const [id, a] of Object.entries(toKeyedMap(raw.assets))) {
    assets[id] = {
      order: typeof a.order === 'number' ? a.order : 0,
      poolId: String(a.poolId || id),
      label: String(a.label || id),
      bus: buses[a.bus] ? a.bus : firstChannelBus,
      path: String(a.path || `content/audio/${id}.wav`),
      takeIndex: Math.max(1, Math.round(Number(a.takeIndex) || 1)),
      takeCount: Math.max(1, Math.round(Number(a.takeCount) || 1)),
      defaultDurationMs: finiteClamp(a.defaultDurationMs, 10, 600000, 240),
      transientOffsetMs: finiteClamp(a.transientOffsetMs, 0, 60000, 0),
      tags: Array.isArray(a.tags) ? a.tags.map(String) : [],
    };
  }

  const containers = {};
  for (const [id, c] of Object.entries(toKeyedMap(raw.containers))) {
    containers[id] = {
      order: typeof c.order === 'number' ? c.order : 0,
      label: String(c.label || id),
      bus: buses[c.bus] ? c.bus : firstChannelBus,
      assetIds: Array.isArray(c.assetIds) ? c.assetIds.map(String) : [],
      roundRobinMode: c.roundRobinMode === 'sequential' ? 'sequential' : 'shuffle-no-repeat',
      pitchJitterCents: finiteClamp(c.pitchJitterCents, 0, 1200, 0),
      gainJitterDb: finiteClamp(c.gainJitterDb, 0, 24, 0),
    };
  }

  const events = {};
  for (const [id, ev] of Object.entries(toKeyedMap(raw.events))) {
    const durationMs = finiteClamp(ev.durationMs, 40, 600000, 600);
    const layers = {};
    for (const [lid, l] of Object.entries(toKeyedMap(ev.layers))) {
      const lDur = finiteClamp(l.durationMs, 5, 600000, 220);
      const layerObj = {
        order: typeof l.order === 'number' ? l.order : 0,
        label: String(l.label || lid),
        bus: buses[l.bus] ? l.bus : firstChannelBus,
        enabled: l.enabled !== false,
        solo: Boolean(l.solo),
        startMs: finiteClamp(l.startMs, 0, durationMs, 0),
        durationMs: lDur,
        trimStartMs: finiteClamp(l.trimStartMs, 0, 600000, 0),
        fadeInMs: finiteClamp(l.fadeInMs, 0, lDur, 0),
        fadeOutMs: finiteClamp(l.fadeOutMs, 0, lDur, 20),
        fadeCurve: ['linear', 'cosine', 's-curve'].includes(l.fadeCurve) ? l.fadeCurve : 'cosine',
        gainDb: finiteClamp(l.gainDb, -60, 24, 0),
        pan: finiteClamp(l.pan, -1, 1, 0),
        pitchSemitones: finiteClamp(l.pitchSemitones, -24, 24, 0),
        pitchJitterCents: finiteClamp(l.pitchJitterCents, 0, 1200, 0),
        gainJitterDb: finiteClamp(l.gainJitterDb, 0, 24, 0),
        color: String(l.color || buses[l.bus]?.color || '#60a5fa'),
        strip: normalizeStrip(l.strip),
      };
      if (l.assetId) layerObj.assetId = String(l.assetId);
      if (l.poolId) layerObj.poolId = String(l.poolId);
      layers[lid] = layerObj;
    }
    const evObj = {
      order: typeof ev.order === 'number' ? ev.order : 0,
      label: String(ev.label || id),
      bus: buses[ev.bus] ? ev.bus : firstChannelBus,
      durationMs,
      contactMarkerMs: finiteClamp(ev.contactMarkerMs, 0, durationMs, Math.min(120, durationMs)),
      contactLabel: String(ev.contactLabel || 'Contact'),
      description: String(ev.description || ''),
      layers,
    };
    if (ev.cueId) evObj.cueId = String(ev.cueId);
    events[id] = evObj;
  }

  const sequences = {};
  for (const [id, seq] of Object.entries(toKeyedMap(raw.sequences))) {
    sequences[id] = {
      order: typeof seq.order === 'number' ? seq.order : 0,
      label: String(seq.label || id),
      steps: Array.isArray(seq.steps)
        ? seq.steps
            .filter((s) => s && typeof s.eventId === 'string')
            .map((s) => ({
              eventId: s.eventId,
              atMs: finiteClamp(s.atMs, 0, 3600000, 0),
              label: String(s.label || s.eventId),
            }))
        : [],
    };
  }

  const presets = {};
  for (const [id, p] of Object.entries(toKeyedMap(raw.presets))) {
    const pBuses = {};
    if (p.buses && typeof p.buses === 'object') {
      for (const [bid, patch] of Object.entries(p.buses)) {
        if (patch && typeof patch === 'object') {
          const entry = { strip: normalizeStrip(patch.strip) };
          if (typeof patch.volume === 'number' && Number.isFinite(patch.volume)) {
            entry.volume = finiteClamp(patch.volume, 0, 2, 1);
          }
          pBuses[bid] = entry;
        }
      }
    }
    presets[id] = {
      order: typeof p.order === 'number' ? p.order : 0,
      label: String(p.label || id),
      description: String(p.description || ''),
      buses: pBuses,
    };
  }

  const ducking = {};
  for (const [id, r] of Object.entries(toKeyedMap(raw.dynamics?.ducking))) {
    ducking[id] = {
      sourceBus: String(r.sourceBus || firstChannelBus),
      targetBuses: Array.isArray(r.targetBuses) ? r.targetBuses.map(String) : [],
      enabled: r.enabled !== false,
      amount: finiteClamp(r.amount, 0, 1, 0.35),
      attackMs: finiteClamp(r.attackMs, 0.5, 5000, 20),
      releaseMs: finiteClamp(r.releaseMs, 1, 10000, 200),
    };
  }

  return {
    ganbate: SOUNDBANK_KIND,
    version: SOUNDBANK_VERSION,
    id:
      typeof raw.id === 'string' && /^[0-9a-f]{16}([0-9a-f]{16})?$/.test(raw.id)
        ? raw.id
        : '8b4f1d6209c3471aa520e374b8c9d012',
    name: String(raw.name || 'Project SoundBank'),
    sampleRate: Math.round(finiteClamp(raw.sampleRate, 8000, 192000, DEFAULT_SAMPLE_RATE)),
    buses,
    assets,
    containers,
    events,
    sequences,
    presets,
    dynamics: {
      enabled: Boolean(raw.dynamics?.enabled),
      tension: finiteClamp(raw.dynamics?.tension, 0, 1, 0),
      gracefulExitMs: finiteClamp(raw.dynamics?.gracefulExitMs, 0, 30000, 180),
      ducking,
    },
  };
}

export function validateSoundBankRoot(root) {
  const issues = [];
  if (!root || typeof root !== 'object') return ['root must be an object'];
  if (root.ganbate !== SOUNDBANK_KIND) issues.push('ganbate must equal "soundbank"');
  if (root.version !== SOUNDBANK_VERSION) issues.push('version must equal 1');
  if (typeof root.id !== 'string' || !/^[0-9a-f]{16}([0-9a-f]{16})?$/.test(root.id)) {
    issues.push('id must be a 16- or 32-digit lowercase hex id');
  }
  if (!root.buses || !root.buses[MASTER_BUS_ID] || root.buses[MASTER_BUS_ID].role !== 'master') {
    issues.push('buses must include a master bus');
  }
  for (const [aid, a] of Object.entries(root.assets || {})) {
    if (!root.buses?.[a.bus]) issues.push(`asset ${aid} references unknown bus ${a.bus}`);
    if (!a.path || String(a.path).includes('..')) issues.push(`asset ${aid} has invalid path`);
  }
  for (const [cid, c] of Object.entries(root.containers || {})) {
    if (!root.buses?.[c.bus]) issues.push(`container ${cid} references unknown bus ${c.bus}`);
    if (!Array.isArray(c.assetIds) || c.assetIds.length === 0) {
      issues.push(`container ${cid} must list at least one assetId`);
    }
  }
  for (const [eid, ev] of Object.entries(root.events || {})) {
    if (!root.buses?.[ev.bus]) issues.push(`event ${eid} references unknown bus ${ev.bus}`);
    for (const [lid, l] of Object.entries(ev.layers || {})) {
      if (!root.buses?.[l.bus]) issues.push(`layer ${eid}/${lid} references unknown bus ${l.bus}`);
      if (!l.assetId && !l.poolId) issues.push(`layer ${eid}/${lid} needs assetId or poolId`);
    }
  }
  return issues;
}

export function orderedBuses(root) {
  return Object.entries(root?.buses || {})
    .map(([id, spec]) => ({ id, ...spec }))
    .sort((a, b) => {
      const am = a.role === 'master' || a.id === MASTER_BUS_ID ? 1 : 0;
      const bm = b.role === 'master' || b.id === MASTER_BUS_ID ? 1 : 0;
      if (am !== bm) return am - bm;
      if ((a.order ?? 0) !== (b.order ?? 0)) return (a.order ?? 0) - (b.order ?? 0);
      return a.id.localeCompare(b.id);
    });
}

export function orderedEvents(root) {
  return Object.entries(root?.events || {})
    .map(([id, ev]) => ({ id, ...ev }))
    .sort((a, b) => (a.order ?? 0) - (b.order ?? 0) || a.id.localeCompare(b.id));
}

export function orderedLayers(event) {
  return Object.entries(event?.layers || {})
    .map(([id, l]) => ({ id, ...l }))
    .sort((a, b) => (a.order ?? 0) - (b.order ?? 0) || a.id.localeCompare(b.id));
}

export function orderedContainers(root) {
  return Object.entries(root?.containers || {})
    .map(([id, c]) => ({ id, ...c }))
    .sort((a, b) => (a.order ?? 0) - (b.order ?? 0) || a.id.localeCompare(b.id));
}

export function orderedAssets(root) {
  return Object.entries(root?.assets || {})
    .map(([id, a]) => ({ id, ...a }))
    .sort((a, b) => (a.order ?? 0) - (b.order ?? 0) || a.id.localeCompare(b.id));
}

export function orderedSequences(root) {
  return Object.entries(root?.sequences || {})
    .map(([id, s]) => ({ id, ...s }))
    .sort((a, b) => (a.order ?? 0) - (b.order ?? 0) || a.id.localeCompare(b.id));
}

export function orderedPresets(root) {
  return Object.entries(root?.presets || {})
    .map(([id, p]) => ({ id, ...p }))
    .sort((a, b) => (a.order ?? 0) - (b.order ?? 0) || a.id.localeCompare(b.id));
}

// ---- Pure DSP Math, Metering, QOA/WAV Codecs & Offline Bounce -------------------------------

function finiteClamp(v, min, max, fallback) {
  const n = Number(v);
  if (!Number.isFinite(n)) return fallback;
  return Math.min(max, Math.max(min, n));
}

export function dbToLinear(db) {
  const n = Number(db);
  if (!Number.isFinite(n) || n <= MIN_METER_DB) return 0;
  return 10 ** (n / 20);
}

export function linearToDb(gain) {
  const n = Number(gain);
  if (!Number.isFinite(n) || n <= 1e-6) return MIN_METER_DB;
  return Math.max(MIN_METER_DB, 20 * Math.log10(n));
}

/** Computes `[[ll, rl], [lr, rr]]` 2x2 Mid/Side stereo routing & width matrix (`0%..200%`). */
export function computeMidSideMatrix(stereoMode = 'stereo', widthPct = 100) {
  if (stereoMode === 'mono') return [[0.5, 0.5], [0.5, 0.5]];
  if (stereoMode === 'left') return [[1, 0], [1, 0]];
  if (stereoMode === 'right') return [[0, 1], [0, 1]];
  const w = finiteClamp(widthPct, 0, 200, 100) / 100;
  const diag = 0.5 * (1 + w);
  const cross = 0.5 * (1 - w);
  return [[diag, cross], [cross, diag]];
}

function biquadCoeffs(kind, fs, freqHz, q, gainDb = 0) {
  const f0 = Math.min(fs * 0.495, Math.max(12, freqHz));
  const qv = Math.min(24, Math.max(0.1, q || 0.707));
  const w0 = (2 * Math.PI * f0) / fs;
  const cw = Math.cos(w0);
  const sw = Math.sin(w0);

  let b0 = 1, b1 = 0, b2 = 0, a0 = 1, a1 = 0, a2 = 0;
  if (kind === 'highpass') {
    const alpha = sw / (2 * qv);
    b0 = (1 + cw) * 0.5;
    b1 = -(1 + cw);
    b2 = (1 + cw) * 0.5;
    a0 = 1 + alpha;
    a1 = -2 * cw;
    a2 = 1 - alpha;
  } else if (kind === 'lowpass') {
    const alpha = sw / (2 * qv);
    b0 = (1 - cw) * 0.5;
    b1 = 1 - cw;
    b2 = (1 - cw) * 0.5;
    a0 = 1 + alpha;
    a1 = -2 * cw;
    a2 = 1 - alpha;
  } else if (kind === 'peaking') {
    if (Math.abs(gainDb) < 1e-6) return { b0: 1, b1: 0, b2: 0, a1: 0, a2: 0 };
    const A = 10 ** (gainDb / 40);
    const alpha = sw / (2 * qv);
    b0 = 1 + alpha * A;
    b1 = -2 * cw;
    b2 = 1 - alpha * A;
    a0 = 1 + alpha / A;
    a1 = -2 * cw;
    a2 = 1 - alpha / A;
  } else if (kind === 'lowshelf') {
    if (Math.abs(gainDb) < 1e-6) return { b0: 1, b1: 0, b2: 0, a1: 0, a2: 0 };
    const A = 10 ** (gainDb / 40);
    const alpha = sw * 0.5 * Math.SQRT2;
    const tsa = 2 * Math.sqrt(A) * alpha;
    b0 = A * ((A + 1) - (A - 1) * cw + tsa);
    b1 = 2 * A * ((A - 1) - (A + 1) * cw);
    b2 = A * ((A + 1) - (A - 1) * cw - tsa);
    a0 = (A + 1) + (A - 1) * cw + tsa;
    a1 = -2 * ((A - 1) + (A + 1) * cw);
    a2 = (A + 1) + (A - 1) * cw - tsa;
  } else if (kind === 'highshelf') {
    if (Math.abs(gainDb) < 1e-6) return { b0: 1, b1: 0, b2: 0, a1: 0, a2: 0 };
    const A = 10 ** (gainDb / 40);
    const alpha = sw * 0.5 * Math.SQRT2;
    const tsa = 2 * Math.sqrt(A) * alpha;
    b0 = A * ((A + 1) + (A - 1) * cw + tsa);
    b1 = -2 * A * ((A - 1) + (A + 1) * cw);
    b2 = A * ((A + 1) + (A - 1) * cw - tsa);
    a0 = (A + 1) - (A - 1) * cw + tsa;
    a1 = 2 * ((A - 1) - (A + 1) * cw);
    a2 = (A + 1) - (A - 1) * cw - tsa;
  }
  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 };
}

function biquadMagDb(c, fs, freqHz) {
  const w = (2 * Math.PI * Math.min(fs * 0.499, Math.max(1, freqHz))) / fs;
  const c1 = Math.cos(w), s1 = Math.sin(w);
  const c2 = Math.cos(2 * w), s2 = Math.sin(2 * w);
  const nr = c.b0 + c.b1 * c1 + c.b2 * c2;
  const ni = -(c.b1 * s1 + c.b2 * s2);
  const dr = 1 + c.a1 * c1 + c.a2 * c2;
  const di = -(c.a1 * s1 + c.a2 * s2);
  const num = nr * nr + ni * ni;
  const den = Math.max(1e-24, dr * dr + di * di);
  return 10 * Math.log10(Math.max(1e-24, num / den));
}

/** Evaluates HPF + LPF + 3-Band Parametric EQ magnitude response (dB) across `freqsHz`. */
export function evaluateStripResponseDb(strip, freqsHz, sampleRate = DEFAULT_SAMPLE_RATE) {
  if (!strip || strip.bypass) return freqsHz.map(() => 0);
  const fs = Math.max(8000, sampleRate);
  const stages = [];
  if (strip.highPass?.enabled) {
    stages.push(biquadCoeffs('highpass', fs, strip.highPass.frequencyHz, strip.highPass.q));
  }
  if (strip.lowPass?.enabled) {
    stages.push(biquadCoeffs('lowpass', fs, strip.lowPass.frequencyHz, strip.lowPass.q));
  }
  if (strip.eq?.enabled) {
    stages.push(biquadCoeffs('lowshelf', fs, strip.eq.lowFreqHz, 0.707, strip.eq.lowGainDb));
    stages.push(biquadCoeffs('peaking', fs, strip.eq.midFreqHz, strip.eq.midQ, strip.eq.midGainDb));
    stages.push(biquadCoeffs('highshelf', fs, strip.eq.highFreqHz, 0.707, strip.eq.highGainDb));
  }
  return freqsHz.map((f) => stages.reduce((acc, st) => acc + biquadMagDb(st, fs, f), 0));
}

export function overdriveTransferSample(x, drive = 0) {
  const d = finiteClamp(drive, 0, 100, 0) / 100;
  const k = 1 + d * 19;
  const norm = Math.max(1e-6, Math.tanh(k));
  const asym = 0.08 * d * x * x;
  return (Math.tanh(k * x + asym) - Math.tanh(asym)) / norm;
}

export function evaluateFadeCurve(t, curve = 'cosine') {
  const x = finiteClamp(t, 0, 1, 0);
  if (curve === 'linear') return x;
  if (curve === 's-curve') return x * x * (3 - 2 * x);
  return 0.5 * (1 - Math.cos(Math.PI * x));
}

export function clipFadeEnvelope(frameIdx, totalFrames, fadeInFrames, fadeOutFrames, curve) {
  if (totalFrames <= 0 || frameIdx < 0 || frameIdx >= totalFrames) return 0;
  let g = 1;
  if (fadeInFrames > 0 && frameIdx < fadeInFrames) {
    g *= evaluateFadeCurve(frameIdx / fadeInFrames, curve);
  }
  if (fadeOutFrames > 0 && frameIdx + fadeOutFrames > totalFrames) {
    const rem = Math.max(0, totalFrames - 1 - frameIdx);
    g *= evaluateFadeCurve(rem / fadeOutFrames, curve);
  }
  return g;
}

export function analyzeStereoFrames(left, right, gainReductionDb = 0) {
  const n = Math.min(left?.length || 0, right?.length || 0);
  if (n === 0) {
    return {
      peakLDb: MIN_METER_DB,
      peakRDb: MIN_METER_DB,
      rmsLDb: MIN_METER_DB,
      rmsRDb: MIN_METER_DB,
      correlation: 1,
      clipped: false,
      gainReductionDb,
    };
  }
  let peakL = 0, peakR = 0, sqL = 0, sqR = 0, lr = 0;
  for (let i = 0; i < n; i++) {
    const l = Number.isFinite(left[i]) ? left[i] : 0;
    const r = Number.isFinite(right[i]) ? right[i] : 0;
    const al = Math.abs(l), ar = Math.abs(r);
    if (al > peakL) peakL = al;
    if (ar > peakR) peakR = ar;
    sqL += l * l;
    sqR += r * r;
    lr += l * r;
  }
  const denom = Math.sqrt(sqL * sqR);
  const correlation = denom > 1e-12 ? Math.max(-1, Math.min(1, lr / denom)) : 1;
  return {
    peakLDb: linearToDb(peakL),
    peakRDb: linearToDb(peakR),
    rmsLDb: linearToDb(Math.sqrt(sqL / n)),
    rmsRDb: linearToDb(Math.sqrt(sqR / n)),
    correlation,
    clipped: peakL >= 1 || peakR >= 1,
    gainReductionDb,
  };
}

// Pure-JS QOA v1.0 decoder so `.qoa` SoundBank files decode directly into Web Audio & Node tests!
const QOA_DEQUANT_TAB = [
  [1, -1, 3, -3, 5, -5, 7, -7],
  [5, -5, 18, -18, 32, -32, 49, -49],
  [16, -16, 53, -53, 95, -95, 147, -147],
  [34, -34, 113, -113, 203, -203, 315, -315],
  [63, -63, 210, -210, 378, -378, 588, -588],
  [104, -104, 345, -345, 621, -621, 966, -966],
  [158, -158, 528, -528, 950, -950, 1477, -1477],
  [228, -228, 760, -760, 1368, -1368, 2128, -2128],
  [316, -316, 1053, -1053, 1895, -1895, 2947, -2947],
  [422, -422, 1405, -1405, 2529, -2529, 3934, -3934],
  [548, -548, 1828, -1828, 3290, -3290, 5117, -5117],
  [696, -696, 2320, -2320, 4176, -4176, 6496, -6496],
  [868, -868, 2893, -2893, 5207, -5207, 8099, -8099],
  [1064, -1064, 3548, -3548, 6386, -6386, 9933, -9933],
  [1286, -1286, 4288, -4288, 7718, -7718, 12005, -12005],
  [1536, -1536, 5120, -5120, 9216, -9216, 14336, -14336],
];

export function decodeQoaBytes(input) {
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
  if (bytes.byteLength < 16) return null;
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (dv.getUint32(0, false) !== 0x716f6166) return null;
  const totalSamples = dv.getUint32(4, false);
  if (totalSamples === 0 || totalSamples > 48000 * 600) return null;

  const left = new Float32Array(totalSamples);
  const right = new Float32Array(totalSamples);
  let sampleRate = DEFAULT_SAMPLE_RATE;
  let pos = 8;
  let outIdx = 0;

  while (pos + 8 <= bytes.byteLength && outIdx < totalSamples) {
    const chAndRate = dv.getUint32(pos, false);
    const channels = (chAndRate >>> 24) & 0xff;
    sampleRate = chAndRate & 0x00ffffff;
    const fsamples = dv.getUint16(pos + 4, false);
    const fsize = dv.getUint16(pos + 6, false);
    if (channels < 1 || channels > 8 || fsamples < 1 || pos + fsize > bytes.byteLength) return null;
    const frameEnd = pos + fsize;
    pos += 8;

    const history = Array.from({ length: channels }, () => [0, 0, 0, 0]);
    const weights = Array.from({ length: channels }, () => [0, 0, 0, 0]);
    for (let c = 0; c < channels; c++) {
      for (let i = 0; i < 4; i++) { history[c][i] = dv.getInt16(pos, false); pos += 2; }
      for (let i = 0; i < 4; i++) { weights[c][i] = dv.getInt16(pos, false); pos += 2; }
    }

    let sIdx = 0;
    while (sIdx < fsamples && pos + 8 * channels <= frameEnd) {
      const sliceLen = Math.min(20, fsamples - sIdx);
      const chOut = Array.from({ length: channels }, () => new Int16Array(sliceLen));
      for (let c = 0; c < channels; c++) {
        const hi = dv.getUint32(pos, false);
        const lo = dv.getUint32(pos + 4, false);
        pos += 8;
        let bits = (BigInt(hi) << 32n) | BigInt(lo);
        const sf = Number((bits >> 60n) & 0x0fn);
        for (let si = 0; si < sliceLen; si++) {
          const shift = BigInt(57 - si * 3);
          const qr = Number((bits >> shift) & 0x07n);
          const h = history[c], w = weights[c];
          const pred = (h[0] * w[0] + h[1] * w[1] + h[2] * w[2] + h[3] * w[3]) >> 13;
          const deq = QOA_DEQUANT_TAB[sf][qr];
          const rec = Math.max(-32768, Math.min(32767, pred + deq));
          const delta = deq >> 4;
          for (let k = 0; k < 4; k++) w[k] += h[k] < 0 ? -delta : delta;
          h[0] = h[1]; h[1] = h[2]; h[2] = h[3]; h[3] = rec;
          chOut[c][si] = rec;
        }
      }
      for (let si = 0; si < sliceLen && outIdx < totalSamples; si++) {
        left[outIdx] = chOut[0][si] / 32768;
        right[outIdx] = (channels >= 2 ? chOut[1][si] : chOut[0][si]) / 32768;
        outIdx += 1;
      }
      sIdx += 20;
    }
    pos = frameEnd;
  }
  return { sampleRate, left, right };
}

export function encodeWavPcm16(left, right, sampleRate = DEFAULT_SAMPLE_RATE) {
  const frames = Math.min(left.length, right.length);
  const dataBytes = frames * 4;
  const buf = new ArrayBuffer(44 + dataBytes);
  const dv = new DataView(buf);
  const writeAscii = (off, str) => {
    for (let i = 0; i < str.length; i++) dv.setUint8(off + i, str.charCodeAt(i));
  };
  writeAscii(0, 'RIFF');
  dv.setUint32(4, 36 + dataBytes, true);
  writeAscii(8, 'WAVEfmt ');
  dv.setUint32(16, 16, true);
  dv.setUint16(20, 1, true);
  dv.setUint16(22, 2, true);
  dv.setUint32(24, sampleRate, true);
  dv.setUint32(28, sampleRate * 4, true);
  dv.setUint16(32, 4, true);
  dv.setUint16(34, 16, true);
  writeAscii(36, 'data');
  dv.setUint32(40, dataBytes, true);
  let p = 44;
  for (let i = 0; i < frames; i++) {
    const cl = Math.max(-1, Math.min(1, Number.isFinite(left[i]) ? left[i] : 0));
    const cr = Math.max(-1, Math.min(1, Number.isFinite(right[i]) ? right[i] : 0));
    dv.setInt16(p, cl < 0 ? Math.round(cl * 32768) : Math.round(cl * 32767), true);
    dv.setInt16(p + 2, cr < 0 ? Math.round(cr * 32768) : Math.round(cr * 32767), true);
    p += 4;
  }
  return new Uint8Array(buf);
}

function hashSeed(text) {
  let h = 2166136261 >>> 0;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  return h || 1;
}

export function synthesizeWaveformPeaks(seedText, count = 40) {
  let s = hashSeed(String(seedText || 'wave'));
  const peaks = new Float32Array(count);
  for (let i = 0; i < count; i++) {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    const env = Math.exp((-i / count) * 2.7);
    peaks[i] = Math.max(0.06, Math.min(1, (0.35 + ((s & 0xff) / 255) * 0.65) * env));
  }
  return peaks;
}

function applyStripOffline(left, right, strip, sampleRate) {
  if (!strip || strip.bypass) return { grDb: 0 };
  const n = left.length;
  const inGain = dbToLinear(strip.inputGainDb);
  const [[ll, rl], [lr, rr]] = computeMidSideMatrix(strip.stereoMode, strip.stereoWidthPct);
  const pan = finiteClamp(strip.pan, -1, 1, 0);

  const stages = [];
  if (strip.highPass?.enabled) {
    stages.push(biquadCoeffs('highpass', sampleRate, strip.highPass.frequencyHz, strip.highPass.q));
  }
  if (strip.lowPass?.enabled) {
    stages.push(biquadCoeffs('lowpass', sampleRate, strip.lowPass.frequencyHz, strip.lowPass.q));
  }
  if (strip.eq?.enabled) {
    stages.push(biquadCoeffs('lowshelf', sampleRate, strip.eq.lowFreqHz, 0.707, strip.eq.lowGainDb));
    stages.push(biquadCoeffs('peaking', sampleRate, strip.eq.midFreqHz, strip.eq.midQ, strip.eq.midGainDb));
    stages.push(biquadCoeffs('highshelf', sampleRate, strip.eq.highFreqHz, 0.707, strip.eq.highGainDb));
  }
  const z1L = new Float64Array(stages.length), z2L = new Float64Array(stages.length);
  const z1R = new Float64Array(stages.length), z2R = new Float64Array(stages.length);

  let envGr = 0, peakGr = 0;
  const comp = strip.compressor;
  const atk = Math.exp(-1 / (Math.max(0.0001, comp?.attackSec || 0.008) * sampleRate));
  const rel = Math.exp(-1 / (Math.max(0.005, comp?.releaseSec || 0.14) * sampleRate));
  const makeup = dbToLinear(comp?.makeupGainDb || 0);

  const od = strip.overdrive;
  const odOut = dbToLinear(od?.outputGainDb || 0);

  for (let i = 0; i < n; i++) {
    let l = left[i] * inGain;
    let r = right[i] * inGain;
    const ml = ll * l + rl * r;
    const mr = lr * l + rr * r;
    if (pan < 0) {
      const k = -pan;
      l = ml + mr * k * 0.5;
      r = mr * (1 - k);
    } else if (pan > 0) {
      const k = pan;
      l = ml * (1 - k);
      r = mr + ml * k * 0.5;
    } else {
      l = ml;
      r = mr;
    }

    for (let s = 0; s < stages.length; s++) {
      const c = stages[s];
      const yl = c.b0 * l + z1L[s];
      z1L[s] = c.b1 * l - c.a1 * yl + z2L[s];
      z2L[s] = c.b2 * l - c.a2 * yl;
      l = yl;
      const yr = c.b0 * r + z1R[s];
      z1R[s] = c.b1 * r - c.a1 * yr + z2R[s];
      z2R[s] = c.b2 * r - c.a2 * yr;
      r = yr;
    }

    if (comp?.enabled) {
      const inDb = linearToDb(Math.max(Math.abs(l), Math.abs(r)));
      const over = inDb - comp.thresholdDb;
      const slope = 1 - 1 / Math.max(1, comp.ratio);
      const knee = Math.max(0, comp.kneeDb);
      let targetGr = 0;
      if (knee > 0 && over > -0.5 * knee && over < 0.5 * knee) {
        const x = over + 0.5 * knee;
        targetGr = (slope * x * x) / (2 * knee);
      } else if (over >= 0.5 * knee) {
        targetGr = slope * over;
      }
      const coeff = targetGr > envGr ? atk : rel;
      envGr = coeff * envGr + (1 - coeff) * targetGr;
      if (envGr > peakGr) peakGr = envGr;
      const g = dbToLinear(-envGr) * makeup;
      const mix = comp.mix;
      l = l * (1 - mix) + l * g * mix;
      r = r * (1 - mix) + r * g * mix;
    }

    if (od?.enabled && od.mix > 0) {
      const wl = overdriveTransferSample(l, od.drive) * odOut;
      const wr = overdriveTransferSample(r, od.drive) * odOut;
      l = l * (1 - od.mix) + wl * od.mix;
      r = r * (1 - od.mix) + wr * od.mix;
    }

    left[i] = l;
    right[i] = r;
  }
  return { grDb: peakGr };
}

/** Deterministic pure-JS event renderer (used for offline `48 kHz` WAV bounce & headless metering). */
export function renderEventOfflineJs(root, eventId, decodedCache = new Map(), sampleRate = DEFAULT_SAMPLE_RATE) {
  const ev = root?.events?.[eventId];
  if (!ev) return null;
  const totalFrames = Math.max(64, Math.ceil(((ev.durationMs + 250) / 1000) * sampleRate));
  const busFrames = {};
  for (const b of orderedBuses(root)) {
    if (b.id !== MASTER_BUS_ID) {
      busFrames[b.id] = { left: new Float32Array(totalFrames), right: new Float32Array(totalFrames) };
    }
  }

  const layers = orderedLayers(ev);
  const anyLayerSolo = layers.some((l) => l.enabled && l.solo);

  for (const layer of layers) {
    if (!layer.enabled || (anyLayerSolo && !layer.solo)) continue;
    const dst = busFrames[layer.bus];
    if (!dst) continue;

    let assetId = layer.assetId;
    if (!assetId && layer.poolId && root.containers?.[layer.poolId]?.assetIds?.length) {
      assetId = root.containers[layer.poolId].assetIds[0];
    }
    const rateScale = 2 ** ((layer.pitchSemitones || 0) / 12);
    const clipLen = Math.max(1, Math.round(((layer.durationMs || 200) / 1000) * sampleRate));
    const clipL = new Float32Array(clipLen);
    const clipR = new Float32Array(clipLen);

    const cached = assetId ? decodedCache.get(assetId) : null;
    const trimFrames = Math.round(((layer.trimStartMs || 0) / 1000) * sampleRate);
    let rng = hashSeed(assetId || layer.id);
    const baseFreq = 180 + (rng % 260);
    for (let i = 0; i < clipLen; i++) {
      if (cached) {
        const srcIdx = Math.min(cached.left.length - 1, Math.floor((trimFrames + i) * rateScale));
        if (srcIdx >= 0 && srcIdx < cached.left.length) {
          clipL[i] = cached.left[srcIdx];
          clipR[i] = cached.right[srcIdx];
        }
      } else {
        const t = (i / sampleRate) * rateScale;
        rng = (Math.imul(rng, 1664525) + 1013904223) >>> 0;
        const noise = ((rng & 0xffff) / 32768 - 1) * 0.25 * Math.exp(-t * 80);
        const s = (Math.sin(2 * Math.PI * baseFreq * t) * 0.55 + noise) * Math.exp(-t * 13);
        clipL[i] = s;
        clipR[i] = s;
      }
    }

    const fi = Math.min(clipLen, Math.round(((layer.fadeInMs || 0) / 1000) * sampleRate));
    const fo = Math.min(clipLen, Math.round(((layer.fadeOutMs || 0) / 1000) * sampleRate));
    const g = dbToLinear(layer.gainDb || 0);
    for (let i = 0; i < clipLen; i++) {
      const env = clipFadeEnvelope(i, clipLen, fi, fo, layer.fadeCurve) * g;
      clipL[i] *= env;
      clipR[i] *= env;
    }

    applyStripOffline(clipL, clipR, layer.strip, sampleRate);
    const startFrame = Math.min(totalFrames, Math.round(((layer.startMs || 0) / 1000) * sampleRate));
    const copy = Math.min(clipLen, totalFrames - startFrame);
    for (let i = 0; i < copy; i++) {
      dst.left[startFrame + i] += clipL[i];
      dst.right[startFrame + i] += clipR[i];
    }
  }

  const buses = orderedBuses(root);
  const anyBusSolo = buses.some((b) => b.id !== MASTER_BUS_ID && b.solo);
  const masterL = new Float32Array(totalFrames);
  const masterR = new Float32Array(totalFrames);
  const meters = {};

  for (const b of buses) {
    if (b.id === MASTER_BUS_ID) continue;
    const buf = busFrames[b.id];
    if (b.muted || (anyBusSolo && !b.solo)) {
      buf.left.fill(0);
      buf.right.fill(0);
      meters[b.id] = analyzeStereoFrames(buf.left, buf.right, 0);
      continue;
    }
    const { grDb } = applyStripOffline(buf.left, buf.right, b.strip, sampleRate);
    for (let i = 0; i < totalFrames; i++) {
      buf.left[i] *= b.volume;
      buf.right[i] *= b.volume;
      masterL[i] += buf.left[i];
      masterR[i] += buf.right[i];
    }
    meters[b.id] = analyzeStereoFrames(buf.left, buf.right, grDb);
  }

  const masterBus = root.buses[MASTER_BUS_ID];
  if (masterBus?.muted) {
    masterL.fill(0);
    masterR.fill(0);
    meters[MASTER_BUS_ID] = analyzeStereoFrames(masterL, masterR, 0);
  } else if (masterBus) {
    const { grDb } = applyStripOffline(masterL, masterR, masterBus.strip, sampleRate);
    for (let i = 0; i < totalFrames; i++) {
      masterL[i] *= masterBus.volume;
      masterR[i] *= masterBus.volume;
    }
    meters[MASTER_BUS_ID] = analyzeStereoFrames(masterL, masterR, grDb);
  }

  return { sampleRate, left: masterL, right: masterR, meters };
}

export function bounceEventToWavBytes(root, eventId, decodedCache = new Map()) {
  const rendered = renderEventOfflineJs(root, eventId, decodedCache, DEFAULT_SAMPLE_RATE);
  if (!rendered) return null;
  return encodeWavPcm16(rendered.left, rendered.right, DEFAULT_SAMPLE_RATE);
}

// ---- Atomic `StudioDocOp` Builders ----------------------------------------------------------

/** Builds the `Op` placing an asset or multi-take pool at `event.contactMarkerMs - transientOffsetMs`. */
export function buildPlaceLayerOp(root, eventId, item, kind = 'pool') {
  const ev = root?.events?.[eventId];
  if (!ev) return null;
  const existing = orderedLayers(ev);
  const nextOrder = existing.length ? Math.max(...existing.map((l) => l.order ?? 0)) + 1 : 0;
  const layerId = `lyr_${randomHexId(16)}`;
  let transientMs = 0;
  let durationMs = 220;
  let bus = ev.bus;
  let label = item.label || item.id;

  if (kind === 'pool') {
    bus = item.bus || bus;
    const firstAsset = item.assetIds?.[0] ? root.assets?.[item.assetIds[0]] : null;
    if (firstAsset) {
      transientMs = firstAsset.transientOffsetMs || 0;
      durationMs = firstAsset.defaultDurationMs || 220;
    }
  } else {
    bus = item.bus || bus;
    transientMs = item.transientOffsetMs || 0;
    durationMs = item.defaultDurationMs || 220;
  }

  const startMs = Math.max(
    0,
    Math.min(ev.durationMs - 10, Math.round((ev.contactMarkerMs || 0) - transientMs)),
  );
  const layer = {
    order: nextOrder,
    label,
    bus,
    enabled: true,
    solo: false,
    startMs,
    durationMs,
    trimStartMs: 0,
    fadeInMs: transientMs > 8 ? 8 : 2,
    fadeOutMs: Math.min(50, Math.round(durationMs * 0.25)),
    fadeCurve: 'cosine',
    gainDb: 0,
    pan: 0,
    pitchSemitones: 0,
    pitchJitterCents: kind === 'pool' ? item.pitchJitterCents || 20 : 0,
    gainJitterDb: kind === 'pool' ? item.gainJitterDb || 0.5 : 0,
    color: root.buses?.[bus]?.color || '#60a5fa',
    strip: createDefaultStrip(),
  };
  if (kind === 'pool') layer.poolId = item.id;
  else layer.assetId = item.id;

  return {
    layerId,
    ops: [{ set: ['events', eventId, 'layers', layerId], value: layer }],
  };
}

export function buildApplyPresetOps(root, presetId) {
  const preset = root?.presets?.[presetId];
  if (!preset?.buses) return [];
  const ops = [];
  for (const [busId, patch] of Object.entries(preset.buses)) {
    if (!root.buses?.[busId]) continue;
    if (typeof patch.volume === 'number') {
      ops.push({ set: ['buses', busId, 'volume'], value: patch.volume });
    }
    if (patch.strip) {
      ops.push({ set: ['buses', busId, 'strip'], value: structuredClone(patch.strip) });
    }
  }
  return ops;
}

export function buildRegisterProjectSoundOps(root, projectAsset, targetBus, eventId) {
  const stem = String(projectAsset.path || projectAsset.id)
    .split('/')
    .pop()
    .replace(/\.[^.]+$/, '')
    .replace(/[^a-zA-Z0-9_-]+/g, '_');
  const assetId = root.assets?.[stem] ? `${stem}_${randomHexId(8)}` : stem;
  const assets = orderedAssets(root);
  const nextOrder = assets.length ? Math.max(...assets.map((a) => a.order ?? 0)) + 1 : 0;
  const newAsset = {
    order: nextOrder,
    poolId: assetId,
    label: stem.replaceAll('_', ' '),
    bus: targetBus || orderedBuses(root)[0]?.id || MASTER_BUS_ID,
    path: projectAsset.path,
    takeIndex: 1,
    takeCount: 1,
    defaultDurationMs: 260,
    transientOffsetMs: 10,
    tags: ['project'],
  };
  const ops = [{ set: ['assets', assetId], value: newAsset }];
  let placedLayerId = null;
  if (eventId && root.events?.[eventId]) {
    const placed = buildPlaceLayerOp(
      { ...root, assets: { ...root.assets, [assetId]: newAsset } },
      eventId,
      { id: assetId, ...newAsset },
      'asset',
    );
    if (placed) {
      placedLayerId = placed.layerId;
      ops.push(...placed.ops);
    }
  }
  return { assetId, layerId: placedLayerId, ops };
}

// ---- Live Browser Web Audio Graph (gracefully skipped in headless Node) ---------------------

class BrowserDawAudioRuntime {
  constructor() {
    this.ctx = null;
    this.decoded = new Map();
    this.peaks = new Map();
    this.activeSources = new Set();
    this.sequenceTimers = new Set();
    this.masterAnalysers = null;
    this.busAnalysers = new Map();
    this.busCompressors = new Map();
    this.playing = false;
    this.playStartCtxTime = 0;
    this.playDurationMs = 0;
    this.rrHistory = new Map();
  }

  ensureContext() {
    if (this.ctx) return this.ctx;
    const Ctor = globalThis.AudioContext || globalThis.webkitAudioContext;
    if (!Ctor) return null;
    this.ctx = new Ctor();
    if (this.ctx.state === 'suspended') this.ctx.resume?.().catch(() => {});
    return this.ctx;
  }

  async loadAsset(assetId, url) {
    if (this.decoded.has(assetId)) return this.decoded.get(assetId);
    if (!globalThis.fetch || !url) return null;
    try {
      const res = await fetch(url);
      if (!res.ok) return null;
      const ab = await res.arrayBuffer();
      const u8 = new Uint8Array(ab);
      if (u8.byteLength >= 4 && u8[0] === 0x71 && u8[1] === 0x6f && u8[2] === 0x61 && u8[3] === 0x66) {
        const qoa = decodeQoaBytes(u8);
        if (qoa) {
          this.decoded.set(assetId, qoa);
          this.peaks.set(assetId, computePeaksFromArray(qoa.left, 44));
          return qoa;
        }
      }
      const wavMeta = extractWavPeaks(u8, 44);
      if (wavMeta?.peaks) this.peaks.set(assetId, wavMeta.peaks);
      const ctx = this.ensureContext();
      if (ctx?.decodeAudioData) {
        const audioBuf = await ctx.decodeAudioData(ab.slice(0));
        const left = audioBuf.getChannelData(0).slice();
        const right = (audioBuf.numberOfChannels > 1 ? audioBuf.getChannelData(1) : left).slice();
        const entry = { sampleRate: audioBuf.sampleRate, left, right, audioBuf };
        this.decoded.set(assetId, entry);
        if (!this.peaks.has(assetId)) this.peaks.set(assetId, computePeaksFromArray(left, 44));
        return entry;
      }
    } catch {
      // Fallback synthesis will handle missing/offline files cleanly.
    }
    return null;
  }

  stopAll() {
    for (const src of this.activeSources) {
      try { src.stop(); } catch { /* already stopped */ }
    }
    this.activeSources.clear();
    for (const timer of this.sequenceTimers) clearTimeout(timer);
    this.sequenceTimers.clear();
    this.playing = false;
  }

  pickContainerAsset(poolId, container) {
    const ids = container?.assetIds || [];
    if (!ids.length) return null;
    if (ids.length === 1) return ids[0];
    const last = this.rrHistory.get(poolId) ?? -1;
    let idx = (last + 1) % ids.length;
    if (container.roundRobinMode !== 'sequential') {
      do {
        idx = Math.floor(Math.random() * ids.length);
      } while (idx === last && ids.length > 1);
    }
    this.rrHistory.set(poolId, idx);
    return ids[idx];
  }

  triggerRenderedBuffer(left, right, sampleRate, durationMs) {
    const ctx = this.ensureContext();
    if (!ctx) return false;
    const buf = ctx.createBuffer(2, left.length, sampleRate);
    buf.getChannelData(0).set(left);
    buf.getChannelData(1).set(right);
    const src = ctx.createBufferSource();
    src.buffer = buf;

    const splitter = ctx.createChannelSplitter(2);
    const anL = ctx.createAnalyser();
    const anR = ctx.createAnalyser();
    const anMix = ctx.createAnalyser();
    anL.fftSize = 512;
    anR.fftSize = 512;
    anMix.fftSize = 512;
    src.connect(splitter);
    splitter.connect(anL, 0);
    splitter.connect(anR, 1);
    src.connect(anMix);
    src.connect(ctx.destination);
    this.masterAnalysers = {
      anL,
      anR,
      anMix,
      bufL: new Float32Array(256),
      bufR: new Float32Array(256),
      fft: new Uint8Array(anMix.frequencyBinCount),
    };

    this.activeSources.add(src);
    this.playing = true;
    this.playStartCtxTime = ctx.currentTime;
    this.playDurationMs = durationMs;
    src.onended = () => {
      this.activeSources.delete(src);
      if (this.activeSources.size === 0) this.playing = false;
    };
    src.start();
    return true;
  }

  destroy() {
    this.stopAll();
    if (this.ctx?.close) this.ctx.close().catch(() => {});
    this.ctx = null;
  }
}

function computePeaksFromArray(samples, count = 44) {
  const out = new Float32Array(count);
  if (!samples?.length) return out;
  const bin = Math.max(1, Math.floor(samples.length / count));
  for (let i = 0; i < count; i++) {
    let p = 0;
    const start = i * bin;
    const end = Math.min(samples.length, start + bin);
    for (let j = start; j < end; j++) {
      const a = Math.abs(samples[j]);
      if (a > p) p = a;
    }
    out[i] = Math.min(1, p);
  }
  return out;
}

// ---- Mounted 4-Panel Ganbate Studio DAW Workbench -------------------------------------------

function ensureStylesheet(doc) {
  if (!doc?.head || doc.getElementById('ganbate-audio-daw-css')) return;
  const link = doc.createElement('link');
  link.id = 'ganbate-audio-daw-css';
  link.rel = 'stylesheet';
  link.href = '/editor/audio-daw.css';
  doc.head.append(link);
}

/**
 * Mounts the 4-panel Event-Driven Sound & Foley DAW inside `container` using `StudioWorkbenchApi`.
 *
 * @param {HTMLElement} container
 * @param {import('./plugin.d.ts').StudioWorkbenchApi} studioApi
 */
export function mountAudioDaw(container, studioApi = {}) {
  const doc = container.ownerDocument || globalThis.document;
  ensureStylesheet(doc);

  const el = (tag, attrs = {}, ...children) => {
    const node = doc.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (k.startsWith('on') && typeof v === 'function') {
        node.addEventListener(k.slice(2).toLowerCase(), v);
      } else if (k === 'class') {
        node.className = v;
      } else if (k === 'value') {
        node.value = String(v ?? '');
      } else if (typeof v === 'boolean') {
        if (v) node.setAttribute(k, '');
        if (k in node) node[k] = v;
      } else if (v !== null && v !== undefined) {
        node.setAttribute(k, String(v));
      }
    }
    for (const c of children.flat()) {
      if (c === null || c === undefined || c === false) continue;
      node.append(typeof c === 'string' || typeof c === 'number' ? doc.createTextNode(String(c)) : c);
    }
    return node;
  };

  const svgEl = (tag, attrs = {}, ...children) => {
    const node = doc.createElementNS
      ? doc.createElementNS(SVG_NS, tag)
      : doc.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (v !== null && v !== undefined) node.setAttribute(k, String(v));
    }
    for (const c of children.flat()) {
      if (c) node.append(c);
    }
    return node;
  };

  const tr = (key) => {
    const locale = studioApi.getLocale?.() || 'en';
    const fallback = I18N_FALLBACK[locale]?.[key] ?? I18N_FALLBACK.en[key] ?? key;
    if (typeof studioApi.t === 'function') {
      const translated = studioApi.t(key);
      return translated && translated !== key ? translated : fallback;
    }
    return fallback;
  };

  const runtime = new BrowserDawAudioRuntime();
  const unsubs = [];

  const ui = {
    docId: null,
    docPath: '',
    dirty: false,
    undoLabel: null,
    redoLabel: null,
    root: createStarterSoundBank(),
    projectAssets: [],
    soundbankDocs: [],
    browserTab: 'pools', // 'pools' | 'assets' | 'project'
    browserBusFilter: 'all',
    browserQuery: '',
    selectedEventId: 'evt_surface_hit',
    selectedLayerId: 'lyr_contact',
    selectedSequenceId: 'seq_double_hit',
    selectedPresetId: 'neutral-bypass',
    inspectorTarget: { kind: 'bus', id: MASTER_BUS_ID }, // or { kind: 'layer', eventId, id }
    timelineZoom: 1,
    loopEnabled: false,
    scopeMode: 'waveform', // 'waveform' | 'fft'
    latchedClips: new Set(),
    peakHolds: new Map(),
    lastMeters: {},
    playheadMs: 0,
    rafId: 0,
    active: true,
    destroyed: false,
  };

  function resolveAssetUrl(relPath) {
    if (!relPath) return '';
    if (typeof studioApi.assetUrl === 'function') return studioApi.assetUrl(relPath);
    return '/project/' + String(relPath).split('/').map(encodeURIComponent).join('/');
  }

  function syncSelectionInvariants() {
    const evs = orderedEvents(ui.root);
    if (!ui.root.events[ui.selectedEventId]) {
      ui.selectedEventId = evs[0]?.id || '';
    }
    const ev = ui.root.events[ui.selectedEventId];
    const layers = orderedLayers(ev);
    if (!ev?.layers?.[ui.selectedLayerId]) {
      ui.selectedLayerId = layers[0]?.id || '';
    }
    const seqs = orderedSequences(ui.root);
    if (!ui.root.sequences[ui.selectedSequenceId]) {
      ui.selectedSequenceId = seqs[0]?.id || '';
    }
    const presets = orderedPresets(ui.root);
    if (!ui.root.presets[ui.selectedPresetId]) {
      ui.selectedPresetId = presets[0]?.id || '';
    }
    if (ui.inspectorTarget.kind === 'bus' && !ui.root.buses[ui.inspectorTarget.id]) {
      ui.inspectorTarget = { kind: 'bus', id: MASTER_BUS_ID };
    } else if (ui.inspectorTarget.kind === 'layer') {
      const tev = ui.root.events[ui.inspectorTarget.eventId];
      if (!tev?.layers?.[ui.inspectorTarget.id]) {
        ui.inspectorTarget = { kind: 'bus', id: MASTER_BUS_ID };
      }
    }
  }

  function recomputeOfflineMeters() {
    if (!ui.selectedEventId || !ui.root.events[ui.selectedEventId]) return;
    const rendered = renderEventOfflineJs(ui.root, ui.selectedEventId, runtime.decoded);
    if (rendered?.meters) {
      ui.lastMeters = rendered.meters;
      for (const [bid, m] of Object.entries(rendered.meters)) {
        if (m.clipped) ui.latchedClips.add(bid);
        const prev = ui.peakHolds.get(bid) ?? MIN_METER_DB;
        ui.peakHolds.set(bid, Math.max(prev, m.peakLDb, m.peakRDb));
      }
    }
  }

  function ingestSnapshot(docSnap) {
    if (!docSnap) return;
    const rawRoot = docSnap.root && typeof docSnap.root === 'object' ? docSnap.root : docSnap;
    if (rawRoot.ganbate && rawRoot.ganbate !== SOUNDBANK_KIND) return;
    ui.docId = docSnap.id || ui.docId;
    ui.docPath = docSnap.path || ui.docPath;
    ui.dirty = Boolean(docSnap.dirty);
    ui.undoLabel = docSnap.undo ?? null;
    ui.redoLabel = docSnap.redo ?? null;
    ui.root = normalizeSoundBankRoot(rawRoot);
    syncSelectionInvariants();
    recomputeOfflineMeters();
  }

  function refreshDocumentCatalog() {
    if (typeof studioApi.listDocuments === 'function') {
      ui.soundbankDocs = studioApi.listDocuments({ suffix: '.soundbank.json' }) || [];
    } else if (typeof studioApi.getAssets === 'function') {
      ui.soundbankDocs = (studioApi.getAssets() || []).filter((a) =>
        String(a.path || '').endsWith('.soundbank.json'),
      );
    }
    if (typeof studioApi.getAssets === 'function') {
      ui.projectAssets = (studioApi.getAssets() || []).filter(
        (a) =>
          a.kind === 'sound' ||
          /\.(wav|ogg|qoa|mp3|webm)$/i.test(String(a.path || '')),
      );
    }
  }

  async function dispatchOps(label, ops, merge) {
    if (!Array.isArray(ops) || ops.length === 0) return null;
    applyOps(ui.root, ops);
    ui.root = normalizeSoundBankRoot(ui.root);
    ui.dirty = true;
    syncSelectionInvariants();
    recomputeOfflineMeters();
    renderAll();

    const editFn = studioApi.edit || studioApi.editDocument;
    if (typeof editFn === 'function') {
      const updated = await editFn.call(studioApi, ui.docId || ui.docPath || null, ops, label, merge);
      if (updated && typeof updated === 'object' && updated.root) {
        ingestSnapshot(updated);
        renderAll();
      }
      return updated;
    }
    return null;
  }

  async function auditionSingleAsset(asset) {
    if (!asset) return;
    const url = resolveAssetUrl(asset.path);
    const loaded = await runtime.loadAsset(asset.id, url);
    const sr = loaded?.sampleRate || DEFAULT_SAMPLE_RATE;
    const durMs = asset.defaultDurationMs || 220;
    const len = loaded ? loaded.left.length : Math.round((durMs / 1000) * sr);
    const left = loaded ? loaded.left.slice() : new Float32Array(len);
    const right = loaded ? loaded.right.slice() : new Float32Array(len);
    if (!loaded) {
      let s = hashSeed(asset.id);
      const f = 200 + (s % 300);
      for (let i = 0; i < len; i++) {
        const t = i / sr;
        s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
        const v = Math.sin(2 * Math.PI * f * t) * Math.exp(-t * 15) * 0.6;
        left[i] = v;
        right[i] = v;
      }
    }
    runtime.triggerRenderedBuffer(left, right, sr, durMs);
  }

  async function auditionEvent(eventId = ui.selectedEventId) {
    const ev = ui.root.events[eventId];
    if (!ev) return null;
    for (const l of orderedLayers(ev)) {
      const aid =
        l.assetId ||
        (l.poolId ? runtime.pickContainerAsset(l.poolId, ui.root.containers[l.poolId]) : null);
      if (aid && ui.root.assets[aid]) {
        await runtime.loadAsset(aid, resolveAssetUrl(ui.root.assets[aid].path));
      }
    }
    const rendered = renderEventOfflineJs(ui.root, eventId, runtime.decoded);
    if (rendered) {
      ui.lastMeters = rendered.meters;
      for (const [bid, m] of Object.entries(rendered.meters)) {
        if (m.clipped) ui.latchedClips.add(bid);
        ui.peakHolds.set(bid, Math.max(ui.peakHolds.get(bid) ?? MIN_METER_DB, m.peakLDb, m.peakRDb));
      }
      runtime.triggerRenderedBuffer(rendered.left, rendered.right, rendered.sampleRate, ev.durationMs);
      startMeterLoop();
    }
    studioApi.sendPieCue?.({
      cueId: ev.cueId || eventId,
      track: 'audio',
      payload: { eventId, bus: ev.bus, contactMarkerMs: ev.contactMarkerMs },
    });
    renderConsolePanel();
    return rendered;
  }

  function playSequence(seqId = ui.selectedSequenceId) {
    const seq = ui.root.sequences[seqId];
    if (!seq?.steps?.length) return;
    runtime.stopAll();
    for (const step of seq.steps) {
      const timer = setTimeout(() => {
        runtime.sequenceTimers.delete(timer);
        if (!ui.destroyed) {
          ui.selectedEventId = step.eventId;
          syncSelectionInvariants();
          renderTimelinePanel();
          void auditionEvent(step.eventId);
        }
      }, Math.max(0, step.atMs));
      runtime.sequenceTimers.add(timer);
    }
  }

  function bounceEventWav(eventId = ui.selectedEventId) {
    const bytes = bounceEventToWavBytes(ui.root, eventId, runtime.decoded);
    if (!bytes) return null;
    if (globalThis.Blob && globalThis.URL?.createObjectURL && doc?.createElement) {
      try {
        const blob = new Blob([bytes], { type: 'audio/wav' });
        const href = URL.createObjectURL(blob);
        const a = doc.createElement('a');
        a.href = href;
        a.download = `${eventId}_48kHz_16bit.wav`;
        a.click?.();
        setTimeout(() => URL.revokeObjectURL(href), 2000);
      } catch {
        // Ignore download navigation errors in non-browser harnesses
      }
    }
    studioApi.notify?.('info', `Bounced ${eventId} (${bytes.byteLength} bytes, 48 kHz 16-bit WAV)`);
    return bytes;
  }

  // ---- Root Layout Containers -----------------------------------------------------------------

  const rootEl = el('div', { class: 'gdaw-root', 'data-workbench': 'audio-daw' });
  const toolbarEl = el('header', { class: 'gdaw-toolbar' });
  const browserEl = el('section', { class: 'gdaw-panel gdaw-area-browser' });
  const timelineEl = el('section', { class: 'gdaw-panel gdaw-area-timeline' });
  const consoleEl = el('section', { class: 'gdaw-panel gdaw-area-console' });
  const dspEl = el('section', { class: 'gdaw-panel gdaw-area-dsp' });
  const gridEl = el('div', { class: 'gdaw-grid' }, browserEl, timelineEl, consoleEl, dspEl);
  rootEl.append(toolbarEl, gridEl);
  container.replaceChildren ? container.replaceChildren(rootEl) : ((container.innerHTML = ''), container.append(rootEl));

  // ---- Top Toolbar ----------------------------------------------------------------------------

  function renderToolbar() {
    const docSelect = el(
      'select',
      {
        'aria-label': 'SoundBank document',
        onchange: async (e) => {
          const val = e.target.value;
          if (val && typeof studioApi.openDocument === 'function') {
            const opened = await studioApi.openDocument(val);
            if (opened) {
              ingestSnapshot(opened);
              renderAll();
            }
          }
        },
      },
      ui.soundbankDocs.length === 0
        ? el('option', { value: ui.docPath || '' }, ui.docPath || `${ui.root.name} (in-memory)`)
        : ui.soundbankDocs.map((d) =>
            el(
              'option',
              { value: d.id || d.path, selected: d.id === ui.docId || d.path === ui.docPath },
              d.path.split('/').pop(),
            ),
          ),
    );

    const presetSelect = el(
      'select',
      {
        'aria-label': 'Mix preset',
        onchange: (e) => {
          ui.selectedPresetId = e.target.value;
        },
      },
      orderedPresets(ui.root).map((p) =>
        el('option', { value: p.id, selected: p.id === ui.selectedPresetId }, p.label),
      ),
    );

    toolbarEl.replaceChildren(
      el('strong', {}, tr('dawTitle')),
      docSelect,
      el(
        'button',
        {
          type: 'button',
          'data-action': 'create-soundbank',
          onclick: async () => {
            const initialRoot = createStarterSoundBank({
              id: randomHexId(32),
              name: 'Project SoundBank',
            });
            if (typeof studioApi.createDocument === 'function') {
              const created = await studioApi.createDocument({
                path: 'content/audio/project.soundbank.json',
                kind: SOUNDBANK_KIND,
                name: initialRoot.name,
                initialRoot,
              });
              if (created) {
                refreshDocumentCatalog();
                ingestSnapshot(created);
                renderAll();
                return;
              }
            }
            ingestSnapshot({ id: initialRoot.id, path: 'content/audio/project.soundbank.json', root: initialRoot });
            renderAll();
          },
        },
        tr('newSoundbank'),
      ),
      el(
        'span',
        { class: `gdaw-badge ${ui.dirty ? 'dirty' : ''}` },
        ui.dirty ? 'Draft · Unsaved' : 'Synced',
      ),
      el('span', { class: 'gdaw-sep' }),
      el(
        'button',
        {
          type: 'button',
          'data-action': 'undo',
          onclick: async () => {
            const snap = await studioApi.undo?.(ui.docId || ui.docPath || undefined);
            if (snap) {
              ingestSnapshot(snap);
              renderAll();
            }
          },
        },
        tr('undo'),
      ),
      el(
        'button',
        {
          type: 'button',
          'data-action': 'redo',
          onclick: async () => {
            const snap = await studioApi.redo?.(ui.docId || ui.docPath || undefined);
            if (snap) {
              ingestSnapshot(snap);
              renderAll();
            }
          },
        },
        tr('redo'),
      ),
      el(
        'button',
        {
          type: 'button',
          class: 'primary',
          'data-action': 'save',
          onclick: async () => {
            const ok = await studioApi.save?.(ui.docId || ui.docPath || undefined);
            if (ok) {
              ui.dirty = false;
              renderToolbar();
            }
          },
        },
        tr('save'),
      ),
      el('span', { class: 'gdaw-sep' }),
      presetSelect,
      el(
        'button',
        {
          type: 'button',
          'data-action': 'apply-preset',
          onclick: () => {
            const ops = buildApplyPresetOps(ui.root, ui.selectedPresetId);
            void dispatchOps(`Apply preset ${ui.selectedPresetId}`, ops);
          },
        },
        tr('applyPreset'),
      ),
      el('span', { class: 'gdaw-grow' }),
      el(
        'button',
        {
          type: 'button',
          class: ui.root.dynamics.enabled ? 'active' : '',
          'data-action': 'toggle-dynamics',
          onclick: () => {
            void dispatchOps('Toggle sidechain ducking', [
              { set: ['dynamics', 'enabled'], value: !ui.root.dynamics.enabled },
            ]);
          },
        },
        `Ducking: ${ui.root.dynamics.enabled ? 'ON' : 'OFF'}`,
      ),
    );
  }

  // ---- Panel 1: SoundBank & Multi-Take Cue Browser --------------------------------------------

  function buildSparklineSvg(seedOrAssetId, color = '#60a5fa') {
    const peaks = runtime.peaks.get(seedOrAssetId) || synthesizeWaveformPeaks(seedOrAssetId, 36);
    const bars = [];
    const w = 180 / peaks.length;
    for (let i = 0; i < peaks.length; i++) {
      const h = Math.max(2, peaks[i] * 14);
      bars.push(
        svgEl('rect', {
          x: (i * w).toFixed(1),
          y: (8 - h * 0.5).toFixed(1),
          width: Math.max(1.4, w - 1).toFixed(1),
          height: h.toFixed(1),
          fill: color,
          rx: '0.6',
        }),
      );
    }
    return svgEl('svg', { class: 'gdaw-spark', viewBox: '0 0 180 16', preserveAspectRatio: 'none' }, bars);
  }

  function renderBrowserPanel() {
    const buses = orderedBuses(ui.root).filter((b) => b.id !== MASTER_BUS_ID);
    const query = ui.browserQuery.trim().toLowerCase();

    const listEl = el('div', { class: 'gdaw-cue-list' });
    if (ui.browserTab === 'pools') {
      const pools = orderedContainers(ui.root).filter((c) => {
        if (ui.browserBusFilter !== 'all' && c.bus !== ui.browserBusFilter) return false;
        return !query || c.label.toLowerCase().includes(query) || c.id.toLowerCase().includes(query);
      });
      for (const pool of pools) {
        const busColor = ui.root.buses[pool.bus]?.color || '#60a5fa';
        listEl.append(
          el(
            'div',
            { class: 'gdaw-cue-item', 'data-pool-id': pool.id },
            el(
              'div',
              {},
              el('div', { class: 'gdaw-cue-title' }, pool.label),
              el(
                'div',
                { class: 'gdaw-cue-meta' },
                el('span', {}, `${pool.assetIds.length} takes · ${pool.roundRobinMode}`),
                el('span', {}, `Bus: ${ui.root.buses[pool.bus]?.label || pool.bus}`),
              ),
            ),
            el(
              'div',
              { class: 'gdaw-row' },
              el(
                'button',
                {
                  type: 'button',
                  title: 'Audition round-robin take',
                  onclick: () => {
                    const picked = runtime.pickContainerAsset(pool.id, pool);
                    if (picked && ui.root.assets[picked]) {
                      void auditionSingleAsset({ id: picked, ...ui.root.assets[picked] });
                    }
                  },
                },
                '▶',
              ),
              el(
                'button',
                {
                  type: 'button',
                  class: 'primary',
                  'data-action': `place-pool-${pool.id}`,
                  onclick: () => {
                    const built = buildPlaceLayerOp(ui.root, ui.selectedEventId, pool, 'pool');
                    if (built) {
                      ui.selectedLayerId = built.layerId;
                      ui.inspectorTarget = {
                        kind: 'layer',
                        eventId: ui.selectedEventId,
                        id: built.layerId,
                      };
                      void dispatchOps(`Place ${pool.label}`, built.ops);
                    }
                  },
                },
                tr('placeAtContact'),
              ),
            ),
            buildSparklineSvg(pool.assetIds[0] || pool.id, busColor),
          ),
        );
      }
    } else if (ui.browserTab === 'assets') {
      const assets = orderedAssets(ui.root).filter((a) => {
        if (ui.browserBusFilter !== 'all' && a.bus !== ui.browserBusFilter) return false;
        return (
          !query ||
          a.label.toLowerCase().includes(query) ||
          a.id.toLowerCase().includes(query) ||
          a.path.toLowerCase().includes(query)
        );
      });
      for (const asset of assets) {
        const busColor = ui.root.buses[asset.bus]?.color || '#60a5fa';
        listEl.append(
          el(
            'div',
            { class: 'gdaw-cue-item', 'data-asset-id': asset.id },
            el(
              'div',
              {},
              el('div', { class: 'gdaw-cue-title' }, asset.label),
              el(
                'div',
                { class: 'gdaw-cue-meta' },
                el('span', {}, `${Math.round(asset.defaultDurationMs)} ms · onset ${Math.round(asset.transientOffsetMs)} ms`),
                el('span', {}, ui.root.buses[asset.bus]?.label || asset.bus),
              ),
            ),
            el(
              'div',
              { class: 'gdaw-row' },
              el('button', { type: 'button', onclick: () => void auditionSingleAsset(asset) }, '▶'),
              el(
                'button',
                {
                  type: 'button',
                  class: 'primary',
                  'data-action': `place-asset-${asset.id}`,
                  onclick: () => {
                    const built = buildPlaceLayerOp(ui.root, ui.selectedEventId, asset, 'asset');
                    if (built) {
                      ui.selectedLayerId = built.layerId;
                      ui.inspectorTarget = {
                        kind: 'layer',
                        eventId: ui.selectedEventId,
                        id: built.layerId,
                      };
                      void dispatchOps(`Place ${asset.label}`, built.ops);
                    }
                  },
                },
                tr('placeAtContact'),
              ),
            ),
            buildSparklineSvg(asset.id, busColor),
          ),
        );
      }
    } else {
      const knownPaths = new Set(orderedAssets(ui.root).map((a) => a.path));
      const files = ui.projectAssets.filter((f) => !query || f.path.toLowerCase().includes(query));
      for (const file of files) {
        const already = knownPaths.has(file.path);
        listEl.append(
          el(
            'div',
            { class: 'gdaw-cue-item' },
            el(
              'div',
              {},
              el('div', { class: 'gdaw-cue-title' }, file.path.split('/').pop()),
              el('div', { class: 'gdaw-cue-meta' }, file.path),
            ),
            el(
              'button',
              {
                type: 'button',
                class: already ? '' : 'primary',
                onclick: () => {
                  const targetBus =
                    ui.browserBusFilter !== 'all' ? ui.browserBusFilter : buses[0]?.id || MASTER_BUS_ID;
                  const reg = buildRegisterProjectSoundOps(
                    ui.root,
                    file,
                    targetBus,
                    ui.selectedEventId,
                  );
                  if (reg.layerId) ui.selectedLayerId = reg.layerId;
                  void dispatchOps(`Register ${file.path}`, reg.ops);
                },
              },
              already ? tr('placeAtContact') : tr('registerAndPlace'),
            ),
          ),
        );
      }
    }

    const searchInput = el('input', {
      type: 'search',
      placeholder: 'Search cues, pools, tags…',
      value: ui.browserQuery,
      oninput: (e) => {
        ui.browserQuery = e.target.value;
        renderBrowserPanel();
      },
    });

    browserEl.replaceChildren(
      el(
        'div',
        { class: 'gdaw-panel-hdr' },
        el('h3', {}, tr('browserTitle')),
        el(
          'span',
          { class: 'gdaw-badge' },
          `${Object.keys(ui.root.assets).length} assets · ${Object.keys(ui.root.containers).length} pools`,
        ),
      ),
      el(
        'div',
        { class: 'gdaw-browser-controls' },
        el(
          'div',
          { class: 'gdaw-pills' },
          [
            ['pools', `Pools (${Object.keys(ui.root.containers).length})`],
            ['assets', `Assets (${Object.keys(ui.root.assets).length})`],
            ['project', `Files (${ui.projectAssets.length})`],
          ].map(([tab, lbl]) =>
            el(
              'button',
              {
                type: 'button',
                class: ui.browserTab === tab ? 'active' : '',
                onclick: () => {
                  ui.browserTab = tab;
                  renderBrowserPanel();
                },
              },
              lbl,
            ),
          ),
        ),
        searchInput,
        el(
          'div',
          { class: 'gdaw-pills' },
          el(
            'button',
            {
              type: 'button',
              class: ui.browserBusFilter === 'all' ? 'active' : '',
              onclick: () => {
                ui.browserBusFilter = 'all';
                renderBrowserPanel();
              },
            },
            'All',
          ),
          buses.map((b) =>
            el(
              'button',
              {
                type: 'button',
                class: ui.browserBusFilter === b.id ? 'active' : '',
                onclick: () => {
                  ui.browserBusFilter = b.id;
                  renderBrowserPanel();
                },
              },
              b.label,
            ),
          ),
        ),
      ),
      listEl,
    );
  }

  // ---- Panel 2: Event Action Sheet & Contact-Sync Timeline ------------------------------------

  function buildClipSvg(layer) {
    const seed = layer.assetId || layer.poolId || layer.id;
    const peaks = runtime.peaks.get(seed) || synthesizeWaveformPeaks(seed, 42);
    const dur = Math.max(1, layer.durationMs);
    const fiPct = Math.min(100, ((layer.fadeInMs || 0) / dur) * 100);
    const foPct = Math.min(100, ((layer.fadeOutMs || 0) / dur) * 100);

    const wavePts = [];
    for (let i = 0; i < peaks.length; i++) {
      const x = (i / Math.max(1, peaks.length - 1)) * 200;
      const y = 17 - peaks[i] * 14;
      wavePts.push(`${x.toFixed(1)},${y.toFixed(1)}`);
    }
    for (let i = peaks.length - 1; i >= 0; i--) {
      const x = (i / Math.max(1, peaks.length - 1)) * 200;
      const y = 17 + peaks[i] * 14;
      wavePts.push(`${x.toFixed(1)},${y.toFixed(1)}`);
    }

    // Build curved fade envelope path (0..200 x 0..34)
    const envPts = ['0,34'];
    const steps = 12;
    for (let s = 0; s <= steps; s++) {
      const t = s / steps;
      const x = (fiPct * 2 * t).toFixed(1);
      const y = (34 - evaluateFadeCurve(t, layer.fadeCurve) * 32).toFixed(1);
      envPts.push(`${x},${y}`);
    }
    for (let s = 0; s <= steps; s++) {
      const t = s / steps;
      const x = (200 - foPct * 2 * (1 - t)).toFixed(1);
      const y = (34 - evaluateFadeCurve(1 - t, layer.fadeCurve) * 32).toFixed(1);
      envPts.push(`${x},${y}`);
    }
    envPts.push('200,34');

    return svgEl(
      'svg',
      { viewBox: '0 0 200 34', preserveAspectRatio: 'none' },
      svgEl('polygon', { points: wavePts.join(' '), fill: layer.color || '#60a5fa', opacity: '0.68' }),
      svgEl('polyline', {
        points: envPts.slice(1, -1).join(' '),
        fill: 'none',
        stroke: '#f8fafc',
        'stroke-width': '1.3',
      }),
    );
  }

  function renderTimelinePanel() {
    const events = orderedEvents(ui.root);
    const ev = ui.root.events[ui.selectedEventId];
    const buses = orderedBuses(ui.root).filter((b) => b.id !== MASTER_BUS_ID);

    const eventSelect = el(
      'select',
      {
        'aria-label': 'Active event sheet',
        onchange: (e) => {
          ui.selectedEventId = e.target.value;
          syncSelectionInvariants();
          recomputeOfflineMeters();
          renderAll();
        },
      },
      events.map((item) =>
        el('option', { value: item.id, selected: item.id === ui.selectedEventId }, item.label),
      ),
    );

    const seqSelect = el(
      'select',
      {
        'aria-label': 'Choreography sequence',
        onchange: (e) => {
          ui.selectedSequenceId = e.target.value;
        },
      },
      orderedSequences(ui.root).map((s) =>
        el('option', { value: s.id, selected: s.id === ui.selectedSequenceId }, s.label),
      ),
    );

    if (!ev) {
      timelineEl.replaceChildren(
        el('div', { class: 'gdaw-panel-hdr' }, el('h3', {}, tr('timelineTitle'))),
        el('div', { class: 'gdaw-tl-bar' }, 'No event sheets defined.'),
      );
      return;
    }

    const pxPerMs = 0.72 * ui.timelineZoom;
    const trackWidthPx = Math.max(360, Math.round(ev.durationMs * pxPerMs));
    const contactFrame60 = Math.round((ev.contactMarkerMs / 1000) * 60);

    // Ruler ticks every 50ms or 100ms
    const stepMs = ev.durationMs > 900 ? 100 : 50;
    const ticks = [];
    for (let ms = 0; ms <= ev.durationMs; ms += stepMs) {
      const f60 = Math.round((ms / 1000) * 60);
      ticks.push(
        el(
          'div',
          {
            class: 'gdaw-tl-tick',
            style: `left: ${168 + Math.round(ms * pxPerMs)}px`,
          },
          `${ms}ms · F${f60}`,
        ),
      );
    }

    const rulerEl = el(
      'div',
      {
        class: 'gdaw-tl-ruler',
        title: 'Click or drag to move the Animation Contact Marker',
        onpointerdown: (e) => {
          const rect = rulerEl.getBoundingClientRect?.();
          if (!rect) return;
          const updateMarker = (clientX) => {
            const x = clientX - rect.left - 168;
            const nextMs = Math.round(finiteClamp(x / pxPerMs, 0, ev.durationMs, ev.contactMarkerMs));
            void dispatchOps(
              'Move contact marker',
              [{ set: ['events', ui.selectedEventId, 'contactMarkerMs'], value: nextMs }],
              `contact:${ui.selectedEventId}`,
            );
          };
          updateMarker(e.clientX);
          const onMove = (me) => updateMarker(me.clientX);
          const onUp = () => {
            doc.removeEventListener('pointermove', onMove);
            doc.removeEventListener('pointerup', onUp);
          };
          doc.addEventListener('pointermove', onMove);
          doc.addEventListener('pointerup', onUp);
        },
      },
      ticks,
    );

    const layers = orderedLayers(ev);
    const laneEls = layers.map((layer) => {
      const isSelected = layer.id === ui.selectedLayerId;
      const clipLeft = Math.round(layer.startMs * pxPerMs);
      const clipWidth = Math.max(14, Math.round(layer.durationMs * pxPerMs));

      const clipEl = el(
        'div',
        {
          class: `gdaw-clip ${isSelected ? 'selected' : ''} ${layer.enabled ? '' : 'muted'}`,
          'data-layer-id': layer.id,
          style: `left: ${clipLeft}px; width: ${clipWidth}px;`,
          onpointerdown: (e) => {
            e.stopPropagation();
            ui.selectedLayerId = layer.id;
            ui.inspectorTarget = { kind: 'layer', eventId: ui.selectedEventId, id: layer.id };
            const startX = e.clientX;
            const origStart = layer.startMs;
            let moved = false;
            const onMove = (me) => {
              const dx = me.clientX - startX;
              if (Math.abs(dx) < 2 && !moved) return;
              moved = true;
              const nextStart = Math.round(
                finiteClamp(origStart + dx / pxPerMs, 0, ev.durationMs - 5, origStart),
              );
              void dispatchOps(
                'Move foley clip',
                [{ set: ['events', ui.selectedEventId, 'layers', layer.id, 'startMs'], value: nextStart }],
                `clip-start:${layer.id}`,
              );
            };
            const onUp = () => {
              doc.removeEventListener('pointermove', onMove);
              doc.removeEventListener('pointerup', onUp);
              if (!moved) renderAll();
            };
            doc.addEventListener('pointermove', onMove);
            doc.addEventListener('pointerup', onUp);
          },
        },
        buildClipSvg(layer),
      );

      return el(
        'div',
        { class: `gdaw-lane ${isSelected ? 'selected' : ''}` },
        el(
          'div',
          { class: 'gdaw-lane-hdr' },
          el('span', { class: 'gdaw-lane-name', title: layer.label }, layer.label),
          el(
            'div',
            { class: 'gdaw-row' },
            el(
              'button',
              {
                type: 'button',
                class: layer.enabled ? '' : 'active',
                title: 'Mute layer',
                onclick: () => {
                  void dispatchOps('Toggle layer mute', [
                    {
                      set: ['events', ui.selectedEventId, 'layers', layer.id, 'enabled'],
                      value: !layer.enabled,
                    },
                  ]);
                },
              },
              'M',
            ),
            el(
              'button',
              {
                type: 'button',
                class: layer.solo ? 'active' : '',
                title: 'Solo layer',
                onclick: () => {
                  void dispatchOps('Toggle layer solo', [
                    {
                      set: ['events', ui.selectedEventId, 'layers', layer.id, 'solo'],
                      value: !layer.solo,
                    },
                  ]);
                },
              },
              'S',
            ),
            el(
              'button',
              {
                type: 'button',
                class:
                  ui.inspectorTarget.kind === 'layer' && ui.inspectorTarget.id === layer.id
                    ? 'active'
                    : '',
                title: 'Inspect layer DSP strip',
                onclick: () => {
                  ui.selectedLayerId = layer.id;
                  ui.inspectorTarget = { kind: 'layer', eventId: ui.selectedEventId, id: layer.id };
                  renderAll();
                },
              },
              'FX',
            ),
            el(
              'button',
              {
                type: 'button',
                class: 'danger',
                title: 'Remove layer',
                onclick: () => {
                  void dispatchOps('Remove layer', [
                    { remove: ['events', ui.selectedEventId, 'layers', layer.id] },
                  ]);
                },
              },
              '✕',
            ),
          ),
        ),
        el('div', { class: 'gdaw-lane-track', style: `width: ${trackWidthPx}px;` }, clipEl),
      );
    });

    const contactLine = el(
      'div',
      {
        class: 'gdaw-contact-line',
        style: `left: ${168 + Math.round(ev.contactMarkerMs * pxPerMs)}px`,
      },
      el('span', { class: 'gdaw-contact-flag' }, `${ev.contactLabel} · ${Math.round(ev.contactMarkerMs)}ms (F${contactFrame60})`),
    );

    const playheadLine = el('div', {
      class: 'gdaw-playhead',
      style: `left: ${168 + Math.round(ui.playheadMs * pxPerMs)}px`,
    });

    const activeLayer = ev.layers[ui.selectedLayerId];
    const clipInspector = activeLayer
      ? el(
          'div',
          { class: 'gdaw-clip-bar' },
          kvNum('Start (ms)', activeLayer.startMs, 0, ev.durationMs, 1, (v) =>
            dispatchOps(
              'Set clip start',
              [{ set: ['events', ui.selectedEventId, 'layers', ui.selectedLayerId, 'startMs'], value: v }],
              `clip-start:${ui.selectedLayerId}`,
            ),
          ),
          kvNum('Length (ms)', activeLayer.durationMs, 5, 60000, 1, (v) =>
            dispatchOps(
              'Set clip duration',
              [{ set: ['events', ui.selectedEventId, 'layers', ui.selectedLayerId, 'durationMs'], value: v }],
              `clip-dur:${ui.selectedLayerId}`,
            ),
          ),
          kvNum('Trim (ms)', activeLayer.trimStartMs, 0, 60000, 1, (v) =>
            dispatchOps(
              'Set clip trim',
              [{ set: ['events', ui.selectedEventId, 'layers', ui.selectedLayerId, 'trimStartMs'], value: v }],
              `clip-trim:${ui.selectedLayerId}`,
            ),
          ),
          kvNum('Fade In (ms)', activeLayer.fadeInMs, 0, activeLayer.durationMs, 1, (v) =>
            dispatchOps(
              'Set clip fade-in',
              [{ set: ['events', ui.selectedEventId, 'layers', ui.selectedLayerId, 'fadeInMs'], value: v }],
              `clip-fi:${ui.selectedLayerId}`,
            ),
          ),
          kvNum('Fade Out (ms)', activeLayer.fadeOutMs, 0, activeLayer.durationMs, 1, (v) =>
            dispatchOps(
              'Set clip fade-out',
              [{ set: ['events', ui.selectedEventId, 'layers', ui.selectedLayerId, 'fadeOutMs'], value: v }],
              `clip-fo:${ui.selectedLayerId}`,
            ),
          ),
          el(
            'label',
            { class: 'gdaw-kv' },
            'Fade Curve',
            el(
              'select',
              {
                'data-field': 'clip-fade-curve',
                onchange: (e) =>
                  void dispatchOps('Set fade curve', [
                    {
                      set: ['events', ui.selectedEventId, 'layers', ui.selectedLayerId, 'fadeCurve'],
                      value: e.target.value,
                    },
                  ]),
              },
              ['linear', 'cosine', 's-curve'].map((c) =>
                el('option', { value: c, selected: activeLayer.fadeCurve === c }, c),
              ),
            ),
          ),
          kvNum('Clip Gain (dB)', activeLayer.gainDb, -48, 18, 0.5, (v) =>
            dispatchOps(
              'Set clip gain',
              [{ set: ['events', ui.selectedEventId, 'layers', ui.selectedLayerId, 'gainDb'], value: v }],
              `clip-gain:${ui.selectedLayerId}`,
            ),
          ),
          kvNum('Pitch (st)', activeLayer.pitchSemitones, -24, 24, 0.5, (v) =>
            dispatchOps(
              'Set clip pitch',
              [{ set: ['events', ui.selectedEventId, 'layers', ui.selectedLayerId, 'pitchSemitones'], value: v }],
              `clip-pitch:${ui.selectedLayerId}`,
            ),
          ),
          kvNum('Jitter (ct)', activeLayer.pitchJitterCents, 0, 400, 5, (v) =>
            dispatchOps(
              'Set pitch jitter',
              [{ set: ['events', ui.selectedEventId, 'layers', ui.selectedLayerId, 'pitchJitterCents'], value: v }],
              `clip-pjit:${ui.selectedLayerId}`,
            ),
          ),
          el(
            'label',
            { class: 'gdaw-kv' },
            'Bus Route',
            el(
              'select',
              {
                onchange: (e) =>
                  void dispatchOps('Route layer bus', [
                    {
                      set: ['events', ui.selectedEventId, 'layers', ui.selectedLayerId, 'bus'],
                      value: e.target.value,
                    },
                  ]),
              },
              buses.map((b) =>
                el('option', { value: b.id, selected: activeLayer.bus === b.id }, b.label),
              ),
            ),
          ),
        )
      : el('div', { class: 'gdaw-clip-bar' }, 'Select or add a layer from the SoundBank browser.');

    timelineEl.replaceChildren(
      el(
        'div',
        { class: 'gdaw-panel-hdr' },
        el('h3', {}, tr('timelineTitle')),
        el(
          'div',
          { class: 'gdaw-row' },
          el(
            'button',
            {
              type: 'button',
              class: 'primary',
              'data-action': 'trigger-event',
              onclick: () => void auditionEvent(ui.selectedEventId),
            },
            tr('triggerEvent'),
          ),
          el(
            'button',
            {
              type: 'button',
              'data-action': 'stop-audio',
              onclick: () => {
                runtime.stopAll();
                ui.playheadMs = 0;
                renderTimelinePanel();
              },
            },
            tr('stopAudio'),
          ),
          seqSelect,
          el(
            'button',
            {
              type: 'button',
              'data-action': 'play-sequence',
              onclick: () => playSequence(ui.selectedSequenceId),
            },
            tr('playSequence'),
          ),
          el(
            'button',
            {
              type: 'button',
              'data-action': 'bounce-wav',
              onclick: () => bounceEventWav(ui.selectedEventId),
            },
            tr('bounceWav'),
          ),
        ),
      ),
      el(
        'div',
        { class: 'gdaw-tl-bar' },
        eventSelect,
        el(
          'button',
          {
            type: 'button',
            'data-action': 'add-event',
            onclick: () => {
              const newId = `evt_${randomHexId(12)}`;
              const order = events.length ? Math.max(...events.map((x) => x.order ?? 0)) + 1 : 0;
              const defaultBus = buses[0]?.id || MASTER_BUS_ID;
              ui.selectedEventId = newId;
              void dispatchOps('Create event action sheet', [
                {
                  set: ['events', newId],
                  value: {
                    order,
                    cueId: newId,
                    label: `Custom Event ${events.length + 1}`,
                    bus: defaultBus,
                    durationMs: 600,
                    contactMarkerMs: 150,
                    contactLabel: 'Contact',
                    description: '',
                    layers: {},
                  },
                },
              ]);
            },
          },
          '+ Event',
        ),
        kvNum('Sheet (ms)', ev.durationMs, 60, 60000, 10, (v) =>
          dispatchOps(
            'Set sheet duration',
            [{ set: ['events', ui.selectedEventId, 'durationMs'], value: v }],
            `ev-dur:${ui.selectedEventId}`,
          ),
        ),
        kvNum('Contact (ms)', ev.contactMarkerMs, 0, ev.durationMs, 5, (v) =>
          dispatchOps(
            'Set contact marker',
            [{ set: ['events', ui.selectedEventId, 'contactMarkerMs'], value: v }],
            `contact:${ui.selectedEventId}`,
          ),
        ),
        el(
          'label',
          { class: 'gdaw-kv' },
          'Zoom',
          el('input', {
            type: 'range',
            min: '0.5',
            max: '2.5',
            step: '0.1',
            value: ui.timelineZoom,
            oninput: (e) => {
              ui.timelineZoom = Number(e.target.value) || 1;
              renderTimelinePanel();
            },
          }),
        ),
      ),
      el(
        'div',
        { class: 'gdaw-tl-scroll' },
        el(
          'div',
          { class: 'gdaw-tl-stage', style: `width: ${168 + trackWidthPx + 24}px;` },
          rulerEl,
          contactLine,
          playheadLine,
          laneEls,
        ),
      ),
      clipInspector,
    );
  }

  function kvNum(labelText, value, min, max, step, onChange) {
    return el(
      'label',
      { class: 'gdaw-kv' },
      labelText,
      el('input', {
        type: 'number',
        min,
        max,
        step,
        value: Math.round(Number(value) * 100) / 100,
        onchange: (e) => {
          const n = finiteClamp(e.target.value, min, max, value);
          void onChange(n);
        },
      }),
    );
  }

  // ---- Panel 3: Multi-Channel Mixing Console & Live Metering ----------------------------------

  function dbToPct(db) {
    return Math.max(0, Math.min(100, ((finiteClamp(db, MIN_METER_DB, 6, MIN_METER_DB) - MIN_METER_DB) / (6 - MIN_METER_DB)) * 100));
  }

  function renderConsolePanel() {
    const buses = orderedBuses(ui.root);
    const stripsEl = el('div', { class: 'gdaw-strips' });

    for (const bus of buses) {
      const m = ui.lastMeters[bus.id] || {
        peakLDb: MIN_METER_DB,
        peakRDb: MIN_METER_DB,
        rmsLDb: MIN_METER_DB,
        rmsRDb: MIN_METER_DB,
        correlation: 1,
        clipped: false,
        gainReductionDb: 0,
      };
      const holdDb = ui.peakHolds.get(bus.id) ?? Math.max(m.peakLDb, m.peakRDb);
      const isSelected = ui.inspectorTarget.kind === 'bus' && ui.inspectorTarget.id === bus.id;
      const isClip = ui.latchedClips.has(bus.id);
      const grPct = Math.min(100, (Math.max(0, m.gainReductionDb || 0) / 24) * 100);

      stripsEl.append(
        el(
          'div',
          {
            class: `gdaw-strip ${isSelected ? 'selected' : ''} ${bus.id === MASTER_BUS_ID ? 'master' : ''}`,
            'data-bus-id': bus.id,
          },
          el('div', { class: 'gdaw-strip-title', style: `color: ${bus.color}` }, bus.label),
          el(
            'div',
            {
              class: `gdaw-clip-led ${isClip ? 'hot' : ''}`,
              title: 'Click to clear latched clip indicator',
              onclick: () => {
                ui.latchedClips.delete(bus.id);
                ui.peakHolds.set(bus.id, MIN_METER_DB);
                renderConsolePanel();
              },
            },
            isClip ? 'CLIP' : `${Math.round(Math.max(m.peakLDb, m.peakRDb))} dB`,
          ),
          el(
            'div',
            { class: 'gdaw-meter-row' },
            el(
              'div',
              { class: 'gdaw-meter-bar', title: `L RMS ${m.rmsLDb.toFixed(1)} dB` },
              el('div', { class: 'gdaw-meter-rms', style: `height: ${dbToPct(m.rmsLDb).toFixed(1)}%` }),
              el('div', { class: 'gdaw-meter-peak', style: `bottom: ${dbToPct(holdDb).toFixed(1)}%` }),
            ),
            el(
              'div',
              { class: 'gdaw-meter-bar', title: `R RMS ${m.rmsRDb.toFixed(1)} dB` },
              el('div', { class: 'gdaw-meter-rms', style: `height: ${dbToPct(m.rmsRDb).toFixed(1)}%` }),
              el('div', { class: 'gdaw-meter-peak', style: `bottom: ${dbToPct(m.peakRDb).toFixed(1)}%` }),
            ),
            el(
              'div',
              { class: 'gdaw-meter-gr', title: `GR -${(m.gainReductionDb || 0).toFixed(1)} dB` },
              el('div', { class: 'gdaw-meter-gr-fill', style: `height: ${grPct.toFixed(1)}%` }),
            ),
            el(
              'div',
              { class: 'gdaw-fader-col' },
              el('input', {
                type: 'range',
                min: '0',
                max: '1.5',
                step: '0.01',
                value: bus.volume,
                'aria-label': `${bus.label} volume`,
                oninput: (e) => {
                  void dispatchOps(
                    `Set ${bus.label} fader`,
                    [{ set: ['buses', bus.id, 'volume'], value: Number(e.target.value) }],
                    `fader:${bus.id}`,
                  );
                },
              }),
            ),
          ),
          el('input', {
            type: 'range',
            min: '-1',
            max: '1',
            step: '0.02',
            value: bus.strip.pan,
            title: `Pan ${bus.strip.pan.toFixed(2)}`,
            oninput: (e) => {
              void dispatchOps(
                `Pan ${bus.label}`,
                [{ set: ['buses', bus.id, 'strip', 'pan'], value: Number(e.target.value) }],
                `pan:${bus.id}`,
              );
            },
          }),
          el(
            'div',
            { class: 'gdaw-strip-btns' },
            el(
              'button',
              {
                type: 'button',
                class: bus.muted ? 'active' : '',
                onclick: () =>
                  void dispatchOps(`Mute ${bus.label}`, [
                    { set: ['buses', bus.id, 'muted'], value: !bus.muted },
                  ]),
              },
              'M',
            ),
            el(
              'button',
              {
                type: 'button',
                disabled: bus.id === MASTER_BUS_ID,
                class: bus.solo ? 'active' : '',
                onclick: () =>
                  void dispatchOps(`Solo ${bus.label}`, [
                    { set: ['buses', bus.id, 'solo'], value: !bus.solo },
                  ]),
              },
              'S',
            ),
            el(
              'button',
              {
                type: 'button',
                class: isSelected ? 'active' : '',
                onclick: () => {
                  ui.inspectorTarget = { kind: 'bus', id: bus.id };
                  renderConsolePanel();
                  renderDspPanel();
                },
              },
              'FX',
            ),
          ),
        ),
      );
    }

    const masterMeter = ui.lastMeters[MASTER_BUS_ID] || { correlation: 1 };
    const corrPct = ((finiteClamp(masterMeter.correlation, -1, 1, 1) + 1) / 2) * 100;
    const scopeCanvas = el('canvas', { class: 'gdaw-scope-canvas', width: '162', height: '86' });
    drawScopeCanvas(scopeCanvas);

    consoleEl.replaceChildren(
      el(
        'div',
        { class: 'gdaw-panel-hdr' },
        el('h3', {}, tr('consoleTitle')),
        el(
          'div',
          { class: 'gdaw-row' },
          el(
            'button',
            {
              type: 'button',
              'data-action': 'add-bus',
              onclick: () => {
                const idx = buses.length;
                const busId = `bus_${idx}`;
                ui.inspectorTarget = { kind: 'bus', id: busId };
                void dispatchOps('Add channel bus', [
                  {
                    set: ['buses', busId],
                    value: {
                      order: idx - 1,
                      label: `Bus ${idx}`,
                      role: 'channel',
                      color: '#60a5fa',
                      volume: 1,
                      muted: false,
                      solo: false,
                      strip: createDefaultStrip(),
                    },
                  },
                ]);
              },
            },
            '+ Bus',
          ),
        ),
      ),
      el(
        'div',
        { class: 'gdaw-console-body' },
        stripsEl,
        el(
          'div',
          { class: 'gdaw-scope-box' },
          el(
            'div',
            { class: 'gdaw-row' },
            el(
              'button',
              {
                type: 'button',
                class: ui.scopeMode === 'waveform' ? 'active' : '',
                onclick: () => {
                  ui.scopeMode = 'waveform';
                  renderConsolePanel();
                },
              },
              'Wave',
            ),
            el(
              'button',
              {
                type: 'button',
                class: ui.scopeMode === 'fft' ? 'active' : '',
                onclick: () => {
                  ui.scopeMode = 'fft';
                  renderConsolePanel();
                },
              },
              'FFT',
            ),
          ),
          scopeCanvas,
          el(
            'div',
            { class: 'gdaw-kv' },
            `Phase Corr: ${(masterMeter.correlation ?? 1).toFixed(2)}`,
            el(
              'div',
              { class: 'gdaw-corr-track' },
              el('div', { class: 'gdaw-corr-dot', style: `left: ${corrPct.toFixed(1)}%` }),
            ),
          ),
        ),
      ),
    );
  }

  function drawScopeCanvas(canvas) {
    const ctx2d = canvas.getContext?.('2d');
    if (!ctx2d) return;
    const w = canvas.width, h = canvas.height;
    ctx2d.fillStyle = '#0b0d12';
    ctx2d.fillRect(0, 0, w, h);
    ctx2d.strokeStyle = '#e8b44a';
    ctx2d.lineWidth = 1.4;

    const ma = runtime.masterAnalysers;
    if (runtime.playing && ma) {
      if (ui.scopeMode === 'fft') {
        ma.anMix.getByteFrequencyData(ma.fft);
        ctx2d.fillStyle = '#38bdf8';
        const bins = 32;
        const bw = w / bins;
        for (let i = 0; i < bins; i++) {
          const v = (ma.fft[i] / 255) * (h - 6);
          ctx2d.fillRect(i * bw, h - v, Math.max(1, bw - 1), v);
        }
        return;
      }
      ma.anL.getFloatTimeDomainData(ma.bufL);
      ctx2d.beginPath();
      for (let i = 0; i < ma.bufL.length; i++) {
        const x = (i / (ma.bufL.length - 1)) * w;
        const y = h * 0.5 - ma.bufL[i] * (h * 0.42);
        if (i === 0) ctx2d.moveTo(x, y);
        else ctx2d.lineTo(x, y);
      }
      ctx2d.stroke();
      return;
    }

    // Idle reference centerline
    ctx2d.strokeStyle = '#2c2f36';
    ctx2d.beginPath();
    ctx2d.moveTo(0, h * 0.5);
    ctx2d.lineTo(w, h * 0.5);
    ctx2d.stroke();
  }

  function startMeterLoop() {
    if (ui.rafId || !globalThis.requestAnimationFrame) return;
    const tick = () => {
      ui.rafId = 0;
      if (ui.destroyed || !runtime.playing) return;
      const ctx = runtime.ctx;
      const ma = runtime.masterAnalysers;
      if (ctx && ma) {
        ui.playheadMs = Math.min(
          runtime.playDurationMs,
          (ctx.currentTime - runtime.playStartCtxTime) * 1000,
        );
        ma.anL.getFloatTimeDomainData(ma.bufL);
        ma.anR.getFloatTimeDomainData(ma.bufR);
        const prevGr = ui.lastMeters[MASTER_BUS_ID]?.gainReductionDb || 0;
        ui.lastMeters[MASTER_BUS_ID] = analyzeStereoFrames(ma.bufL, ma.bufR, prevGr);
        renderConsolePanel();
      }
      ui.rafId = globalThis.requestAnimationFrame(tick);
    };
    ui.rafId = globalThis.requestAnimationFrame(tick);
  }

  // ---- Panel 4: Interactive Per-Bus & Per-Layer DSP Insert Rack -------------------------------

  function activeStripContext() {
    if (ui.inspectorTarget.kind === 'layer') {
      const ev = ui.root.events[ui.inspectorTarget.eventId];
      const layer = ev?.layers?.[ui.inspectorTarget.id];
      if (layer) {
        return {
          title: `Clip: ${layer.label}`,
          strip: layer.strip,
          basePath: ['events', ui.inspectorTarget.eventId, 'layers', ui.inspectorTarget.id, 'strip'],
          grDb: 0,
        };
      }
    }
    const busId = ui.root.buses[ui.inspectorTarget.id] ? ui.inspectorTarget.id : MASTER_BUS_ID;
    const bus = ui.root.buses[busId];
    return {
      title: `Bus: ${bus.label}`,
      strip: bus.strip,
      basePath: ['buses', busId, 'strip'],
      grDb: ui.lastMeters[busId]?.gainReductionDb || 0,
    };
  }

  function buildEqSvg(strip) {
    const freqs = [];
    for (let i = 0; i < 48; i++) {
      freqs.push(20 * (20000 / 20) ** (i / 47));
    }
    const mags = evaluateStripResponseDb(strip, freqs, ui.root.sampleRate);
    const pts = mags.map((db, i) => {
      const x = (i / 47) * 260;
      const y = 39 - finiteClamp(db, -18, 18, 0) * (34 / 18);
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    });

    return svgEl(
      'svg',
      { class: 'gdaw-eq-svg', viewBox: '0 0 260 78', preserveAspectRatio: 'none' },
      svgEl('line', { x1: '0', y1: '39', x2: '260', y2: '39', stroke: '#2c2f36', 'stroke-width': '1' }),
      svgEl('polyline', {
        points: pts.join(' '),
        fill: 'none',
        stroke: '#e8b44a',
        'stroke-width': '1.8',
      }),
    );
  }

  function buildOverdriveSvg(drive) {
    const pts = [];
    for (let i = 0; i <= 40; i++) {
      const xNorm = (i / 40) * 2 - 1;
      const yNorm = overdriveTransferSample(xNorm, drive);
      const x = (i / 40) * 260;
      const y = 27 - yNorm * 23;
      pts.push(`${x.toFixed(1)},${y.toFixed(1)}`);
    }
    return svgEl(
      'svg',
      { class: 'gdaw-od-svg', viewBox: '0 0 260 54', preserveAspectRatio: 'none' },
      svgEl('line', { x1: '0', y1: '27', x2: '260', y2: '27', stroke: '#2c2f36', 'stroke-width': '1' }),
      svgEl('polyline', {
        points: pts.join(' '),
        fill: 'none',
        stroke: '#fb923c',
        'stroke-width': '1.8',
      }),
    );
  }

  function renderDspPanel() {
    const ctx = activeStripContext();
    const s = ctx.strip;
    const setSub = (subPath, val, mergeKey) =>
      dispatchOps(
        `DSP ${subPath.join('.')}`,
        [{ set: [...ctx.basePath, ...subPath], value: val }],
        mergeKey ? `dsp:${ctx.basePath.join('/')}:${mergeKey}` : undefined,
      );

    const slider = (labelText, val, min, max, step, subPath, fmt = (v) => String(v)) =>
      el(
        'label',
        { class: 'gdaw-kv' },
        `${labelText}: ${fmt(val)}`,
        el('input', {
          type: 'range',
          min,
          max,
          step,
          value: val,
          'data-dsp': subPath.join('.'),
          oninput: (e) => void setSub(subPath, Number(e.target.value), subPath.join('.')),
        }),
      );

    const targetSelect = el(
      'select',
      {
        'aria-label': 'DSP target',
        onchange: (e) => {
          const val = e.target.value;
          if (val.startsWith('layer:')) {
            const lid = val.slice(6);
            ui.inspectorTarget = { kind: 'layer', eventId: ui.selectedEventId, id: lid };
          } else {
            ui.inspectorTarget = { kind: 'bus', id: val.slice(4) };
          }
          renderConsolePanel();
          renderDspPanel();
        },
      },
      orderedBuses(ui.root).map((b) =>
        el(
          'option',
          {
            value: `bus:${b.id}`,
            selected: ui.inspectorTarget.kind === 'bus' && ui.inspectorTarget.id === b.id,
          },
          `Bus · ${b.label}`,
        ),
      ),
      orderedLayers(ui.root.events[ui.selectedEventId]).map((l) =>
        el(
          'option',
          {
            value: `layer:${l.id}`,
            selected: ui.inspectorTarget.kind === 'layer' && ui.inspectorTarget.id === l.id,
          },
          `Clip · ${l.label}`,
        ),
      ),
    );

    dspEl.replaceChildren(
      el(
        'div',
        { class: 'gdaw-panel-hdr' },
        el('h3', {}, tr('dspTitle')),
        el(
          'div',
          { class: 'gdaw-row' },
          targetSelect,
          el(
            'button',
            {
              type: 'button',
              class: s.bypass ? 'active' : '',
              'data-action': 'dsp-bypass',
              onclick: () => void setSub(['bypass'], !s.bypass),
            },
            'Bypass',
          ),
          el(
            'button',
            {
              type: 'button',
              'data-action': 'dsp-reset',
              onclick: () =>
                void dispatchOps('Reset DSP strip', [
                  { set: ctx.basePath, value: createDefaultStrip() },
                ]),
            },
            'Reset',
          ),
        ),
      ),
      el(
        'div',
        { class: 'gdaw-dsp-scroll' },
        // Card 1: Input Gain, Routing & Mid/Side Width
        el(
          'div',
          { class: 'gdaw-dsp-card' },
          el('div', { class: 'gdaw-dsp-card-hdr' }, 'Gain · Stereo Routing · Mid/Side Width'),
          el(
            'div',
            { class: 'gdaw-dsp-grid' },
            slider('Input Gain', s.inputGainDb, -24, 18, 0.5, ['inputGainDb'], (v) => `${v.toFixed(1)} dB`),
            slider('Pan', s.pan, -1, 1, 0.02, ['pan'], (v) => v.toFixed(2)),
            el(
              'label',
              { class: 'gdaw-kv' },
              'Stereo Mode',
              el(
                'select',
                {
                  'data-dsp': 'stereoMode',
                  onchange: (e) => void setSub(['stereoMode'], e.target.value),
                },
                ['stereo', 'mono', 'left', 'right'].map((m) =>
                  el('option', { value: m, selected: s.stereoMode === m }, m),
                ),
              ),
            ),
            slider('M/S Width', s.stereoWidthPct, 0, 200, 1, ['stereoWidthPct'], (v) => `${Math.round(v)}%`),
          ),
        ),
        // Card 2: HPF / LPF / 3-Band Parametric EQ + SVG Biquad Curve
        el(
          'div',
          { class: 'gdaw-dsp-card' },
          el(
            'div',
            { class: 'gdaw-dsp-card-hdr' },
            'HPF · LPF · 3-Band Parametric EQ',
            el(
              'div',
              { class: 'gdaw-row' },
              el(
                'button',
                {
                  type: 'button',
                  class: s.highPass.enabled ? 'active' : '',
                  onclick: () => void setSub(['highPass', 'enabled'], !s.highPass.enabled),
                },
                'HPF',
              ),
              el(
                'button',
                {
                  type: 'button',
                  class: s.lowPass.enabled ? 'active' : '',
                  onclick: () => void setSub(['lowPass', 'enabled'], !s.lowPass.enabled),
                },
                'LPF',
              ),
              el(
                'button',
                {
                  type: 'button',
                  class: s.eq.enabled ? 'active' : '',
                  'data-action': 'toggle-eq',
                  onclick: () => void setSub(['eq', 'enabled'], !s.eq.enabled),
                },
                'EQ',
              ),
            ),
          ),
          buildEqSvg(s),
          el(
            'div',
            { class: 'gdaw-dsp-grid' },
            slider('HPF Hz', s.highPass.frequencyHz, 20, 2000, 5, ['highPass', 'frequencyHz'], (v) => `${Math.round(v)} Hz`),
            slider('LPF Hz', s.lowPass.frequencyHz, 500, 20000, 50, ['lowPass', 'frequencyHz'], (v) => `${Math.round(v)} Hz`),
            slider('Low Gain', s.eq.lowGainDb, -18, 18, 0.5, ['eq', 'lowGainDb'], (v) => `${v.toFixed(1)} dB`),
            slider('Mid Gain', s.eq.midGainDb, -18, 18, 0.5, ['eq', 'midGainDb'], (v) => `${v.toFixed(1)} dB`),
            slider('Mid Freq', s.eq.midFreqHz, 150, 8000, 20, ['eq', 'midFreqHz'], (v) => `${Math.round(v)} Hz`),
            slider('High Gain', s.eq.highGainDb, -18, 18, 0.5, ['eq', 'highGainDb'], (v) => `${v.toFixed(1)} dB`),
          ),
        ),
        // Card 3: Dynamics Compressor
        el(
          'div',
          { class: 'gdaw-dsp-card' },
          el(
            'div',
            { class: 'gdaw-dsp-card-hdr' },
            `Dynamics Compressor (GR -${ctx.grDb.toFixed(1)} dB)`,
            el(
              'button',
              {
                type: 'button',
                class: s.compressor.enabled ? 'active' : '',
                'data-action': 'toggle-compressor',
                onclick: () => void setSub(['compressor', 'enabled'], !s.compressor.enabled),
              },
              s.compressor.enabled ? 'ON' : 'OFF',
            ),
          ),
          el(
            'div',
            { class: 'gdaw-dsp-grid' },
            slider('Threshold', s.compressor.thresholdDb, -48, 0, 0.5, ['compressor', 'thresholdDb'], (v) => `${v.toFixed(1)} dB`),
            slider('Ratio', s.compressor.ratio, 1, 20, 0.5, ['compressor', 'ratio'], (v) => `${v.toFixed(1)}:1`),
            slider('Attack', s.compressor.attackSec, 0.001, 0.2, 0.001, ['compressor', 'attackSec'], (v) => `${Math.round(v * 1000)} ms`),
            slider('Release', s.compressor.releaseSec, 0.01, 1.0, 0.01, ['compressor', 'releaseSec'], (v) => `${Math.round(v * 1000)} ms`),
            slider('Makeup', s.compressor.makeupGainDb, -6, 18, 0.5, ['compressor', 'makeupGainDb'], (v) => `${v.toFixed(1)} dB`),
            slider('Mix', s.compressor.mix, 0, 1, 0.02, ['compressor', 'mix'], (v) => `${Math.round(v * 100)}%`),
          ),
        ),
        // Card 4: Soft-Clip Overdrive
        el(
          'div',
          { class: 'gdaw-dsp-card' },
          el(
            'div',
            { class: 'gdaw-dsp-card-hdr' },
            'Overdrive / Harmonic Saturation',
            el(
              'button',
              {
                type: 'button',
                class: s.overdrive.enabled ? 'active' : '',
                'data-action': 'toggle-overdrive',
                onclick: () => void setSub(['overdrive', 'enabled'], !s.overdrive.enabled),
              },
              s.overdrive.enabled ? 'ON' : 'OFF',
            ),
          ),
          buildOverdriveSvg(s.overdrive.drive),
          el(
            'div',
            { class: 'gdaw-dsp-grid' },
            slider('Drive', s.overdrive.drive, 0, 100, 1, ['overdrive', 'drive'], (v) => `${Math.round(v)}%`),
            slider('Mix', s.overdrive.mix, 0, 1, 0.02, ['overdrive', 'mix'], (v) => `${Math.round(v * 100)}%`),
            slider('Tone LPF', s.overdrive.toneHz, 800, 18000, 100, ['overdrive', 'toneHz'], (v) => `${Math.round(v)} Hz`),
            el(
              'label',
              { class: 'gdaw-kv' },
              'Oversample',
              el(
                'select',
                {
                  onchange: (e) => void setSub(['overdrive', 'oversample'], e.target.value),
                },
                ['none', '2x', '4x'].map((m) =>
                  el('option', { value: m, selected: s.overdrive.oversample === m }, m),
                ),
              ),
            ),
          ),
        ),
        // Card 5: Stereo Delay & Room Reverb
        el(
          'div',
          { class: 'gdaw-dsp-card' },
          el(
            'div',
            { class: 'gdaw-dsp-card-hdr' },
            'Stereo Delay & Room Reverb',
            el(
              'div',
              { class: 'gdaw-row' },
              el(
                'button',
                {
                  type: 'button',
                  class: s.delay.enabled ? 'active' : '',
                  onclick: () => void setSub(['delay', 'enabled'], !s.delay.enabled),
                },
                'Delay',
              ),
              el(
                'button',
                {
                  type: 'button',
                  class: s.reverb.enabled ? 'active' : '',
                  onclick: () => void setSub(['reverb', 'enabled'], !s.reverb.enabled),
                },
                'Reverb',
              ),
            ),
          ),
          el(
            'div',
            { class: 'gdaw-dsp-grid' },
            slider('Delay Time', s.delay.timeMs, 10, 1000, 5, ['delay', 'timeMs'], (v) => `${Math.round(v)} ms`),
            slider('Feedback', s.delay.feedback, 0, 0.9, 0.02, ['delay', 'feedback'], (v) => `${Math.round(v * 100)}%`),
            slider('Delay Mix', s.delay.mix, 0, 1, 0.02, ['delay', 'mix'], (v) => `${Math.round(v * 100)}%`),
            slider('Room Size', s.reverb.roomSize, 0, 1, 0.02, ['reverb', 'roomSize'], (v) => `${Math.round(v * 100)}%`),
            slider('HF Damp', s.reverb.damping, 0, 1, 0.02, ['reverb', 'damping'], (v) => `${Math.round(v * 100)}%`),
            slider('Reverb Mix', s.reverb.mix, 0, 1, 0.02, ['reverb', 'mix'], (v) => `${Math.round(v * 100)}%`),
          ),
        ),
      ),
    );
  }

  function renderAll() {
    if (ui.destroyed) return;
    renderToolbar();
    renderBrowserPanel();
    renderTimelinePanel();
    renderConsolePanel();
    renderDspPanel();
  }

  // ---- Initial Studio Wiring & Subscriptions --------------------------------------------------

  refreshDocumentCatalog();
  const activeSnap = studioApi.getActiveDocument?.() || studioApi.getDocument?.();
  if (
    activeSnap &&
    (String(activeSnap.path || '').endsWith('.soundbank.json') ||
      activeSnap.kind === SOUNDBANK_KIND ||
      activeSnap.root?.ganbate === SOUNDBANK_KIND)
  ) {
    ingestSnapshot(activeSnap);
  } else if (ui.soundbankDocs.length > 0 && typeof studioApi.openDocument === 'function') {
    const first = ui.soundbankDocs[0];
    void studioApi.openDocument(first.id || first.path).then((opened) => {
      if (opened && !ui.destroyed) {
        ingestSnapshot(opened);
        renderAll();
      }
    });
  } else {
    syncSelectionInvariants();
    recomputeOfflineMeters();
  }

  if (typeof studioApi.on === 'function') {
    unsubs.push(
      studioApi.on('doc', (payload) => {
        if (payload && typeof payload === 'object') {
          if (
            payload.id === ui.docId ||
            String(payload.path || '').endsWith('.soundbank.json') ||
            payload.root?.ganbate === SOUNDBANK_KIND
          ) {
            ingestSnapshot(payload);
            renderAll();
          }
        }
      }),
      studioApi.on('assets', () => {
        refreshDocumentCatalog();
        renderToolbar();
        renderBrowserPanel();
      }),
      studioApi.on('locale', () => renderAll()),
    );
  } else if (typeof studioApi.subscribe === 'function') {
    unsubs.push(
      studioApi.subscribe((snap) => {
        refreshDocumentCatalog();
        if (snap?.activeDocument) ingestSnapshot(snap.activeDocument);
        renderAll();
      }),
    );
  }

  renderAll();

  function destroy() {
    if (ui.destroyed) return;
    ui.destroyed = true;
    if (ui.rafId && globalThis.cancelAnimationFrame) {
      globalThis.cancelAnimationFrame(ui.rafId);
      ui.rafId = 0;
    }
    while (unsubs.length) {
      const off = unsubs.pop();
      try { off?.(); } catch { /* ignore */ }
    }
    runtime.destroy();
    if (rootEl.parentNode === container) rootEl.remove?.();
  }

  return {
    destroy,
    unmount: destroy,
    refresh(snapshot) {
      refreshDocumentCatalog();
      if (snapshot?.activeDocument) ingestSnapshot(snapshot.activeDocument);
      renderAll();
    },
    setDocument(pathOrId, docSnap) {
      if (pathOrId && typeof pathOrId === 'object' && docSnap === undefined) {
        ingestSnapshot(pathOrId);
      } else if (docSnap) {
        ingestSnapshot(docSnap);
      } else if (typeof pathOrId === 'string' && typeof studioApi.openDocument === 'function') {
        void studioApi.openDocument(pathOrId).then((opened) => {
          if (opened && !ui.destroyed) {
            ingestSnapshot(opened);
            renderAll();
          }
        });
        return;
      }
      renderAll();
    },
    onActivate() {
      ui.active = true;
      refreshDocumentCatalog();
      renderAll();
    },
    onDeactivate() {
      ui.active = false;
      runtime.stopAll();
    },
    getState() {
      return {
        docId: ui.docId,
        docPath: ui.docPath,
        dirty: ui.dirty,
        root: structuredClone(ui.root),
        selectedEventId: ui.selectedEventId,
        selectedLayerId: ui.selectedLayerId,
        inspectorTarget: { ...ui.inspectorTarget },
        meters: { ...ui.lastMeters },
      };
    },
    auditionEvent,
    playSequence,
    stopPlayback: () => runtime.stopAll(),
    bounceEventWav,
  };
}

export default mountAudioDaw;
