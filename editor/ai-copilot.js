/**
 * Ganbate Studio — Single-Application AI Copilot, Starter World Templates,
 * Genre-Agnostic Schema Hub & Token-Compact AI Context Inspector.
 *
 * Pure engine module (`ganbate-engine`): zero game-domain hardcoding. Dynamically adapts to
 * whatever game project manifest, schemas, bricks, and workbenches are loaded.
 */

export const UX_TIERS = Object.freeze({
  creator: {
    id: 'creator',
    visibleEngineTabs: new Set(['world', 'model', 'character', 'sequencer']),
  },
  studio: {
    id: 'studio',
    visibleEngineTabs: new Set([
      'world',
      'model',
      'material',
      'character',
      'sequencer',
      'vfx',
      'audio',
      'layout-rig',
    ]),
  },
  engineer: {
    id: 'engineer',
    visibleEngineTabs: null, // all tabs visible
  },
});

const GBS_DIRECT_PREFIX = /^(tx\s+|look\s+patch\s+|cam\s+frame\s+)/i;

function isDirectGbsLine(line) {
  if (GBS_DIRECT_PREFIX.test(line)) return true;
  if (/^(spawn|place|ring|attach|move|rotate|look_at)\b/.test(line)) {
    return /\$|\[.*\]|\b(radius|face|snap|yaw|pitch|roll|gap|clearance)=|\bsnap\s+onto\b|;\s*\w/.test(
      line,
    ) || /^(spawn|look_at)\s+[A-Za-z0-9_]+\s+(empty|all|@selection|[a-z]+:)/.test(line);
  }
  return false;
}

const LIGHTING_KEYWORDS = [
  { re: /\b(night|midnight|moon|dark)\b/i, time: 22.0, preset: 'night', label: 'Night (22:00)' },
  { re: /\b(dusk|sunset|evening|golden\s*hour)\b/i, time: 18.2, preset: 'dusk', label: 'Dusk (18:12)' },
  { re: /\b(dawn|sunrise|morning)\b/i, time: 6.8, preset: 'dawn', label: 'Morning (06:48)' },
  { re: /\b(noon|midday|daylight|sunny|bright)\b/i, time: 12.5, preset: 'day', label: 'Midday (12:30)' },
];

export const STARTER_TEMPLATES = Object.freeze([
  {
    id: 'courtyard_walkway',
    title: 'Outdoor Courtyard & Walkway',
    summary: 'Arrival spawn, plaza centerpiece, 8-point perimeter ring, and golden-hour framing.',
    time: 18.0,
    buildGbs(catalogItems = []) {
      const lamp = matchBestCatalogSource(catalogItems, ['lantern', 'lamp', 'beacon', 'light', 'pillar']);
      const center = matchBestCatalogSource(catalogItems, ['fountain', 'statue', 'tree', 'stage', 'table']);
      const lines = [
        'tx "Starter Template: Courtyard & Walkway"',
        `spawn PlazaCenter ${center || 'empty'} name="Plaza_Centerpiece"`,
        'place $PlazaCenter at [0, 0, 0]',
        'spawn ArrivalPoint empty name="Player_Arrival_Spawn"',
        'place $ArrivalPoint at [0, 0, 8]',
      ];
      if (lamp) {
        lines.push(`ring 8 ${lamp} around $PlazaCenter radius=6.0 face=in snap=ground name="Perimeter"`);
      } else {
        for (let i = 0; i < 6; i++) {
          const angle = (i * Math.PI * 2) / 6;
          const x = Number((Math.cos(angle) * 5.5).toFixed(2));
          const z = Number((Math.sin(angle) * 5.5).toFixed(2));
          lines.push(`spawn Marker_${i + 1} empty name="Courtyard_Marker_${i + 1}"`);
          lines.push(`place $Marker_${i + 1} at [${x}, 0, ${z}]`);
        }
      }
      lines.push('cam frame all azimuth=35 elevation=24 padding=1.35');
      return lines.join('\n');
    },
  },
  {
    id: 'grand_hall_stage',
    title: 'Grand Interior Hall & Stage',
    summary: 'Symmetric column hall, focal stage/table anchor, and dramatic interior framing.',
    time: 20.5,
    buildGbs(catalogItems = []) {
      const stageSrc = matchBestCatalogSource(catalogItems, ['table', 'stage', 'desk', 'altar', 'pedestal']);
      const seatSrc = matchBestCatalogSource(catalogItems, ['chair', 'stool', 'seat', 'pillar', 'column']);
      const lines = [
        'tx "Starter Template: Grand Interior Hall"',
        `spawn FocalStage ${stageSrc || 'empty'} name="Main_Focal_Stage"`,
        'place $FocalStage at [0, 0, 0]',
        'spawn HallEntrance empty name="Hall_Entrance_Trigger"',
        'place $HallEntrance at [0, 0, 7.5]',
      ];
      if (seatSrc) {
        lines.push(`ring 6 ${seatSrc} around $FocalStage radius=2.6 face=in snap=ground name="Station"`);
      }
      lines.push('cam frame $FocalStage azimuth=28 elevation=22 padding=1.6');
      return lines.join('\n');
    },
  },
  {
    id: 'character_showcase',
    title: 'Character & Prop Turntable',
    summary: 'Hero subject mark with Key / Fill / Rim reference anchors and portrait framing.',
    time: 17.2,
    buildGbs(catalogItems = []) {
      const heroSrc = matchBestCatalogSource(catalogItems, ['char', 'actor', 'avatar', 'hero', 'npc', 'model']);
      return [
        'tx "Starter Template: Character Turntable"',
        `spawn HeroSubject ${heroSrc || 'empty'} name="Hero_Subject"`,
        'place $HeroSubject at [0, 0, 0]',
        'spawn KeyLightAnchor empty name="Key_Light_Rig"',
        'place $KeyLightAnchor at [1.8, 2.2, 2.1]',
        'spawn RimLightAnchor empty name="Rim_Light_Rig"',
        'place $RimLightAnchor at [-1.6, 2.4, -1.8]',
        'cam frame $HeroSubject azimuth=18 elevation=14 padding=1.35',
      ].join('\n');
    },
  },
  {
    id: 'trigger_arena',
    title: 'Playable Trigger & Obstacle Greybox',
    summary: 'Player start, stepping waypoints, interaction trigger zone, and overview camera.',
    time: 13.0,
    buildGbs(catalogItems = []) {
      const propSrc = matchBestCatalogSource(catalogItems, ['box', 'crate', 'block', 'rock', 'prop']);
      const lines = [
        'tx "Starter Template: Playable Trigger Greybox"',
        'spawn PlayerStart empty name="Player_Spawn"',
        'place $PlayerStart at [0, 0, 6]',
        `spawn StepA ${propSrc || 'empty'} name="Waypoint_1"`,
        'place $StepA at [-1.5, 0, 3]',
        `spawn StepB ${propSrc || 'empty'} name="Waypoint_2"`,
        'place $StepB at [1.5, 0, 0]',
        'spawn GoalTrigger empty name="Goal_Trigger_Zone"',
        'place $GoalTrigger at [0, 0, -4]',
        'cam frame all azimuth=40 elevation=28 padding=1.4',
      ];
      return lines.join('\n');
    },
  },
]);

/**
 * Scores and picks the best catalog source token (`brick:...`, `prefab:...`, or asset id/key)
 * matching keywords from a user prompt or template.
 */
export function matchBestCatalogSource(catalogItems = [], keywords = []) {
  if (!Array.isArray(catalogItems) || !catalogItems.length) return null;
  const cleanTokens = keywords
    .map((k) => String(k || '').toLowerCase().trim())
    .filter((k) => k.length >= 2);
  if (!cleanTokens.length) return null;

  let bestScore = 0;
  let bestKey = null;
  for (const item of catalogItems) {
    const kind = String(item.kind || '').toLowerCase();
    if (kind && !['brick', 'prefab', 'model'].includes(kind)) continue;
    const hay = `${item.name || ''} ${item.path || ''} ${item.key || ''} ${item.signature || ''}`.toLowerCase();
    let score = 0;
    for (const tok of cleanTokens) {
      if (hay.includes(tok)) {
        score += 10;
        if ((item.name || '').toLowerCase() === tok) score += 15;
      }
    }
    if (score > 0) {
      if (kind === 'brick') score += 4;
      else if (kind === 'prefab') score += 3;
      else if (kind === 'model') score += 2;
      if (score > bestScore) {
        bestScore = score;
        bestKey = item.key || item.brickId || item.id || null;
      }
    }
  }
  return bestKey;
}

function sanitizeAlias(raw, fallback = 'Item') {
  const cleaned = String(raw || '')
    .replace(/[^a-zA-Z0-9_]+/g, '_')
    .replace(/^_+|_+$/g, '');
  if (!cleaned) return fallback;
  return /^[0-9]/.test(cleaned) ? `E_${cleaned}` : cleaned.slice(0, 28);
}

/**
 * Compiles a plain-English creator prompt (or direct `.gbs` snippet) into an actionable plan:
 * `{ gbsScript, timeOfDay, lightingPreset, summary }` — with zero domain hardcoding.
 */
export function compileNaturalPromptToGbs(promptText, context = {}) {
  const raw = String(promptText || '').trim();
  if (!raw) return { gbsScript: '', timeOfDay: null, lightingPreset: null, summary: 'Empty prompt' };

  // Direct `.gbs` passthrough when the author or LLM already wrote `.gbs`
  const firstLine = raw.split(/\r?\n|;/)[0].trim();
  if (isDirectGbsLine(firstLine)) {
    const normalized = raw
      .split(/\r?\n|;\s*/)
      .map((l) => l.trim())
      .filter(Boolean)
      .join('\n');
    return {
      gbsScript: normalized,
      timeOfDay: null,
      lightingPreset: null,
      summary: 'Direct .gbs spatial script',
    };
  }

  const catalogItems = Array.isArray(context.catalogItems) ? context.catalogItems : [];
  const hasSelection = Boolean(context.hasSelection);
  const cursor = Array.isArray(context.cursorWorld) && context.cursorWorld.length === 3
    ? context.cursorWorld
    : [0, 0, 0];

  let timeOfDay = null;
  let lightingPreset = null;
  const summaryParts = [];
  const gbsLines = [];

  for (const lk of LIGHTING_KEYWORDS) {
    if (lk.re.test(raw)) {
      timeOfDay = lk.time;
      lightingPreset = lk.preset;
      summaryParts.push(`Set atmosphere to ${lk.label}`);
      break;
    }
  }

  const stopWords = new Set([
    'create', 'add', 'spawn', 'place', 'put', 'make', 'build', 'drop', 'around', 'ring', 'circle',
    'surrounding', 'row', 'line', 'path', 'along', 'beside', 'next', 'to', 'left', 'right', 'front',
    'back', 'above', 'selection', 'selected', 'ground', 'floor', 'at', 'with', 'radius', 'spacing',
    'the', 'a', 'an', 'of', 'in', 'on', 'and', 'then', 'frame', 'camera', 'all', 'set', 'time',
    'night', 'dusk', 'sunset', 'dawn', 'morning', 'noon', 'daylight', 'meters', 'meter', 'm',
  ]);
  const contentWords = raw
    .toLowerCase()
    .replace(/[^a-z0-9_\-\s]/g, ' ')
    .split(/\s+/)
    .map((w) => w.replace(/s$/, ''))
    .filter((w) => w.length >= 2 && !stopWords.has(w) && !/^\d+$/.test(w));

  const matchedSource = matchBestCatalogSource(catalogItems, contentWords);
  const baseLabel = contentWords[0] ? sanitizeAlias(contentWords[0], 'Object') : 'Object';

  // 1. Ring / Circle / Surround pattern
  const ringMatch = raw.match(/\b(?:ring|circle|surround\w*)\b/i);
  if (ringMatch) {
    const countMatch = raw.match(/\b(\d{1,2})\b/);
    const count = Math.min(64, Math.max(2, Number(countMatch?.[1] || 6)));
    const radMatch = raw.match(/radius\s*[=:]?\s*(\d+(?:\.\d+)?)|(\d+(?:\.\d+)?)\s*m\b/i);
    const radius = Number(radMatch?.[1] || radMatch?.[2] || 4.0);
    const centerTok = hasSelection ? '@selection' : `[${cursor.join(', ')}]`;
    gbsLines.push(`tx "AI Prompt: Ring of ${count} ${baseLabel}"`);
    if (matchedSource) {
      gbsLines.push(
        `ring ${count} ${matchedSource} around ${centerTok} radius=${radius} face=in snap=ground name="${baseLabel}"`,
      );
    } else {
      for (let i = 0; i < count; i++) {
        const ang = (i * Math.PI * 2) / count;
        const x = Number((cursor[0] + Math.cos(ang) * radius).toFixed(2));
        const z = Number((cursor[2] + Math.sin(ang) * radius).toFixed(2));
        gbsLines.push(`spawn ${baseLabel}_${i + 1} empty name="${baseLabel}_${i + 1}"`);
        gbsLines.push(`place $${baseLabel}_${i + 1} at [${x}, ${cursor[1]}, ${z}]`);
      }
    }
    summaryParts.push(`Arranged ${count} ${baseLabel} items in a ${radius}m ring`);
  } else if (/\b(row|line|path|walkway|avenue)\b/i.test(raw)) {
    // 2. Row / Path / Line pattern
    const countMatch = raw.match(/\b(\d{1,2})\b/);
    const count = Math.min(32, Math.max(2, Number(countMatch?.[1] || 5)));
    const spMatch = raw.match(/(?:spacing|every|gap)\s*[=:]?\s*(\d+(?:\.\d+)?)|(\d+(?:\.\d+)?)\s*m\b/i);
    const spacing = Number(spMatch?.[1] || spMatch?.[2] || 3.0);
    const startZ = cursor[2] - ((count - 1) * spacing) / 2;
    gbsLines.push(`tx "AI Prompt: Path of ${count} ${baseLabel}"`);
    for (let i = 0; i < count; i++) {
      const z = Number((startZ + i * spacing).toFixed(2));
      const alias = `${baseLabel}_${i + 1}`;
      gbsLines.push(`spawn ${alias} ${matchedSource || 'empty'} name="${alias}"`);
      if (matchedSource) {
        gbsLines.push(`place $${alias} snap onto ground at [${cursor[0]}, ${z}]`);
      } else {
        gbsLines.push(`place $${alias} at [${cursor[0]}, ${cursor[1]}, ${z}]`);
      }
    }
    summaryParts.push(`Placed ${count} ${baseLabel} items along a ${spacing}m spaced line`);
  } else if (/\b(spawn|add|place|create|put|drop)\b/i.test(raw)) {
    // 3. Single / Coordinate / Beside placement
    const coordMatch = raw.match(/(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)/);
    const dirMatch = raw.match(/\b(left|right|front|back|above|below)\b/i);
    const alias = `${baseLabel}_${Math.floor(Math.random() * 90 + 10)}`;
    gbsLines.push(`tx "AI Prompt: Place ${baseLabel}"`);
    gbsLines.push(`spawn ${alias} ${matchedSource || 'empty'} name="${baseLabel}"`);
    if (coordMatch) {
      gbsLines.push(
        `place $${alias} at [${Number(coordMatch[1])}, ${Number(coordMatch[2])}, ${Number(coordMatch[3])}]`,
      );
    } else if (hasSelection && (/\bbeside|next\s+to\b/i.test(raw) || dirMatch)) {
      const dir = (dirMatch?.[1] || 'right').toLowerCase();
      gbsLines.push(`place $${alias} beside @selection ${dir} gap=0.35`);
    } else if (matchedSource) {
      gbsLines.push(`place $${alias} snap onto ground at [${cursor[0]}, ${cursor[2]}]`);
    } else {
      gbsLines.push(`place $${alias} at [${cursor.join(', ')}]`);
    }
    summaryParts.push(
      matchedSource
        ? `Placed catalog asset (${matchedSource}) as ${baseLabel}`
        : `Created scene entity ${baseLabel}`,
    );
  }

  // 4. Move / Rotate active selection
  const rotMatch = raw.match(/\brotate\b.*?(?:yaw\s*[=:]?\s*)?(-?\d+(?:\.\d+)?)/i);
  if (rotMatch && hasSelection && !gbsLines.length) {
    gbsLines.push(`tx "AI Prompt: Rotate selection"`);
    gbsLines.push(`rotate @selection yaw=${Number(rotMatch[1])}`);
    summaryParts.push(`Rotated @selection yaw=${Number(rotMatch[1])}°`);
  }

  const moveUpMatch = raw.match(/\bmove\s+(?:up|down)\s+(-?\d+(?:\.\d+)?)/i);
  if (moveUpMatch && hasSelection && !gbsLines.length) {
    const sign = /\bdown\b/i.test(raw) ? -1 : 1;
    const dy = Number(moveUpMatch[1]) * sign;
    gbsLines.push(`tx "AI Prompt: Move selection"`);
    gbsLines.push(`move @selection by [0, ${dy}, 0]`);
    summaryParts.push(`Moved @selection by Y=${dy}m`);
  }

  // 5. Camera frame command
  if (/\b(frame|focus|zoom\s+to)\b/i.test(raw)) {
    const subj = /\ball|scene|world\b/i.test(raw) || !hasSelection ? 'all' : '@selection';
    gbsLines.push(`cam frame ${subj} azimuth=32 elevation=22 padding=1.35`);
    summaryParts.push(`Framed camera on ${subj}`);
  }

  return {
    gbsScript: gbsLines.join('\n'),
    timeOfDay,
    lightingPreset,
    summary: summaryParts.join(' · ') || 'No matching spatial rule (configure LLM Agent API or use .gbs)',
  };
}

/**
 * Builds a client-side fallback `<800-token` L0/L1 Project Context Digest when running in
 * Cloud/Vite mode without the native Rust WebSocket `project-context` command.
 */
export function buildFallbackProjectContext({
  projectName = 'Ganbate Project',
  assets = [],
  schemas = [],
  selection = {},
  catalogItems = [],
  workbench = 'world',
} = {}) {
  const counts = {};
  const scenes = [];
  for (const a of assets) {
    const k = a.kind || 'unknown';
    counts[k] = (counts[k] || 0) + 1;
    if (k === 'scene') scenes.push({ id: a.id, path: a.path });
  }
  const schemaNames = Array.isArray(schemas)
    ? schemas.map((s) => (typeof s === 'string' ? s : s.kind || s.id || '')).filter(Boolean)
    : Object.keys(schemas || {});
  const topSigs = catalogItems
    .slice(0, 20)
    .map((it) => it.signature || `${it.kind || 'asset'}:${it.key || it.id} (${it.path || it.name})`);
  const gbsGrammar =
    'tx "Label" | spawn <alias> <brick|prefab|model|empty> [name="..."] | place <target> at [x,y,z] | ' +
    'place <target> snap onto <ground|entity> [at [x,z]] | ring <N> <source> around <center|@selection> [radius=2.0] [snap=ground] | ' +
    'attach <target> <schema_kind> [field=val] | rotate <target> yaw=<deg> | cam frame <subject|@selection|all>';

  const countsLine = Object.entries(counts)
    .map(([k, v]) => `${k}=${v}`)
    .join(', ');
  const digestText = [
    `# Ganbate AI Context Digest: ${projectName}`,
    `ActiveDoc: ${selection?.docPath || selection?.doc || 'none'} | Workbench: ${workbench}`,
    `Inventory: ${countsLine || 'empty'}`,
    `Scenes: ${scenes.map((s) => s.path).join(', ') || 'none'}`,
    `Schemas: ${schemaNames.join(', ') || 'builtin'}`,
    `GBS_DSL: ${gbsGrammar}`,
    ...(topSigs.length ? ['Catalog_Top:', ...topSigs.map((s) => `  ${s}`)] : []),
  ].join('\n');

  return {
    ok: true,
    projectName,
    estimatedTokens: Math.ceil(digestText.length / 4),
    digestText,
    countsByKind: counts,
    scenes,
    schemas: schemaNames,
    selection,
    topSignatures: topSigs,
    gbsGrammar,
  };
}

/**
 * Extracts executable `.gbs` lines from an external LLM completion response.
 */
export function extractGbsFromLlmReply(content = '') {
  const fenced = String(content || '').match(/```(?:gbs|text)?\s*([\s\S]*?)```/i);
  const candidate = fenced ? fenced[1] : String(content || '');
  return candidate
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l && !l.startsWith('#') && /^(tx|spawn|place|ring|attach|move|rotate|look_at|cam\s+frame|look\s+patch)\b/i.test(l))
    .join('\n');
}

const STORAGE_LLM_CFG = 'ganbate.studio.aiAgentConfig';

function loadAgentConfig() {
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_LLM_CFG);
    if (raw) return JSON.parse(raw);
  } catch { /* ignore */ }
  return {
    endpoint: '',
    model: 'gpt-4o-mini',
    apiKey: '',
  };
}

function saveAgentConfig(cfg) {
  try {
    globalThis.localStorage?.setItem(STORAGE_LLM_CFG, JSON.stringify(cfg));
  } catch { /* ignore */ }
}

function domEl(doc, tag, className = '', text = '') {
  const node = doc.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  return node;
}

/**
 * Mounts the docked Single-App AI Copilot, Starter Templates & AI Context drawer.
 */
export function mountAiCopilot(container, hooks = {}) {
  const doc = container.ownerDocument || globalThis.document;
  let activeTab = 'chat';
  let catalogCache = [];
  let contextCache = null;
  const agentCfg = loadAgentConfig();
  const messages = [
    {
      role: 'assistant',
      text: 'Welcome! Describe what to build in plain English (or connect any LLM / MCP Agent). Every edit runs as one validated, undoable transaction.',
      code: '',
      canUndo: false,
    },
  ];

  const root = domEl(doc, 'aside', 'gb-copilot-drawer');
  root.hidden = true;
  root.dataset.role = 'ai-copilot-drawer';

  const head = domEl(doc, 'header', 'gb-copilot-head');
  const titleWrap = domEl(doc, 'div', 'gb-copilot-title');
  titleWrap.append(
    domEl(doc, 'strong', '', 'AI Copilot & World Builder'),
    domEl(doc, 'span', '', 'Genre-Agnostic Single-App Studio · Token-Compact Context'),
  );
  const closeBtn = domEl(doc, 'button', 'mini', '✕');
  closeBtn.type = 'button';
  closeBtn.addEventListener('click', () => api.close());
  head.append(titleWrap, closeBtn);

  const tabBar = domEl(doc, 'div', 'gb-copilot-tabs');
  const TABS = [
    ['chat', 'Prompt & Copilot'],
    ['templates', 'Templates & Game'],
    ['context', 'AI Context & MCP'],
  ];
  const tabButtons = new Map();
  for (const [id, label] of TABS) {
    const b = domEl(doc, 'button', 'gb-copilot-tab', label);
    b.type = 'button';
    b.dataset.copilotTab = id;
    b.addEventListener('click', () => {
      activeTab = id;
      void refreshCatalogAndContext();
      renderBody();
    });
    tabButtons.set(id, b);
    tabBar.append(b);
  }

  const body = domEl(doc, 'div', 'gb-copilot-body');
  const compose = domEl(doc, 'div', 'gb-copilot-compose');
  const promptInput = domEl(doc, 'textarea');
  promptInput.placeholder =
    'Describe scene edits (e.g. "Ring 6 lanterns around selection radius 4m at dusk" or "Create a path of 5 trees")…';
  const composeRow = domEl(doc, 'div', 'gb-copilot-compose-row');
  const statusSpan = domEl(doc, 'span', 'meta', '@selection live');
  const sendBtn = domEl(doc, 'button', 'mini on', 'Run Prompt');
  sendBtn.type = 'button';
  composeRow.append(statusSpan, sendBtn);
  compose.append(promptInput, composeRow);

  root.append(head, tabBar, body, compose);
  container.append(root);

  async function refreshCatalogAndContext() {
    try {
      if (typeof hooks.queryCatalog === 'function') {
        const items = await hooks.queryCatalog('');
        if (Array.isArray(items)) catalogCache = items;
      }
      if (typeof hooks.fetchProjectContext === 'function') {
        contextCache = await hooks.fetchProjectContext();
      }
    } catch { /* ignore */ }
    if (!contextCache) {
      contextCache = buildFallbackProjectContext({
        projectName: hooks.getProjectName?.() || 'Ganbate Project',
        assets: hooks.listAssets?.() || [],
        schemas: hooks.getSchemas?.() || {},
        selection: hooks.getSelection?.() || {},
        catalogItems: catalogCache,
        workbench: hooks.getActiveWorkbench?.() || 'world',
      });
    }
  }

  async function ensureEditableSceneOpen(slug = 'starter_world') {
    if (hooks.hasOpenScene?.()) return true;
    const assets = hooks.listAssets?.() || [];
    const firstScene = assets.find((a) => a.kind === 'scene');
    if (firstScene && typeof hooks.openDocument === 'function') {
      await hooks.openDocument(firstScene.id);
      return Boolean(hooks.hasOpenScene?.());
    }
    if (typeof hooks.createScene === 'function') {
      await hooks.createScene(`content/scenes/${slug}.scene.json`, slug);
      return Boolean(hooks.hasOpenScene?.());
    }
    return false;
  }

  async function executePrompt(rawPrompt) {
    const clean = String(rawPrompt || '').trim();
    if (!clean) return;
    messages.push({ role: 'user', text: clean, code: '', canUndo: false });
    renderBody();

    await refreshCatalogAndContext();
    await ensureEditableSceneOpen('copilot_scene');

    let gbsScript = '';
    let summary = '';
    let timeOfDay = null;
    let lightingPreset = null;

    // Use external LLM endpoint if configured, otherwise instant built-in intent compiler
    if (agentCfg.endpoint?.trim() && typeof globalThis.fetch === 'function') {
      try {
        statusSpan.textContent = 'Querying AI Agent endpoint…';
        const sys = [
          'You are the built-in spatial architect for Ganbate Studio.',
          'Respond ONLY with valid line-oriented .gbs statements inside a ```gbs code fence.',
          contextCache?.digestText || '',
        ].join('\n\n');
        const headers = { 'Content-Type': 'application/json' };
        if (agentCfg.apiKey?.trim()) headers.Authorization = `Bearer ${agentCfg.apiKey.trim()}`;
        const res = await globalThis.fetch(agentCfg.endpoint.trim(), {
          method: 'POST',
          headers,
          body: JSON.stringify({
            model: agentCfg.model || 'gpt-4o-mini',
            temperature: 0.1,
            messages: [
              { role: 'system', content: sys },
              { role: 'user', content: clean },
            ],
          }),
        });
        if (res.ok) {
          const data = await res.json();
          const replyText = data?.choices?.[0]?.message?.content || data?.reply || '';
          gbsScript = extractGbsFromLlmReply(replyText);
          if (gbsScript) summary = 'Compiled via connected LLM Agent API';
        }
      } catch (err) {
        summary = `LLM endpoint fallback (${err.message})`;
      }
    }

    if (!gbsScript) {
      const sel = hooks.getSelection?.() || {};
      const plan = compileNaturalPromptToGbs(clean, {
        catalogItems: catalogCache,
        hasSelection: Array.isArray(sel.entityIds) && sel.entityIds.length > 0,
        cursorWorld: sel.cursorWorld || [0, 0, 0],
      });
      gbsScript = plan.gbsScript;
      timeOfDay = plan.timeOfDay;
      lightingPreset = plan.lightingPreset;
      summary = summary ? `${summary} · ${plan.summary}` : plan.summary;
    }

    if (timeOfDay != null && typeof hooks.setTimeOfDay === 'function') {
      hooks.setTimeOfDay(timeOfDay, lightingPreset);
    }

    let appliedOk = false;
    let verifyNote = '';
    if (gbsScript && typeof hooks.applyGbs === 'function') {
      const res = await hooks.applyGbs(gbsScript);
      if (res?.ok) {
        appliedOk = true;
        if (typeof hooks.verifyScene === 'function') {
          const rep = await hooks.verifyScene();
          if (rep) {
            const lints = rep.spatialLints?.length || 0;
            verifyNote = ` · Verified in ${Math.round(rep.elapsedMs || 0)}ms (${lints} lints)`;
          }
        }
      } else if (res?.error) {
        summary = `Error: ${res.error}`;
      }
    }

    messages.push({
      role: 'assistant',
      text: `${summary}${verifyNote}`,
      code: gbsScript,
      canUndo: appliedOk || timeOfDay != null,
    });
    statusSpan.textContent = '@selection live';
    renderBody();
  }

  sendBtn.addEventListener('click', () => {
    const val = promptInput.value;
    promptInput.value = '';
    void executePrompt(val);
  });
  promptInput.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter' && !ev.shiftKey) {
      ev.preventDefault();
      const val = promptInput.value;
      promptInput.value = '';
      void executePrompt(val);
    }
  });

  function renderChatTab() {
    const sampleCard = domEl(doc, 'div', 'gb-copilot-card');
    sampleCard.append(
      domEl(doc, 'h4', '', '1-Click Prompt Starters'),
      domEl(doc, 'p', '', 'Click a sample prompt or type your own below:'),
    );
    const chips = domEl(doc, 'div', 'gb-copilot-chips');
    const samples = [
      'Ring 6 lanterns around selection radius 4m at dusk',
      'Create a path of 5 trees spacing 3m',
      'Set time to midnight and frame camera on all',
      'Place prop beside selection right',
    ];
    for (const sText of samples) {
      const chip = domEl(doc, 'button', 'gb-copilot-chip', sText);
      chip.type = 'button';
      chip.addEventListener('click', () => {
        void executePrompt(sText);
      });
      chips.append(chip);
    }
    sampleCard.append(chips);
    body.append(sampleCard);

    const logBox = domEl(doc, 'div', 'gb-copilot-log');
    for (const m of messages) {
      const bubble = domEl(doc, 'div', `gb-copilot-msg ${m.role}`);
      bubble.append(domEl(doc, 'div', '', m.text));
      if (m.code) {
        bubble.append(domEl(doc, 'pre', 'gb-copilot-code', m.code));
      }
      if (m.canUndo && typeof hooks.undo === 'function') {
        const actions = domEl(doc, 'div', 'gb-copilot-msg-actions');
        const undoBtn = domEl(doc, 'button', 'mini', '↶ Undo Transaction');
        undoBtn.type = 'button';
        undoBtn.addEventListener('click', () => { void hooks.undo(); });
        actions.append(undoBtn);
        bubble.append(actions);
      }
      logBox.append(bubble);
    }
    body.append(logBox);
  }

  function renderTemplatesTab() {
    const tplCard = domEl(doc, 'div', 'gb-copilot-card');
    tplCard.append(
      domEl(doc, 'h4', '', 'Universal Starter World Templates'),
      domEl(
        doc,
        'p',
        '',
        'Never start from an empty grid. Generates a structured, undoable 3D scene adapted to your project catalog:',
      ),
    );
    const grid = domEl(doc, 'div', 'gb-copilot-grid');
    for (const tpl of STARTER_TEMPLATES) {
      const btn = domEl(doc, 'button', 'gb-copilot-tpl');
      btn.type = 'button';
      btn.dataset.templateId = tpl.id;
      btn.append(domEl(doc, 'strong', '', tpl.title), domEl(doc, 'span', '', tpl.summary));
      btn.addEventListener('click', async () => {
        await refreshCatalogAndContext();
        await ensureEditableSceneOpen(tpl.id);
        if (tpl.time != null) hooks.setTimeOfDay?.(tpl.time);
        const script = tpl.buildGbs(catalogCache);
        const res = await hooks.applyGbs?.(script);
        messages.push({
          role: 'assistant',
          text: res?.ok ? `Built "${tpl.title}" starter layout.` : `Template note: ${res?.error || 'applied'}`,
          code: script,
          canUndo: Boolean(res?.ok),
        });
        activeTab = 'chat';
        renderBody();
      });
      grid.append(btn);
    }
    tplCard.append(grid);
    body.append(tplCard);

    // Editable Game Schemas & Workspaces card — makes 100% of any game project discoverable
    const gameCard = domEl(doc, 'div', 'gb-copilot-card');
    const schemasObj = hooks.getSchemas?.() || {};
    const schemaKeys = Array.isArray(schemasObj)
      ? schemasObj.map((s) => s.kind || s.id).filter(Boolean)
      : Object.keys(schemasObj);
    const customWbs = hooks.getProjectWorkbenches?.() || [];
    gameCard.append(
      domEl(doc, 'h4', '', `Editable Game Systems (${schemaKeys.length} Schemas)`),
      domEl(
        doc,
        'p',
        '',
        'Every game rule, table/level config, and plugin registered in project.ganbate.json is editable right inside this single application:',
      ),
    );
    const wbChips = domEl(doc, 'div', 'gb-copilot-chips');
    for (const wb of customWbs) {
      const b = domEl(doc, 'button', 'gb-copilot-chip', `Plugin: ${wb.label || wb.id}`);
      b.type = 'button';
      b.addEventListener('click', () => hooks.activateWorkbench?.(`project:${wb.id}`));
      wbChips.append(b);
    }
    const newDataBtn = domEl(doc, 'button', 'gb-copilot-chip', '+ New Schema Data Doc');
    newDataBtn.type = 'button';
    newDataBtn.addEventListener('click', () => hooks.openNewDataDialog?.());
    wbChips.append(newDataBtn);
    gameCard.append(wbChips);
    body.append(gameCard);

    // Smart Bricks quick palette
    const brickCard = domEl(doc, 'div', 'gb-copilot-card');
    brickCard.append(
      domEl(doc, 'h4', '', '1-Click Smart Bricks & Prefabs'),
      domEl(doc, 'p', '', 'Click to spawn & ground-snap at your active 3D viewport target:'),
    );
    const brickChips = domEl(doc, 'div', 'gb-copilot-chips');
    const placeable = catalogCache
      .filter((it) => ['brick', 'prefab', 'model'].includes(String(it.kind || '').toLowerCase()))
      .slice(0, 14);
    for (const item of placeable) {
      const label = item.name || item.path || item.key;
      const chip = domEl(doc, 'button', 'gb-copilot-chip', `+ ${label}`);
      chip.type = 'button';
      chip.addEventListener('click', async () => {
        await ensureEditableSceneOpen('world');
        const alias = sanitizeAlias(label, 'Brick');
        const sel = hooks.getSelection?.() || {};
        const pt = sel.cursorWorld || [0, 0, 0];
        const ref = item.key || item.brickId || item.id;
        await hooks.applyGbs?.(
          `tx "Place ${label}"\nspawn ${alias} ${ref} name="${alias}"\nplace $${alias} snap onto ground at [${pt[0]}, ${pt[2]}]`,
        );
      });
      brickChips.append(chip);
    }
    if (!placeable.length) {
      brickChips.append(domEl(doc, 'span', 'meta', 'Open a project to list Smart Bricks.'));
    }
    brickCard.append(brickChips);
    body.append(brickCard);
  }

  function renderContextTab() {
    const ctx = contextCache || buildFallbackProjectContext();
    const digestCard = domEl(doc, 'div', 'gb-copilot-card');
    const hdr = domEl(doc, 'h4');
    hdr.append(
      domEl(doc, 'span', '', 'Token-Compact AI Context Pyramid'),
      domEl(doc, 'span', 'gb-copilot-badge', `~${ctx.estimatedTokens || 0} tokens`),
    );
    const copyCtxBtn = domEl(doc, 'button', 'mini', 'Copy AI Context Digest');
    copyCtxBtn.type = 'button';
    copyCtxBtn.addEventListener('click', () => {
      globalThis.navigator?.clipboard?.writeText?.(ctx.digestText || '');
      copyCtxBtn.textContent = 'Copied!';
      setTimeout(() => { copyCtxBtn.textContent = 'Copy AI Context Digest'; }, 1200);
    });
    digestCard.append(
      hdr,
      domEl(
        doc,
        'p',
        '',
        'External AI agents read this compact L0/L1 digest (via MCP project_context or ganbate://context) instead of 200k-token raw scene files:',
      ),
      domEl(doc, 'pre', 'gb-copilot-code', ctx.digestText || ''),
      copyCtxBtn,
    );
    body.append(digestCard);

    // MCP & External LLM Connector card
    const mcpCard = domEl(doc, 'div', 'gb-copilot-card');
    const origin = globalThis.location?.origin || 'http://127.0.0.1:7480';
    const mcpSnippet = JSON.stringify(
      {
        mcpServers: {
          'ganbate-studio': {
            url: `${origin}/api/mcp`,
          },
        },
      },
      null,
      2,
    );
    mcpCard.append(
      domEl(doc, 'h4', '', 'Connect External AI Agents (MCP & LLM API)'),
      domEl(doc, 'p', '', 'Live JSON-RPC 2.0 MCP endpoint for Claude, Cursor, Gemini, or custom Python/TS agents:'),
      domEl(doc, 'pre', 'gb-copilot-code', mcpSnippet),
    );

    const epField = domEl(doc, 'label', 'gb-copilot-field', 'Optional OpenAI-Compatible / Local Ollama Chat URL');
    const epInput = domEl(doc, 'input');
    epInput.type = 'url';
    epInput.placeholder = 'http://127.0.0.1:11434/v1/chat/completions';
    epInput.value = agentCfg.endpoint || '';
    epField.append(epInput);

    const modelField = domEl(doc, 'label', 'gb-copilot-field', 'Model Name');
    const modelInput = domEl(doc, 'input');
    modelInput.type = 'text';
    modelInput.value = agentCfg.model || 'gpt-4o-mini';
    modelField.append(modelInput);

    const keyField = domEl(doc, 'label', 'gb-copilot-field', 'API Key (stored locally in browser only)');
    const keyInput = domEl(doc, 'input');
    keyInput.type = 'password';
    keyInput.placeholder = 'sk-... (optional for local Ollama)';
    keyInput.value = agentCfg.apiKey || '';
    keyField.append(keyInput);

    const saveCfgBtn = domEl(doc, 'button', 'mini on', 'Save Agent API Settings');
    saveCfgBtn.type = 'button';
    saveCfgBtn.addEventListener('click', () => {
      agentCfg.endpoint = epInput.value.trim();
      agentCfg.model = modelInput.value.trim() || 'gpt-4o-mini';
      agentCfg.apiKey = keyInput.value.trim();
      saveAgentConfig(agentCfg);
      saveCfgBtn.textContent = 'Saved ✓';
      setTimeout(() => { saveCfgBtn.textContent = 'Save Agent API Settings'; }, 1200);
    });

    mcpCard.append(epField, modelField, keyField, saveCfgBtn);
    body.append(mcpCard);
  }

  function renderBody() {
    for (const [id, btn] of tabButtons) {
      btn.classList.toggle('on', id === activeTab);
    }
    compose.hidden = activeTab !== 'chat';
    body.replaceChildren();
    if (activeTab === 'chat') renderChatTab();
    else if (activeTab === 'templates') renderTemplatesTab();
    else renderContextTab();
  }

  const api = {
    element: root,
    isOpen: () => !root.hidden,
    open: (tab = null) => {
      if (tab) activeTab = tab;
      root.hidden = false;
      hooks.onToggle?.(true);
      renderBody();
      void refreshCatalogAndContext().then(() => renderBody());
    },
    close: () => {
      root.hidden = true;
      hooks.onToggle?.(false);
    },
    toggle: (tab = null) => {
      if (root.hidden) api.open(tab);
      else api.close();
    },
    executePrompt,
    destroy: () => root.remove(),
  };

  renderBody();
  return api;
}
