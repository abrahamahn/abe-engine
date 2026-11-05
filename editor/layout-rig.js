// Interactive Responsive Anchor Layout Rig overlay (`#layout-rig-overlay`) for `Kind::Data`.
// Aligns over either the 3D `#viewport` or the active single PIE device frame, streaming merged
// normalized `vec2` ops to the Studio host and PIE runtime.
import * as M from './math.js';
import { buildDataValueOps, materializeDataValues, schemaHasLayoutAnchors } from './data-inspector.js';

function isNormalizedVec2(field, path, forceIncludeAllVec2) {
  if (!field || field.type !== 'vec2') return false;
  if (forceIncludeAllVec2) return true;
  if (field.unit === 'norm') return true;
  if (Number.isFinite(field.min) && Number.isFinite(field.max) && field.min >= -0.5 && field.max <= 1.5) {
    return true;
  }
  return path.some((seg) => seg === 'anchors' || seg === 'slots' || seg === 'layout');
}

/** Discovers draggable `vec2` anchors from `doc.root.values` + `schemaDef`. */
export function discoverLayoutAnchors(values, schemaDef, { includeAllVec2 = false } = {}) {
  if (!schemaDef?.fields) return [];
  const effective = materializeDataValues(values, schemaDef);
  const anchors = [];
  const force = includeAllVec2 || schemaDef.editor === 'layout-rig';

  const walk = (curVal, fields, path, parentTitle = '') => {
    for (const [key, field] of Object.entries(fields || {})) {
      if (!field || field.hidden || field.readonly) continue;
      const nextPath = [...path, key];
      const val = curVal?.[key];
      if (field.type === 'vec2' && isNormalizedVec2(field, nextPath, force)) {
        const pair = Array.isArray(val) && val.length >= 2 ? val : [0.5, 0.5];
        const title = parentTitle
          ? (key === 'pos' || key === 'anchor' || key === 'position' ? parentTitle : `${parentTitle}.${field.label || key}`)
          : (field.label || key);
        anchors.push({
          path: nextPath,
          pathKey: nextPath.join('.'),
          label: title,
          x: Number(pair[0]) || 0,
          y: Number(pair[1]) || 0,
          min: Number.isFinite(field.min) ? field.min : 0,
          max: Number.isFinite(field.max) ? field.max : 1,
        });
      } else if (field.type === 'object' && field.fields) {
        const objTitle = val && typeof val === 'object' && (val.name || val.label || val.id)
          ? String(val.name || val.label || val.id)
          : (field.label || key);
        walk(val || {}, field.fields, nextPath, parentTitle ? `${parentTitle}.${objTitle}` : objTitle);
      } else if (field.type === 'array' && field.items && Array.isArray(val)) {
        val.forEach((item, idx) => {
          const itemPath = [...nextPath, String(idx)];
          if (field.items.type === 'vec2' && isNormalizedVec2(field.items, itemPath, force)) {
            const pair = Array.isArray(item) && item.length >= 2 ? item : [0.5, 0.5];
            anchors.push({
              path: itemPath,
              pathKey: itemPath.join('.'),
              label: `${field.label || key}[${idx}]`,
              x: Number(pair[0]) || 0,
              y: Number(pair[1]) || 0,
              min: Number.isFinite(field.items.min) ? field.items.min : 0,
              max: Number.isFinite(field.items.max) ? field.items.max : 1,
            });
          } else if (field.items.type === 'object' && field.items.fields) {
            const itemTitle = item && typeof item === 'object' && (item.name || item.id || item.label)
              ? String(item.name || item.id || item.label)
              : `${field.label || key}[${idx}]`;
            walk(item || {}, field.items.fields, itemPath, itemTitle);
          }
        });
      }
    }
  };

  walk(effective, schemaDef.fields, []);
  return anchors;
}

function swapLeftRightToken(text) {
  if (/left/i.test(text)) {
    return text.replace(/left/gi, (m) => (m[0] === 'L' ? 'Right' : 'right'));
  }
  if (/right/i.test(text)) {
    return text.replace(/right/gi, (m) => (m[0] === 'R' ? 'Left' : 'left'));
  }
  return null;
}

function findMirrorAnchor(anchor, allAnchors) {
  const swappedPath = swapLeftRightToken(anchor.pathKey);
  if (swappedPath) {
    const byPath = allAnchors.find((a) => a.pathKey !== anchor.pathKey && a.pathKey === swappedPath);
    if (byPath) return byPath;
  }
  const swappedLabel = swapLeftRightToken(anchor.label);
  if (swappedLabel) {
    const byLabel = allAnchors.find((a) => a.pathKey !== anchor.pathKey && a.label === swappedLabel);
    if (byLabel) return byLabel;
  }
  return null;
}

export function createLayoutRig({
  el,
  t,
  getContext,
  getTargetElement,
  onEditData,
  onFocusField,
  onOpenDockedTool,
  onActivateAnchorRig,
}) {
  const manualOverrides = new Map(); // docId -> boolean
  let mirrorX = false;
  let selectedPathKey = null;
  let dragState = null;
  let dragSeq = 0;
  let resizeObs = null;

  const overlayEl = () => document.getElementById('layout-rig-overlay');
  const previewFrameEl = () => document.getElementById('preview-frame');

  function isActiveFor(doc, schema) {
    if (!doc || doc.error || !doc.root || !schema) return false;
    if (!schemaHasLayoutAnchors(schema)) return false;
    if (manualOverrides.has(doc.id)) return manualOverrides.get(doc.id);
    return schema.editor === 'layout-rig';
  }

  function toggleForCurrentDoc() {
    const { inspectedDataDoc, kinds } = getContext();
    if (!inspectedDataDoc?.root) return false;
    const schema = kinds?.[inspectedDataDoc.root.schema];
    const next = !isActiveFor(inspectedDataDoc, schema);
    manualOverrides.set(inspectedDataDoc.id, next);
    draw();
    return next;
  }

  function syncOverlayBounds() {
    const overlay = overlayEl();
    const frame = previewFrameEl();
    const target = getTargetElement?.();
    if (!overlay || overlay.hidden || !frame || !target || target.hidden) return false;
    const fRect = frame.getBoundingClientRect();
    const tRect = target.getBoundingClientRect();
    if (tRect.width <= 0 || tRect.height <= 0) return false;
    overlay.style.left = `${tRect.left - fRect.left}px`;
    overlay.style.top = `${tRect.top - fRect.top}px`;
    overlay.style.width = `${tRect.width}px`;
    overlay.style.height = `${tRect.height}px`;
    return true;
  }

  function buildBatchOps(doc, schema, updates) {
    const working = structuredClone(doc.root?.values || {});
    const ops = [];
    for (const u of updates) {
      const built = buildDataValueOps(working, schema, u.path, u.value, false);
      for (const b of built) {
        ops.push(b);
        if (b.set && b.set[0] === 'values') {
          let cur = working;
          for (let i = 1; i < b.set.length - 1; i++) {
            const k = b.set[i];
            if (!cur[k] || typeof cur[k] !== 'object' || Array.isArray(cur[k])) cur[k] = {};
            cur = cur[k];
          }
          cur[b.set[b.set.length - 1]] = structuredClone(b.value);
        }
      }
    }
    return ops;
  }

  function buildSwitcherPills() {
    return el('span', { class: 'layout-rig-pills', 'data-role': 'layout-rig-switcher' },
      el('button', {
        type: 'button',
        id: 'layout-rig-open-25d',
        class: 'mini layout-rig-pill',
        'data-layout-tool': 'layout-tuner',
        title: t('layoutRig25dTableTip'),
        onclick: (ev) => {
          ev.stopPropagation();
          onOpenDockedTool?.('layout-tuner');
        },
      }, t('layoutRig25dTable')),
      el('button', {
        type: 'button',
        id: 'layout-rig-open-3d',
        class: 'mini layout-rig-pill',
        'data-layout-tool': 'graphics-layout-tuner',
        title: t('layoutRig3dTableTip'),
        onclick: (ev) => {
          ev.stopPropagation();
          onOpenDockedTool?.('graphics-layout-tuner');
        },
      }, t('layoutRig3dTable')),
      el('button', {
        type: 'button',
        id: 'layout-rig-anchor-mode',
        class: 'mini layout-rig-pill on',
        'aria-pressed': 'true',
        'data-layout-tool': 'anchor-rig',
        title: t('layoutRigAnchorModeTip'),
        onclick: (ev) => {
          ev.stopPropagation();
          onActivateAnchorRig?.();
        },
      }, t('layoutRigAnchorMode')),
    );
  }

  function draw() {
    const overlay = overlayEl();
    if (!overlay) return;
    if (dragState) {
      syncOverlayBounds();
      return;
    }

    const { inspectedDataDoc, kinds, pieMode, activeWorkbench, tableLayoutTools } = getContext();
    const schema = inspectedDataDoc?.root ? kinds?.[inspectedDataDoc.root.schema] : null;
    const hasAnchorDoc = isActiveFor(inspectedDataDoc, schema);
    const inLayoutWorkbench = activeWorkbench === 'layout-rig';
    const hasTableTools = Boolean(tableLayoutTools?.layoutTuner || tableLayoutTools?.graphicsLayoutTuner);

    if (pieMode === 'matrix' || (!hasAnchorDoc && !inLayoutWorkbench)) {
      overlay.hidden = true;
      overlay.replaceChildren();
      return;
    }

    const anchors = hasAnchorDoc
      ? discoverLayoutAnchors(inspectedDataDoc.root.values, schema, { includeAllVec2: true })
      : [];
    overlay.hidden = false;
    if (!syncOverlayBounds()) {
      overlay.style.inset = '0';
      overlay.style.width = '100%';
      overlay.style.height = '100%';
    }

    const safeBox = el('div', { class: 'layout-rig-safe', 'aria-hidden': 'true' });
    const guideX = el('div', { class: 'layout-rig-guide-x', 'aria-hidden': 'true' });
    const guideY = el('div', { class: 'layout-rig-guide-y', 'aria-hidden': 'true' });

    const showSwitcher = inLayoutWorkbench && (hasTableTools || !hasAnchorDoc);
    const barChildren = [
      el('span', { class: 'layout-rig-title' }, hasAnchorDoc ? `${t('layoutRig')} · ${anchors.length}` : t('layoutRig')),
    ];
    if (showSwitcher) {
      barChildren.push(buildSwitcherPills());
    }
    if (hasAnchorDoc) {
      barChildren.push(
        el('button', {
          type: 'button',
          id: 'layout-rig-mirror',
          class: `mini${mirrorX ? ' on' : ''}`,
          'aria-pressed': String(mirrorX),
          onclick: (ev) => {
            ev.stopPropagation();
            mirrorX = !mirrorX;
            draw();
          },
        }, t('mirrorX')),
        el('span', { class: 'layout-rig-hint' }, t('snapShiftHint')),
      );
    } else {
      barChildren.push(
        el('span', { class: 'layout-rig-hint' }, t('layoutRigSelectSchemaHint')),
      );
    }
    const bar = el('div', { class: 'layout-rig-bar' }, ...barChildren);

    const handleNodes = new Map();
    for (const anchor of anchors) {
      const selected = selectedPathKey === anchor.pathKey;
      const coordReadout = el('span', { class: 'layout-rig-coords' },
        `${anchor.x.toFixed(2)}, ${anchor.y.toFixed(2)}`,
      );
      const handle = el('div', {
        class: `layout-rig-handle${selected ? ' selected' : ''}`,
        'data-anchor-path': anchor.pathKey,
        style: `left:${(anchor.x * 100).toFixed(3)}%;top:${(anchor.y * 100).toFixed(3)}%`,
        title: `${anchor.label} (${anchor.pathKey})`,
      },
        el('span', { class: 'layout-rig-dot' }),
        el('span', { class: 'layout-rig-tag' }, anchor.label, coordReadout),
      );
      handleNodes.set(anchor.pathKey, { handle, coordReadout, anchor });

      handle.addEventListener('pointerdown', (ev) => {
        if (ev.button !== 0) return;
        ev.preventDefault();
        ev.stopPropagation();
        selectedPathKey = anchor.pathKey;
        for (const [k, entry] of handleNodes.entries()) {
          entry.handle.classList.toggle('selected', k === selectedPathKey);
        }
        onFocusField?.(inspectedDataDoc.id, anchor.path);

        const rect = overlay.getBoundingClientRect();
        if (rect.width <= 0 || rect.height <= 0) return;
        const mergeKey = `rig:${inspectedDataDoc.id}:${++dragSeq}`;
        const counterpart = mirrorX ? findMirrorAnchor(anchor, anchors) : null;
        dragState = {
          pointerId: ev.pointerId,
          docId: inspectedDataDoc.id,
          anchor,
          counterpart,
          mergeKey,
          moved: false,
        };
        handle.setPointerCapture(ev.pointerId);

        const onMove = (me) => {
          if (!dragState || me.pointerId !== dragState.pointerId) return;
          const curRect = overlay.getBoundingClientRect();
          if (curRect.width <= 0 || curRect.height <= 0) return;
          let nx = (me.clientX - curRect.left) / curRect.width;
          let ny = (me.clientY - curRect.top) / curRect.height;
          if (me.shiftKey) {
            const snap = 0.025;
            nx = Math.round(nx / snap) * snap;
            ny = Math.round(ny / snap) * snap;
          }
          nx = M.tidy(Math.max(anchor.min, Math.min(anchor.max, nx)), 4);
          ny = M.tidy(Math.max(anchor.min, Math.min(anchor.max, ny)), 4);
          dragState.moved = true;

          handle.style.left = `${(nx * 100).toFixed(3)}%`;
          handle.style.top = `${(ny * 100).toFixed(3)}%`;
          coordReadout.textContent = `${nx.toFixed(2)}, ${ny.toFixed(2)}`;

          const updates = [{ path: anchor.path, value: [nx, ny] }];
          if (counterpart) {
            const mx = M.tidy(Math.max(counterpart.min, Math.min(counterpart.max, 1 - nx)), 4);
            updates.push({ path: counterpart.path, value: [mx, ny] });
            const cpNode = handleNodes.get(counterpart.pathKey);
            if (cpNode) {
              cpNode.handle.style.left = `${(mx * 100).toFixed(3)}%`;
              cpNode.handle.style.top = `${(ny * 100).toFixed(3)}%`;
              cpNode.coordReadout.textContent = `${mx.toFixed(2)}, ${ny.toFixed(2)}`;
            }
          }

          const latestDoc = getContext().inspectedDataDoc;
          if (latestDoc?.id === dragState.docId) {
            const ops = buildBatchOps(latestDoc, schema, updates);
            if (ops.length) {
              onEditData?.(`${t('vMove')} ${anchor.label}`, ops, mergeKey, latestDoc.id);
            }
          }
        };

        const onUp = (ue) => {
          if (dragState && ue.pointerId !== dragState.pointerId) return;
          handle.removeEventListener('pointermove', onMove);
          handle.removeEventListener('pointerup', onUp);
          handle.removeEventListener('pointercancel', onUp);
          const wasMoved = dragState?.moved;
          dragState = null;
          if (wasMoved) draw();
        };

        handle.addEventListener('pointermove', onMove);
        handle.addEventListener('pointerup', onUp);
        handle.addEventListener('pointercancel', onUp);
      });
    }

    overlay.replaceChildren(
      safeBox,
      guideX,
      guideY,
      bar,
      ...[...handleNodes.values()].map((v) => v.handle),
    );
  }

  function setupEvents() {
    if (typeof ResizeObserver !== 'undefined') {
      resizeObs = new ResizeObserver(() => syncOverlayBounds());
      const frame = previewFrameEl();
      if (frame) resizeObs.observe(frame);
    }
    window.addEventListener('resize', syncOverlayBounds);
  }

  return {
    setupEvents,
    draw,
    isActiveFor,
    toggleForCurrentDoc,
    syncOverlayBounds,
  };
}
