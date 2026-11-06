// Project-defined tools run at the game's origin. They never receive the Studio pairing token.
function isLocalHttpHostname(hostname) {
  const h = String(hostname || '').toLowerCase().replace(/\.$/, '');
  if (h === 'localhost' || h === '127.0.0.1' || h === '[::1]' || h === '0.0.0.0') return true;
  if (h.endsWith('.localhost') || h.endsWith('.local') || h.endsWith('.internal') || h.endsWith('.ts.net')) return true;
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(h);
  if (m) {
    const a = Number(m[1]);
    const b = Number(m[2]);
    if (a === 10 || a === 127) return true;
    if (a === 192 && b === 168) return true;
    if (a === 172 && b >= 16 && b <= 31) return true;
    if (a === 100 && b >= 64 && b <= 127) return true;
  }
  return false;
}

function webOrigin(value, { allowSameOrigin = false, studioOrigin = null } = {}) {
  const url = new URL(value);
  const sameAsStudio = Boolean(
    allowSameOrigin && studioOrigin && url.origin === new URL('/', studioOrigin).origin,
  );
  const local = isLocalHttpHostname(url.hostname) || (sameAsStudio && url.protocol === 'http:');
  if (url.username || url.password || url.search || url.hash || url.pathname !== '/' ||
      !(url.protocol === 'https:' || (url.protocol === 'http:' && local))) {
    throw new Error('Project tools require an HTTPS origin or a local HTTP server.');
  }
  return { origin: url.origin, local };
}

/**
 * Normalizes loopback origins (`127.0.0.1` alongside `localhost` / `[::1]`) when resolving
 * manifest-configured project tools without an explicit override:
 * - Prefers IPv4 `127.0.0.1` whenever Studio itself is on `localhost` or `[::1]` so `http://localhost:5173`
 *   in `project-tools.json` reaches IPv4-bound dev servers on IPv6-first Linux hosts while maintaining
 *   strict cross-hostname cookie isolation (`localhost` vs `127.0.0.1`).
 * - Conversely swaps `127.0.0.1` / `[::1]` to `localhost` when Studio itself is opened on `127.0.0.1`.
 */
export function normalizeLoopbackOrigin(configured, studioOrigin, { allowSameOrigin = false, explicitOverride = false } = {}) {
  if (!configured?.local || allowSameOrigin || explicitOverride) return configured;
  const studioUrl = new URL(studioOrigin);
  const toolUrl = new URL(configured.origin);
  const studioHost = studioUrl.hostname.replace(/\.$/, '');
  const toolHost = toolUrl.hostname.replace(/\.$/, '');
  if (studioHost !== '127.0.0.1' && (toolHost === 'localhost' || toolHost === '[::1]')) {
    toolUrl.hostname = '127.0.0.1';
    return { origin: toolUrl.origin, local: true };
  }
  if (studioHost === '127.0.0.1' && (toolHost === '127.0.0.1' || toolHost === '[::1]')) {
    toolUrl.hostname = 'localhost';
    return { origin: toolUrl.origin, local: true };
  }
  return configured;
}

/**
 * Normalizes a `?toolOrigin=` query override when both Studio and the tool run on local loopback
 * on different ports, swapping `127.0.0.1` <-> `localhost` if needed to preserve Studio's
 * cross-hostname cookie isolation without rejecting same-port host self-embedding.
 */
export function normalizeLoopbackQueryOverride(override, studioOrigin, { allowSameOrigin = false } = {}) {
  if (!override || allowSameOrigin) return override;
  try {
    const parsed = webOrigin(override);
    if (!parsed.local) return override;
    const studioUrl = new URL(studioOrigin);
    const toolUrl = new URL(parsed.origin);
    // Never rewrite host self-embedding on the exact same port.
    if (toolUrl.origin === studioUrl.origin || (toolUrl.port || '80') === (studioUrl.port || '80')) {
      return override;
    }
    const studioHost = studioUrl.hostname.replace(/\.$/, '');
    const toolHost = toolUrl.hostname.replace(/\.$/, '');
    if (studioHost === toolHost) {
      toolUrl.hostname = studioHost === '127.0.0.1' ? 'localhost' : '127.0.0.1';
      return toolUrl.origin;
    }
    if (studioHost !== '127.0.0.1' && toolHost === '[::1]') {
      toolUrl.hostname = '127.0.0.1';
      return toolUrl.origin;
    }
  } catch {
    return override;
  }
  return override;
}

/** Appends `?embed=studio` to a docked project tool URL so it renders chrome-less inside Studio. */
export function withEmbedStudioParam(href) {
  const url = new URL(href);
  if (!url.searchParams.has('embed')) {
    url.searchParams.set('embed', 'studio');
  }
  return url.href;
}

/**
 * Builtin unified `/studio` gameplay tool routes (`/__studio-tool/<id>`).
 * Explicit `cloudToolsManifest` / `project-tools.json` paths always take precedence when provided.
 */
export const BUILTIN_GAMEPLAY_TOOLS = Object.freeze([
  Object.freeze({
    id: 'motion-tuner',
    label: '🃏 Card & Chip Motion',
    path: '/__studio-tool/motion-tuner',
    open: 'embed',
  }),
  Object.freeze({
    id: 'action-sequencer',
    label: '🎬 Hand Choreography',
    path: '/__studio-tool/action-sequencer',
    open: 'embed',
  }),
  Object.freeze({
    id: 'god-mode',
    label: '🧪 Crowded-Hand Lab',
    path: '/__studio-tool/god-mode',
    open: 'embed',
  }),
  Object.freeze({
    id: 'audio-mixer',
    label: '🎚️ Game Foley Mixer',
    path: '/__studio-tool/audio-mixer',
    open: 'embed',
  }),
]);

/**
 * Resolves the docked 2.5D (`layout-tuner`) and 3D (`graphics-layout-tuner`) Table Layout tools
 * from resolved project tools, deriving the companion `/__studio-tool/*` or `/ganbate-editor/*`
 * route on the same validated game origin when only one of the pair is listed in the manifest.
 */
export function resolveTableLayoutTools(tools = []) {
  if (!Array.isArray(tools) || !tools.length) {
    return { layoutTuner: null, graphicsLayoutTuner: null };
  }
  const pathnameOf = (tool) => {
    try {
      return new URL(tool.href).pathname.replace(/\/+$/, '');
    } catch {
      return '';
    }
  };
  let layoutTuner = tools.find((tool) => {
    const pathname = pathnameOf(tool);
    return tool.id === 'layout-tuner'
      || pathname === '/__studio-tool/layout-tuner'
      || pathname === '/ganbate-editor/layout-tuner'
      || pathname.endsWith('/layout-tuner');
  }) || null;

  let graphicsLayoutTuner = tools.find((tool) => {
    const pathname = pathnameOf(tool);
    return tool.id === 'graphics-layout-tuner'
      || pathname === '/__studio-tool/graphics-layout-tuner'
      || pathname === '/ganbate-editor/graphics-layout-tuner'
      || pathname.endsWith('/graphics-layout-tuner');
  }) || null;

  const editorAnchor = layoutTuner
    || graphicsLayoutTuner
    || tools.find((tool) => {
      const p = pathnameOf(tool);
      return p.startsWith('/__studio-tool') || p.startsWith('/ganbate-editor');
    })
    || null;

  if (editorAnchor) {
    try {
      const anchorUrl = new URL(editorAnchor.href);
      const origin = anchorUrl.origin;
      const prefix = anchorUrl.pathname.startsWith('/ganbate-editor') ? '/ganbate-editor' : '/__studio-tool';
      if (!layoutTuner) {
        const href = new URL(`${prefix}/layout-tuner`, origin).href;
        layoutTuner = {
          id: 'layout-tuner',
          label: '2.5D Table Layout',
          href,
          embedHref: withEmbedStudioParam(href),
          open: 'embed',
        };
      }
      if (!graphicsLayoutTuner) {
        const href = new URL(`${prefix}/graphics-layout-tuner`, origin).href;
        graphicsLayoutTuner = {
          id: 'graphics-layout-tuner',
          label: '3D Table Layout',
          href,
          embedHref: withEmbedStudioParam(href),
          open: 'embed',
        };
      }
    } catch {
      // Keep explicit matches only if URL parsing fails.
    }
  }

  return { layoutTuner, graphicsLayoutTuner };
}

/**
 * Resolves the companion gameplay editors (`motion-tuner`, `action-sequencer`, `god-mode`,
 * and `audio-mixer`) honoring explicit `cloudToolsManifest` / `project-tools.json` paths when
 * provided and falling back to `BUILTIN_GAMEPLAY_TOOLS` (`/__studio-tool/<id>`, `open: 'embed'`).
 */
export function resolveGameplayTools(tools = [], studioOrigin = null, options = {}) {
  const safeTools = Array.isArray(tools) ? tools : [];
  if (!safeTools.length && !(studioOrigin && options?.allowSameOrigin)) {
    return {
      motionTuner: null,
      actionSequencer: null,
      godMode: null,
      audioMixer: null,
    };
  }
  const pathnameOf = (tool) => {
    try {
      return new URL(tool.href).pathname.replace(/\/+$/, '');
    } catch {
      return '';
    }
  };
  const normalizeToolHrefOrigin = (rawOrigin) => {
    if (!studioOrigin) return rawOrigin;
    try {
      const parsed = webOrigin(rawOrigin);
      return normalizeLoopbackOrigin(parsed, studioOrigin, {
        allowSameOrigin: Boolean(options?.allowSameOrigin),
        explicitOverride: Boolean(options?.explicitOverride),
      }).origin;
    } catch {
      return rawOrigin;
    }
  };
  const ensureDockedEntry = (matched) => {
    if (!matched) return null;
    try {
      const parsed = new URL(matched.href);
      const origin = normalizeToolHrefOrigin(parsed.origin);
      const rebuiltHref = new URL(`${parsed.pathname}${parsed.search}${parsed.hash}`, origin).href;
      return {
        ...matched,
        href: rebuiltHref,
        embedHref: withEmbedStudioParam(rebuiltHref),
        open: matched.open === 'panel' || matched.open === 'embed' ? matched.open : 'embed',
      };
    } catch {
      return matched;
    }
  };
  const matchExplicitTool = (id) => ensureDockedEntry(safeTools.find((tool) => {
    const pathname = pathnameOf(tool);
    return tool.id === id
      || pathname === `/__studio-tool/${id}`
      || pathname === `/ganbate-editor/${id}`
      || pathname.endsWith(`/${id}`);
  }) || null);

  let motionTuner = matchExplicitTool('motion-tuner');
  let actionSequencer = matchExplicitTool('action-sequencer');
  let godMode = matchExplicitTool('god-mode');
  let audioMixer = matchExplicitTool('audio-mixer');

  const editorAnchor = motionTuner
    || actionSequencer
    || godMode
    || audioMixer
    || safeTools.find((tool) => {
      const pathname = pathnameOf(tool);
      return pathname.startsWith('/__studio-tool') || pathname.startsWith('/ganbate-editor');
    })
    || null;

  const fallbackOrigin = editorAnchor
    ? (() => {
        try {
          return normalizeToolHrefOrigin(new URL(editorAnchor.href).origin);
        } catch {
          return null;
        }
      })()
    : (studioOrigin && options?.allowSameOrigin
        ? (() => {
            try {
              return new URL('/', studioOrigin).origin;
            } catch {
              return null;
            }
          })()
        : null);

  if (fallbackOrigin) {
    const buildBuiltinFallback = (id) => {
      const builtin = BUILTIN_GAMEPLAY_TOOLS.find((item) => item.id === id);
      if (!builtin) return null;
      const href = new URL(builtin.path, fallbackOrigin).href;
      return {
        id: builtin.id,
        label: builtin.label,
        href,
        embedHref: withEmbedStudioParam(href),
        open: builtin.open,
      };
    };
    try {
      if (!motionTuner) motionTuner = buildBuiltinFallback('motion-tuner');
      if (!actionSequencer) actionSequencer = buildBuiltinFallback('action-sequencer');
      if (!godMode) godMode = buildBuiltinFallback('god-mode');
      if (!audioMixer) audioMixer = buildBuiltinFallback('audio-mixer');
    } catch {
      // Keep explicit matches only if URL parsing fails.
    }
  }

  return {
    motionTuner,
    actionSequencer,
    godMode,
    audioMixer,
  };
}

/**
 * Renders the top-right 1-click Gameplay Editor pill switchers inside `#workbench-deck`:
 * - `[data-role="gameplay-sequencer-switcher"]` for `sequencer`, `action-sequencer`, `motion-tuner`, and `god-mode`.
 * - `[data-role="gameplay-audio-switcher"]` for `audio` and `audio-mixer`.
 */
export function createGameplaySwitcher({ el, t, getContext, onSelectTarget }) {
  const ensureHost = () => {
    let host = document.getElementById('gameplay-workbench-switcher');
    if (!host) {
      const deck = document.getElementById('workbench-deck');
      if (!deck) return null;
      host = el('div', {
        id: 'gameplay-workbench-switcher',
        class: 'gameplay-workbench-switcher',
        hidden: true,
      });
      deck.append(host);
    }
    return host;
  };

  function classifyActiveTarget(activeWorkbench, gameplayTools) {
    if (activeWorkbench === 'sequencer') {
      return { family: 'sequencer', activeKey: 'sequencer' };
    }
    if (
      activeWorkbench === 'project:action-sequencer'
      || (gameplayTools?.actionSequencer && activeWorkbench === `project:${gameplayTools.actionSequencer.id}`)
    ) {
      return { family: 'sequencer', activeKey: 'action-sequencer' };
    }
    if (
      activeWorkbench === 'project:motion-tuner'
      || (gameplayTools?.motionTuner && activeWorkbench === `project:${gameplayTools.motionTuner.id}`)
    ) {
      return { family: 'sequencer', activeKey: 'motion-tuner' };
    }
    if (
      activeWorkbench === 'project:god-mode'
      || (gameplayTools?.godMode && activeWorkbench === `project:${gameplayTools.godMode.id}`)
    ) {
      return { family: 'sequencer', activeKey: 'god-mode' };
    }
    if (activeWorkbench === 'audio') {
      return { family: 'audio', activeKey: 'audio' };
    }
    if (
      activeWorkbench === 'project:audio-mixer'
      || (gameplayTools?.audioMixer && activeWorkbench === `project:${gameplayTools.audioMixer.id}`)
    ) {
      return { family: 'audio', activeKey: 'audio-mixer' };
    }
    return { family: null, activeKey: null };
  }

  function makePill(targetKey, labelKey, tipKey, isActive) {
    return el('button', {
      type: 'button',
      class: `mini layout-rig-pill gameplay-switcher-pill${isActive ? ' on' : ''}`,
      'aria-pressed': String(isActive),
      'data-gameplay-tool': targetKey,
      title: t(tipKey),
      onclick: (ev) => {
        ev?.stopPropagation?.();
        onSelectTarget?.(targetKey);
      },
    }, t(labelKey));
  }

  function draw() {
    const host = ensureHost();
    if (!host) return;
    const { activeWorkbench, gameplayTools } = getContext();
    const { family, activeKey } = classifyActiveTarget(activeWorkbench, gameplayTools);

    if (family === 'sequencer') {
      host.hidden = false;
      host.replaceChildren(
        el('div', { class: 'gameplay-switcher-bar', 'data-role': 'gameplay-sequencer-switcher' },
          makePill('action-sequencer', 'gpHandChoreo', 'gpHandChoreoTip', activeKey === 'action-sequencer'),
          makePill('motion-tuner', 'gpCardChipMotion', 'gpCardChipMotionTip', activeKey === 'motion-tuner'),
          makePill('god-mode', 'gpCrowdedHandLab', 'gpCrowdedHandLabTip', activeKey === 'god-mode'),
          makePill('sequencer', 'gpEngineSequencer', 'gpEngineSequencerTip', activeKey === 'sequencer'),
        ),
      );
      return;
    }

    if (family === 'audio') {
      host.hidden = false;
      host.replaceChildren(
        el('div', { class: 'gameplay-switcher-bar', 'data-role': 'gameplay-audio-switcher' },
          makePill('audio-mixer', 'gpGameFoleyMixer', 'gpGameFoleyMixerTip', activeKey === 'audio-mixer'),
          makePill('audio', 'gpEngineAudioDaw', 'gpEngineAudioDawTip', activeKey === 'audio'),
        ),
      );
      return;
    }

    host.hidden = true;
    host.replaceChildren();
  }

  return { draw };
}

function slugifyToolId(tool, idx) {
  if (typeof tool.id === 'string' && tool.id.trim()) {
    return tool.id.trim().replace(/[^a-zA-Z0-9_-]/g, '-');
  }
  const base = tool.label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return base || `tool-${idx + 1}`;
}

export function resolveProjectTools(manifest, studioOrigin, override, options = {}) {
  if (!manifest || manifest.version !== 1 || !Array.isArray(manifest.tools)) {
    throw new Error('Invalid project tools manifest.');
  }
  const allowSameOrigin = options?.allowSameOrigin === true;
  const rawOrigin = manifest.origin === 'same-origin' && allowSameOrigin
    ? new URL('/', studioOrigin).origin
    : manifest.origin;
  const configured = webOrigin(rawOrigin, { allowSameOrigin, studioOrigin });
  const explicitOverride = Boolean(override);
  const candidate = explicitOverride && configured.local ? webOrigin(override, { allowSameOrigin, studioOrigin }) : configured;
  if (explicitOverride && configured.local && !candidate.local) throw new Error('A local tool origin must stay local.');
  const requested = normalizeLoopbackOrigin(candidate, studioOrigin, { allowSameOrigin, explicitOverride });
  // Cookies are host-scoped, not port-scoped. A paired Studio's HttpOnly cookie belongs only to Studio.
  if (
    !allowSameOrigin &&
    new URL(requested.origin).hostname.replace(/\.$/, '') === new URL(studioOrigin).hostname.replace(/\.$/, '')
  ) {
    throw new Error('Use different hostnames for Studio and the game, e.g. localhost and 127.0.0.1.');
  }
  return manifest.tools.map((tool, idx) => {
    if (typeof tool.label !== 'string' || !tool.label.trim() || typeof tool.path !== 'string' ||
        !tool.path.startsWith('/') || tool.path.startsWith('//') || tool.path.includes('\\')) {
      throw new Error('Invalid project tool label or path.');
    }
    const url = new URL(tool.path, requested.origin);
    if (url.origin !== requested.origin) throw new Error('Project tool leaves its configured origin.');
    if (tool.open !== undefined && tool.open !== 'panel' && tool.open !== 'embed' && tool.open !== 'tab') {
      throw new Error('Invalid project tool target.');
    }
    const open = tool.open ?? 'panel';
    return {
      id: slugifyToolId(tool, idx),
      label: tool.label,
      href: url.href,
      embedHref: open === 'panel' || open === 'embed' ? withEmbedStudioParam(url.href) : url.href,
      open,
    };
  });
}

export function projectToolsPanel(onError, options = {}) {
  const select = document.getElementById('project-tools');
  const tabs = document.getElementById('project-tab-tools');
  const dialog = document.getElementById('project-tool-panel');
  const frame = document.getElementById('project-tool-frame');
  const title = document.getElementById('project-tool-title');
  const external = document.getElementById('project-tool-external');
  let projectLoad = 0;
  let currentTools = [];

  select?.addEventListener('change', () => {
    if (!select.value) return;
    const chosenHref = select.value;
    const matching = currentTools.find((t) => t.href === chosenHref || t.embedHref === chosenHref);
    const option = [...select.options].find((o) => o.value === chosenHref);
    select.selectedIndex = 0;
    if (title && frame && external && dialog) {
      title.textContent = frame.title = option?.textContent || matching?.label || 'Tool';
      external.href = matching?.href || chosenHref;
      frame.src = matching?.href || chosenHref;
      dialog.showModal();
    }
  });
  document.getElementById('project-tool-close')?.addEventListener('click', () => dialog?.close());
  dialog?.addEventListener('close', () => {
    if (!dialog.open) frame?.removeAttribute('src');
  });

  return async (project, advertisedCatalogs, directManifest = null) => {
    const current = ++projectLoad;
    try {
      const rawOverride = new URLSearchParams(location.search).get('toolOrigin');
      const allowSameOrigin = Boolean(options.allowSameOrigin?.());
      const override = normalizeLoopbackQueryOverride(rawOverride, location.origin, { allowSameOrigin });
      let resolvedTools = [];
      if (directManifest && typeof directManifest === 'object') {
        resolvedTools = resolveProjectTools(directManifest, location.origin, override, { allowSameOrigin });
      } else {
        // Older Studio hosts can opt in from a launcher that explicitly supplies a tool origin.
        const catalogs = advertisedCatalogs ?? (override ? (project.editor ?? []).map(dir => dir + '/project-tools.json') : []);
        const groups = await Promise.all(catalogs.map(async path => {
          const url = '/project/' + path.split('/').map(encodeURIComponent).join('/');
          const response = await fetch(url);
          if (response.status === 404) return [];
          if (!response.ok) throw new Error('Could not load ' + url);
          return resolveProjectTools(await response.json(), location.origin, override, { allowSameOrigin });
        }));
        resolvedTools = groups.flat();
      }
      if (current !== projectLoad) return [];
      currentTools = resolvedTools;
      if (select) select.length = 1;
      tabs?.replaceChildren();
      for (const tool of resolvedTools) {
        if (tool.open === 'panel' || tool.open === 'embed') {
          select?.add(new Option(tool.label, tool.href));
          continue;
        }
        // Use a real top-level navigation for tools that require the game's own session.
        const link = document.createElement('a');
        link.href = tool.href;
        link.target = '_blank';
        link.rel = 'noopener noreferrer';
        link.textContent = tool.label;
        tabs?.append(link);
      }
      if (select) select.hidden = select.length === 1;
      options.onToolsLoaded?.(resolvedTools);
      return resolvedTools;
    } catch (error) {
      if (current === projectLoad) {
        currentTools = [];
        if (select) select.hidden = true;
        tabs?.replaceChildren();
        options.onToolsLoaded?.([]);
        onError(String(error));
      }
      return [];
    }
  };
}
