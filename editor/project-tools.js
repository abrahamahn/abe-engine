// Project-defined tools run at the game's origin. They never receive the Studio pairing token.
function webOrigin(value) {
  const url = new URL(value);
  const local = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
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
 * Resolves the docked 2.5D (`layout-tuner`) and 3D (`graphics-layout-tuner`) Table Layout tools
 * from resolved project tools, deriving the companion `/ganbate-editor/*` route on the same
 * validated game origin when only one of the pair is listed in `project-tools.json`.
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
      || pathname === '/ganbate-editor/layout-tuner'
      || pathname.endsWith('/layout-tuner');
  }) || null;

  let graphicsLayoutTuner = tools.find((tool) => {
    const pathname = pathnameOf(tool);
    return tool.id === 'graphics-layout-tuner'
      || pathname === '/ganbate-editor/graphics-layout-tuner'
      || pathname.endsWith('/graphics-layout-tuner');
  }) || null;

  const editorAnchor = layoutTuner
    || graphicsLayoutTuner
    || tools.find((tool) => pathnameOf(tool).startsWith('/ganbate-editor'))
    || null;

  if (editorAnchor) {
    try {
      const origin = new URL(editorAnchor.href).origin;
      if (!layoutTuner) {
        const href = new URL('/ganbate-editor/layout-tuner', origin).href;
        layoutTuner = {
          id: 'layout-tuner',
          label: '2.5D Table Layout',
          href,
          embedHref: withEmbedStudioParam(href),
          open: 'embed',
        };
      }
      if (!graphicsLayoutTuner) {
        const href = new URL('/ganbate-editor/graphics-layout-tuner', origin).href;
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
  const configured = webOrigin(rawOrigin);
  const explicitOverride = Boolean(override);
  const candidate = explicitOverride && configured.local ? webOrigin(override) : configured;
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
