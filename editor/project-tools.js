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

export function resolveProjectTools(manifest, studioOrigin, override) {
  if (manifest.version !== 1 || !Array.isArray(manifest.tools)) throw new Error('Invalid project tools manifest.');
  const configured = webOrigin(manifest.origin);
  const requested = override && configured.local ? webOrigin(override) : configured;
  if (override && configured.local && !requested.local) throw new Error('A local tool origin must stay local.');
  // Cookies are host-scoped, not port-scoped. A paired Studio's HttpOnly cookie belongs only to Studio.
  if (new URL(requested.origin).hostname.replace(/\.$/, '') === new URL(studioOrigin).hostname.replace(/\.$/, '')) {
    throw new Error('Use different hostnames for Studio and the game, e.g. localhost and 127.0.0.1.');
  }
  return manifest.tools.map(tool => {
    if (typeof tool.label !== 'string' || !tool.label.trim() || typeof tool.path !== 'string' ||
        !tool.path.startsWith('/') || tool.path.startsWith('//') || tool.path.includes('\\')) {
      throw new Error('Invalid project tool label or path.');
    }
    const url = new URL(tool.path, requested.origin);
    if (url.origin !== requested.origin) throw new Error('Project tool leaves its configured origin.');
    return { label: tool.label, href: url.href };
  });
}

export function projectToolsPanel(onError) {
  const select = document.getElementById('project-tools');
  const dialog = document.getElementById('project-tool-panel');
  const frame = document.getElementById('project-tool-frame');
  const title = document.getElementById('project-tool-title');
  const external = document.getElementById('project-tool-external');
  let projectLoad = 0;
  select.addEventListener('change', () => {
    if (!select.value) return;
    const option = select.selectedOptions[0];
    title.textContent = frame.title = option.textContent;
    external.href = select.value;
    frame.src = select.value;
    dialog.showModal();
    select.selectedIndex = 0;
  });
  document.getElementById('project-tool-close').addEventListener('click', () => dialog.close());
  dialog.addEventListener('close', () => {
    if (!dialog.open) frame.removeAttribute('src');
  });
  return async (project, advertisedCatalogs) => {
    const current = ++projectLoad;
    try {
      // Older Studio hosts can opt in from a launcher that explicitly supplies a tool origin.
      const override = new URLSearchParams(location.search).get('toolOrigin');
      const catalogs = advertisedCatalogs ?? (override ? (project.editor ?? []).map(dir => dir + '/project-tools.json') : []);
      const groups = await Promise.all(catalogs.map(async path => {
        const url = '/project/' + path.split('/').map(encodeURIComponent).join('/');
        const response = await fetch(url);
        if (response.status === 404) return [];
        if (!response.ok) throw new Error('Could not load ' + url);
        return resolveProjectTools(await response.json(), location.origin, override);
      }));
      if (current !== projectLoad) return;
      select.length = 1;
      for (const tool of groups.flat()) select.add(new Option(tool.label, tool.href));
      select.hidden = select.length === 1;
    } catch (error) {
      if (current === projectLoad) { select.hidden = true; onError(String(error)); }
    }
  };
}
