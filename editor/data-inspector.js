// Schema-driven inspector and path/array edit helpers for `Kind::Data` (`*.data.json`) documents.
import * as M from './math.js';
import { assetBasename, assetFolder, projectAssetUrl } from './asset-library.js';

function clampZero(min, max) {
  let v = 0;
  if (Number.isFinite(min)) v = Math.max(min, v);
  if (Number.isFinite(max)) v = Math.min(max, v);
  return v;
}

/** Computes the effective default value for a schema field definition. */
export function schemaDefaultValue(field) {
  if (!field || typeof field !== 'object') return null;
  if (field.default !== undefined) return structuredClone(field.default);
  switch (field.type) {
    case 'bool':
      return false;
    case 'int':
      return clampZero(
        Number.isFinite(field.min) ? Math.ceil(field.min) : undefined,
        Number.isFinite(field.max) ? Math.floor(field.max) : undefined,
      );
    case 'float':
      return clampZero(field.min, field.max);
    case 'string':
      return '';
    case 'enum':
      return Array.isArray(field.values) && field.values.length ? field.values[0] : '';
    case 'vec2':
      return [0, 0];
    case 'vec3':
      return [0, 0, 0];
    case 'vec4':
      return [0, 0, 0, 0];
    case 'quat':
      return [0, 0, 0, 1];
    case 'rgb':
      return [1, 1, 1];
    case 'rgba':
      return [1, 1, 1, 1];
    case 'asset':
    case 'entity':
      return null;
    case 'array':
      return [];
    case 'object': {
      const out = {};
      for (const [k, sub] of Object.entries(field.fields || {})) {
        out[k] = schemaDefaultValue(sub);
      }
      return out;
    }
    default:
      return null;
  }
}

/** Materializes effective values of a `.data.json` `values` object merged with schema defaults. */
export function materializeDataValues(values, schemaDef) {
  const stored = values && typeof values === 'object' && !Array.isArray(values) ? values : {};
  if (!schemaDef?.fields) return structuredClone(stored);
  const out = {};
  for (const [k, f] of Object.entries(schemaDef.fields)) {
    out[k] = materializeField(stored[k], f);
  }
  return out;
}

function materializeField(stored, field) {
  if (!field) return stored === undefined ? null : structuredClone(stored);
  if (stored === undefined) return schemaDefaultValue(field);
  if (field.type === 'object' && field.fields && stored && typeof stored === 'object' && !Array.isArray(stored)) {
    const out = {};
    for (const [k, sub] of Object.entries(field.fields)) {
      out[k] = materializeField(stored[k], sub);
    }
    return out;
  }
  if (field.type === 'array' && Array.isArray(stored) && field.items) {
    return stored.map((item) => materializeField(item, field.items));
  }
  return structuredClone(stored);
}

/** Returns true if `schemaDef` opts into `"layout-rig"` or declares `vec2` fields. */
export function schemaHasLayoutAnchors(schemaDef) {
  if (!schemaDef || typeof schemaDef !== 'object') return false;
  if (schemaDef.editor === 'layout-rig') return true;
  const walk = (fields) => {
    for (const f of Object.values(fields || {})) {
      if (!f) continue;
      if (f.type === 'vec2') return true;
      if (f.type === 'object' && walk(f.fields)) return true;
      if (f.type === 'array' && f.items) {
        if (f.items.type === 'vec2') return true;
        if (f.items.type === 'object' && walk(f.items.fields)) return true;
      }
    }
    return false;
  };
  return walk(schemaDef.fields);
}

/** Returns true if `schemaDef` opts into `"sequencer"` or declares timed cue objects. */
export function schemaHasSequencerCues(schemaDef) {
  if (!schemaDef || typeof schemaDef !== 'object') return false;
  if (schemaDef.editor === 'sequencer') return true;
  const isCueFields = (fields) => {
    if (!fields || typeof fields !== 'object') return false;
    return (
      (fields.delay && (fields.delay.type === 'float' || fields.delay.type === 'int')) ||
      (fields.duration && (fields.duration.type === 'float' || fields.duration.type === 'int'))
    );
  };
  for (const f of Object.values(schemaDef.fields || {})) {
    if (!f) continue;
    if (f.type === 'array' && f.items?.type === 'object' && isCueFields(f.items.fields)) return true;
    if (f.type === 'object' && f.fields) {
      if (isCueFields(f.fields)) return true;
      for (const sub of Object.values(f.fields)) {
        if (sub?.type === 'object' && isCueFields(sub.fields)) return true;
      }
    }
  }
  return false;
}

/**
 * Builds `Document` ops for a path inside `values` (`relativePath` does NOT include `'values'`).
 * Because `ganbate_scene::doc::Document` treats arrays as atomic values (ops never index into arrays),
 * any edit reaching inside an array automatically clones and replaces the outermost enclosing array.
 */
export function buildDataValueOps(currentValues, schemaDef, relativePath, value, isRemove = false) {
  const path = relativePath.map((seg) => String(seg));
  if (!path.length) return [];

  const rootVals = currentValues && typeof currentValues === 'object' && !Array.isArray(currentValues)
    ? currentValues
    : {};

  // Locate whether any segment along `path` is an array (either in stored data or in schema).
  let curVal = rootVals;
  let curField = schemaDef ? { type: 'object', fields: schemaDef.fields || {} } : null;
  let arrayCut = -1;
  let arrayStored = undefined;
  let arrayField = null;

  for (let i = 0; i < path.length; i++) {
    const seg = path[i];
    if (curField?.type === 'object' && curField.fields) {
      curField = curField.fields[seg] || null;
    } else if (curField?.type === 'array') {
      curField = curField.items || null;
    } else {
      curField = null;
    }
    curVal = curVal && typeof curVal === 'object' ? curVal[seg] : undefined;

    if (i < path.length - 1 && (Array.isArray(curVal) || curField?.type === 'array')) {
      arrayCut = i + 1;
      arrayStored = Array.isArray(curVal) ? curVal : schemaDefaultValue(curField);
      arrayField = curField;
      break;
    }
  }

  if (arrayCut < 0) {
    if (isRemove) {
      return [{ remove: ['values', ...path] }];
    }
    return [{ set: ['values', ...path], value }];
  }

  const arrayPath = path.slice(0, arrayCut);
  const rest = path.slice(arrayCut);
  const nextArray = structuredClone(Array.isArray(arrayStored) ? arrayStored : []);
  mutateNestedContainer(nextArray, arrayField, rest, value, isRemove);
  return [{ set: ['values', ...arrayPath], value: nextArray }];
}

function mutateNestedContainer(container, containerField, rest, value, isRemove) {
  if (!rest.length) return;
  const [head, ...tail] = rest;
  if (Array.isArray(container)) {
    const idx = Number(head);
    if (!Number.isInteger(idx) || idx < 0 || idx >= container.length) return;
    if (!tail.length) {
      if (isRemove) {
        container.splice(idx, 1);
      } else {
        container[idx] = value;
      }
      return;
    }
    const itemField = containerField?.items || null;
    if (!container[idx] || typeof container[idx] !== 'object') {
      container[idx] = itemField ? schemaDefaultValue(itemField) : {};
    }
    mutateNestedContainer(container[idx], itemField, tail, value, isRemove);
    return;
  }
  if (container && typeof container === 'object') {
    if (!tail.length) {
      if (isRemove) {
        delete container[head];
      } else {
        container[head] = value;
      }
      return;
    }
    const subField = containerField?.fields?.[head] || null;
    if (!container[head] || typeof container[head] !== 'object') {
      container[head] = subField ? schemaDefaultValue(subField) : {};
    }
    mutateNestedContainer(container[head], subField, tail, value, isRemove);
  }
}

let sliderDragSeq = 0;
let previewAudio = null;
const collapsedGroups = new Set();

function playSoundAsset(asset) {
  if (!asset) return;
  if (previewAudio) {
    previewAudio.pause();
    previewAudio = null;
  }
  const audio = new Audio(projectAssetUrl(asset.path));
  previewAudio = audio;
  audio.play().catch(() => {});
}

/** Scrolls to and highlights a field row inside `#inspector-body` matching `pathArray`. */
export function focusDataField(box, pathArray) {
  if (!box || !Array.isArray(pathArray) || !pathArray.length) return false;
  const parts = pathArray.map((p) => String(p));
  let row = null;
  for (let len = parts.length; len >= 1 && !row; len--) {
    const candidate = parts.slice(0, len).join('.');
    row = box.querySelector(`[data-data-path="${CSS.escape(candidate)}"]`);
  }
  if (!row) return false;
  let parent = row.parentElement;
  while (parent && parent !== box) {
    if (parent.tagName === 'DETAILS') parent.open = true;
    parent = parent.parentElement;
  }
  for (const prev of box.querySelectorAll('.data-field-flash')) {
    prev.classList.remove('data-field-flash');
  }
  row.classList.add('data-field-flash');
  row.scrollIntoView({ block: 'nearest' });
  const ctrl = row.querySelector('input:not([type="range"]), select, textarea, input[type="range"]');
  ctrl?.focus({ preventScroll: true });
  return true;
}

/** Renders the full schema-driven inspector for a `Kind::Data` document. */
export function drawDataInspector(box, ctx) {
  const {
    doc,
    schema,
    assets,
    entities,
    el,
    t,
    label,
    hasAssetDrag,
    resolveDraggedAsset,
    draggingAsset,
    editData,
    renameData,
    retry,
    back,
    layoutRigActive,
    onToggleLayoutRig,
    sequencerActive,
    onToggleSequencer,
    log,
  } = ctx;

  const headerActions = el('div', { class: 'data-header-actions' },
    schemaHasLayoutAnchors(schema)
      ? el('button', {
          type: 'button',
          id: 'toggle-layout-rig',
          class: `mini${layoutRigActive ? ' on' : ''}`,
          'data-key': 'data-toggle-rig',
          title: t('layoutRigTip'),
          onclick: () => onToggleLayoutRig?.(),
        }, t('layoutRig'))
      : null,
    schemaHasSequencerCues(schema)
      ? el('button', {
          type: 'button',
          id: 'toggle-sequencer',
          class: `mini${sequencerActive ? ' on' : ''}`,
          'data-key': 'data-toggle-seq',
          title: t('sequencerTip'),
          onclick: () => onToggleSequencer?.(),
        }, t('sequencer'))
      : null,
    el('button', {
      type: 'button',
      id: 'data-back-to-scene',
      class: 'mini',
      'data-key': 'data-back',
      onclick: back,
    }, t('backToScene')),
  );

  if (!doc) {
    box.append(headerActions, el('p', { class: 'hint' }, t('dataLoading')));
    return;
  }

  if (doc.error) {
    box.append(
      headerActions,
      el('div', { class: 'meta' }, doc.path),
      el('p', { class: 'look-error' }, doc.error),
      el('button', { type: 'button', class: 'mini', onclick: retry }, t('lookRetry')),
    );
    return;
  }

  const root = doc.root || {};
  const schemaName = root.schema || '';
  const values = root.values && typeof root.values === 'object' && !Array.isArray(root.values)
    ? root.values
    : {};
  const isStyleTokens = schema?.editor === 'style-tokens';

  box.append(
    el('div', { class: 'data-inspector-head', 'data-key': 'data-head' },
      el('div', { class: 'data-inspector-badges' },
        el('span', { class: 'kind-tag kind-data' }, t('kind_data')),
        schemaName ? el('span', { class: 'data-schema-badge', title: schema?.doc || schemaName }, schemaName) : null,
      ),
      headerActions,
    ),
    el('div', { class: 'meta' }, `${doc.path} · id ${doc.id}`),
  );

  if (schema?.doc) {
    box.append(el('p', { class: 'data-schema-doc' }, schema.doc));
  }

  box.append(el('div', {
    class: 'field',
    'data-key': 'data.#name',
    'data-data-path': 'name',
  },
    el('label', {}, t('name')),
    el('input', {
      type: 'text',
      value: root.name || '',
      onchange: (ev) => renameData(ev.target.value),
    }),
  ));

  if (Array.isArray(doc.issues) && doc.issues.length) {
    box.append(el('div', { class: 'data-issues', 'data-key': 'data.#issues' },
      doc.issues.map((issue) => el('div', { class: 'look-error' },
        issue.path ? `${issue.path}: ${issue.message}` : issue.message,
      )),
    ));
  }

  if (!schema) {
    box.append(el('p', { class: 'hint' }, t('noSchema', schemaName || '?')));
    return;
  }

  const bodyWrap = el('div', {
    class: `data-fields-wrap${isStyleTokens ? ' style-tokens-mode' : ''}`,
  });

  const commitPath = (path, nextVal, mergeKey) => {
    const ops = buildDataValueOps(doc.root?.values || {}, schema, path, nextVal, false);
    if (ops.length) editData(`${t('vSet')} ${path.join('.')}`, ops, mergeKey);
  };

  const resetPath = (path) => {
    const ops = buildDataValueOps(doc.root?.values || {}, schema, path, undefined, true);
    if (ops.length) editData(`${t('vReset')} ${path.join('.')}`, ops);
  };

  const renderScalarOrLeaf = (path, fieldDef, stored, customCommit, customReset) => {
    const pathKey = path.join('.');
    const leaf = path[path.length - 1];
    const def = schemaDefaultValue(fieldDef);
    const value = stored === undefined ? def : stored;
    const ty = fieldDef.type;
    const unit = fieldDef.unit ? ` ${fieldDef.unit}` : '';
    const readonly = !!fieldDef.readonly;
    const doCommit = customCommit || ((v, merge) => commitPath(path, v, merge));
    const doReset = customReset || (stored !== undefined ? () => resetPath(path) : null);

    const makeNum = (v, onchange, { step = 'any', min, max } = {}) => {
      let last = v ?? 0;
      const attrs = {
        type: 'number',
        step: fieldDef.step !== undefined ? String(fieldDef.step) : step,
        value: String(last),
        disabled: readonly,
        onchange: (ev) => {
          const raw = ev.target.value.trim();
          const n = raw === '' ? NaN : Number(raw);
          if (!Number.isFinite(n)) {
            ev.target.value = String(last);
            return;
          }
          last = ty === 'int' ? Math.round(n) : n;
          onchange(last);
        },
      };
      if (Number.isFinite(min)) attrs.min = String(min);
      if (Number.isFinite(max)) attrs.max = String(max);
      return el('input', attrs);
    };

    let control;
    if (ty === 'bool') {
      control = el('input', {
        type: 'checkbox',
        disabled: readonly,
        onchange: (ev) => doCommit(ev.target.checked),
      });
      control.checked = !!value;
    } else if (ty === 'float' || ty === 'int') {
      const hasRange = Number.isFinite(fieldDef.min) && Number.isFinite(fieldDef.max);
      const stepVal = fieldDef.step !== undefined
        ? String(fieldDef.step)
        : ty === 'int'
          ? '1'
          : String(M.tidy((fieldDef.max - fieldDef.min) / 200 || 0.01, 5));
      if (hasRange) {
        let activeMerge = null;
        const numInput = makeNum(value, (n) => {
          rangeInput.value = String(n);
          doCommit(n);
        }, { step: ty === 'int' ? '1' : 'any', min: fieldDef.min, max: fieldDef.max });
        const rangeInput = el('input', {
          type: 'range',
          min: String(fieldDef.min),
          max: String(fieldDef.max),
          step: stepVal,
          value: String(value ?? 0),
          disabled: readonly,
          onpointerdown: () => {
            activeMerge = `slider:${doc.id}:${pathKey}:${++sliderDragSeq}`;
          },
          oninput: (ev) => {
            const raw = Number(ev.target.value);
            const clean = ty === 'int' ? Math.round(raw) : M.tidy(raw, 5);
            numInput.value = String(clean);
            if (!activeMerge) activeMerge = `slider:${doc.id}:${pathKey}:${++sliderDragSeq}`;
            doCommit(clean, activeMerge);
          },
          onchange: () => {
            activeMerge = null;
          },
        });
        control = el('span', { class: 'data-range-pair' }, rangeInput, numInput);
      } else {
        control = makeNum(value, (n) => doCommit(n), {
          step: ty === 'int' ? '1' : 'any',
          min: fieldDef.min,
          max: fieldDef.max,
        });
      }
    } else if (ty === 'string') {
      control = el('input', {
        type: 'text',
        value: value ?? '',
        disabled: readonly,
        onchange: (ev) => doCommit(ev.target.value),
      });
    } else if (ty === 'enum') {
      control = el('select', {
        disabled: readonly,
        onchange: (ev) => doCommit(ev.target.value),
      }, (fieldDef.values || []).map((v) => el('option', { value: v, selected: v === value }, v)));
    } else if (ty === 'vec2' || ty === 'vec3' || ty === 'vec4') {
      const len = Number(ty.slice(3));
      const arr = (Array.isArray(value) ? value : new Array(len).fill(0)).slice(0, len);
      while (arr.length < len) arr.push(0);
      control = el('span', { class: 'vec' }, arr.map((x, i) => makeNum(x, (n) => {
        arr[i] = n;
        doCommit(arr.slice());
      })));
    } else if (ty === 'quat') {
      const eul = M.eulerFromQuat(Array.isArray(value) && value.length === 4 ? value : [0, 0, 0, 1]);
      control = el('span', { class: 'vec', title: t('eulerTip') }, eul.map((x, i) => makeNum(x, (n) => {
        eul[i] = n;
        doCommit(M.quatFromEuler(eul).map((q) => M.tidy(q, 6)));
      })));
    } else if (ty === 'rgb' || ty === 'rgba') {
      const v = (Array.isArray(value) ? value : [1, 1, 1, 1]).slice();
      while (v.length < (ty === 'rgba' ? 4 : 3)) v.push(1);
      const hex = M.hexOfLinear(v);
      const hexBadge = el('code', { class: 'color-hex-code' }, hex);
      const colorPicker = el('input', {
        type: 'color',
        value: hex,
        disabled: readonly,
        oninput: (ev) => {
          const c = M.linearOfHex(ev.target.value);
          v.splice(0, 3, ...c.map((x) => M.tidy(x, 5)));
          hexBadge.textContent = ev.target.value;
          doCommit(ty === 'rgba' ? [v[0], v[1], v[2], v[3] ?? 1] : [v[0], v[1], v[2]], `color:${doc.id}:${pathKey}`);
        },
        onchange: (ev) => {
          const c = M.linearOfHex(ev.target.value);
          v.splice(0, 3, ...c.map((x) => M.tidy(x, 5)));
          hexBadge.textContent = ev.target.value;
          doCommit(ty === 'rgba' ? [v[0], v[1], v[2], v[3] ?? 1] : [v[0], v[1], v[2]]);
        },
      });
      control = el('span', { class: 'vec data-color-control' },
        colorPicker,
        hexBadge,
        ty === 'rgba'
          ? makeNum(v[3] ?? 1, (a) => {
              v[3] = a;
              doCommit([v[0], v[1], v[2], a]);
            }, { min: 0, max: 1 })
          : null,
      );
    } else if (ty === 'asset') {
      const options = assets.filter((a) => !fieldDef.kind || a.kind === fieldDef.kind);
      const nameCounts = new Map();
      for (const a of options) {
        const base = assetBasename(a.path);
        nameCounts.set(base, (nameCounts.get(base) || 0) + 1);
      }
      const selectEl = el('select', {
        disabled: readonly,
        onchange: (ev) => doCommit(ev.target.value || null),
      },
        el('option', { value: '' }, t('none')),
        options.map((a) => {
          const base = assetBasename(a.path);
          const dir = assetFolder(a.path);
          const text = (nameCounts.get(base) || 0) > 1 && dir ? `${base} (${dir})` : base;
          return el('option', { value: a.id, selected: a.id === value, title: a.path }, text);
        }),
      );
      const selectedSound = fieldDef.kind === 'sound' && value
        ? assets.find((a) => a.id === value && a.kind === 'sound')
        : null;
      control = selectedSound
        ? el('span', { class: 'vec' },
            selectEl,
            el('button', {
              type: 'button',
              class: 'mini',
              title: t('previewSound'),
              onclick: () => playSoundAsset(selectedSound),
            }, '▶'),
          )
        : selectEl;
    } else if (ty === 'entity') {
      const entEntries = Object.entries(entities || {});
      if (entEntries.length) {
        control = el('select', {
          disabled: readonly,
          onchange: (ev) => doCommit(ev.target.value || null),
        },
          el('option', { value: '' }, t('none')),
          entEntries.map(([eid, ent]) => el('option', {
            value: eid,
            selected: eid === value,
          }, `${ent.name || eid} (${eid.slice(0, 6)})`)),
        );
      } else {
        control = el('input', {
          type: 'text',
          value: value ?? '',
          placeholder: '16-hex entity id',
          disabled: readonly,
          onchange: (ev) => doCommit(ev.target.value.trim() || null),
        });
      }
    } else {
      control = el('input', {
        type: 'text',
        value: JSON.stringify(value ?? null),
        disabled: readonly,
        onchange: (ev) => {
          try {
            doCommit(JSON.parse(ev.target.value));
          } catch (err) {
            log?.(2, `${pathKey}: ${err.message}`);
          }
        },
      });
    }

    const resetBtn = doReset && !readonly
      ? el('button', {
          type: 'button',
          class: 'mini',
          title: t('resetDefault'),
          onclick: doReset,
        }, '↺')
      : el('span');

    const row = el('div', {
      class: `field${path.length > 1 ? ' subfield' : ''}${stored === undefined ? ' default' : ''}`,
      title: fieldDef.doc || '',
      'data-key': `data.${pathKey}`,
      'data-data-path': pathKey,
      'data-asset-kind': ty === 'asset' ? (fieldDef.kind || '*') : null,
    },
      el('label', {}, `${label(fieldDef, leaf)}${unit}`),
      control,
      resetBtn,
    );

    if (ty === 'asset' && !readonly) {
      const accepts = (asset) => asset && (!fieldDef.kind || asset.kind === fieldDef.kind);
      row.addEventListener('dragover', (ev) => {
        if (!hasAssetDrag?.(ev)) return;
        if (draggingAsset && !accepts(draggingAsset)) return;
        ev.preventDefault();
        ev.stopPropagation();
        if (ev.dataTransfer) ev.dataTransfer.dropEffect = 'copy';
        row.classList.add('drop-target');
      });
      row.addEventListener('dragleave', (ev) => {
        if (!row.contains(ev.relatedTarget)) row.classList.remove('drop-target');
      });
      row.addEventListener('drop', (ev) => {
        row.classList.remove('drop-target');
        const asset = resolveDraggedAsset?.(ev);
        if (!accepts(asset)) return;
        ev.preventDefault();
        ev.stopPropagation();
        doCommit(asset.id);
      });
    }

    return row;
  };

  const renderArrayField = (container, path, fieldDef, stored, customCommit, customReset) => {
    const pathKey = path.join('.');
    const leaf = path[path.length - 1];
    const itemsDef = fieldDef.items || { type: 'string' };
    const effectiveArr = Array.isArray(stored)
      ? stored
      : Array.isArray(fieldDef.default)
        ? fieldDef.default
        : [];
    const maxItems = Number.isFinite(fieldDef.max) ? fieldDef.max : Infinity;
    const commitArr = customCommit || ((nextArr, merge) => commitPath(path, nextArr, merge));
    const doReset = customReset || (stored !== undefined ? () => resetPath(path) : null);

    const groupKey = `${doc.id}:${pathKey}`;
    const details = el('details', {
      class: 'data-array-group',
      open: !collapsedGroups.has(groupKey),
      'data-key': `data.${pathKey}#array`,
      'data-data-path': pathKey,
      ontoggle: (ev) => {
        if (ev.currentTarget.open) collapsedGroups.delete(groupKey);
        else collapsedGroups.add(groupKey);
      },
    });

    const countText = Number.isFinite(maxItems)
      ? `${effectiveArr.length} / ${maxItems}`
      : String(effectiveArr.length);
    const summary = el('summary', { class: 'data-group-summary', title: fieldDef.doc || '' },
      el('span', { class: 'data-group-title' }, label(fieldDef, leaf)),
      el('span', { class: 'count-badge' }, countText),
      el('span', { class: 'grow' }),
      el('button', {
        type: 'button',
        class: 'mini',
        'data-action': 'array-add',
        disabled: !!fieldDef.readonly || effectiveArr.length >= maxItems,
        onclick: (ev) => {
          ev.preventDefault();
          ev.stopPropagation();
          const next = structuredClone(effectiveArr);
          next.push(schemaDefaultValue(itemsDef));
          commitArr(next);
        },
      }, t('addItem')),
      doReset && !fieldDef.readonly
        ? el('button', {
            type: 'button',
            class: 'mini',
            title: t('resetDefault'),
            onclick: (ev) => {
              ev.preventDefault();
              ev.stopPropagation();
              doReset();
            },
          }, '↺')
        : null,
    );
    details.append(summary);

    const listWrap = el('div', { class: 'data-array-items' });
    effectiveArr.forEach((itemVal, idx) => {
      const itemPath = [...path, String(idx)];
      const itemPathKey = itemPath.join('.');
      const itemTitle = itemVal && typeof itemVal === 'object' && (itemVal.name || itemVal.id || itemVal.label)
        ? `#${idx + 1} · ${itemVal.name || itemVal.id || itemVal.label}`
        : `#${idx + 1}`;
      const itemBox = el('div', {
        class: 'data-array-item',
        'data-key': `data.${itemPathKey}#item`,
        'data-data-path': itemPathKey,
      });
      const itemBar = el('div', { class: 'data-array-item-bar' },
        el('strong', {}, itemTitle),
        el('span', { class: 'grow' }),
        el('button', {
          type: 'button',
          class: 'mini',
          'data-action': 'array-up',
          disabled: !!fieldDef.readonly || idx === 0,
          title: t('moveUp'),
          onclick: () => {
            const next = structuredClone(effectiveArr);
            [next[idx - 1], next[idx]] = [next[idx], next[idx - 1]];
            commitArr(next);
          },
        }, '↑'),
        el('button', {
          type: 'button',
          class: 'mini',
          'data-action': 'array-down',
          disabled: !!fieldDef.readonly || idx === effectiveArr.length - 1,
          title: t('moveDown'),
          onclick: () => {
            const next = structuredClone(effectiveArr);
            [next[idx], next[idx + 1]] = [next[idx + 1], next[idx]];
            commitArr(next);
          },
        }, '↓'),
        el('button', {
          type: 'button',
          class: 'mini',
          'data-action': 'array-remove',
          disabled: !!fieldDef.readonly,
          title: t('vRemove'),
          onclick: () => {
            const next = structuredClone(effectiveArr);
            next.splice(idx, 1);
            commitArr(next);
          },
        }, '✕'),
      );
      itemBox.append(itemBar);

      if (itemsDef.type === 'object' && itemsDef.fields) {
        const itemObj = itemVal && typeof itemVal === 'object' && !Array.isArray(itemVal) ? itemVal : {};
        for (const [subName, subField] of Object.entries(itemsDef.fields)) {
          if (subField.hidden) continue;
          renderFieldNode(
            itemBox,
            [...itemPath, subName],
            subField,
            itemObj[subName],
            (subVal, merge) => {
              const next = structuredClone(effectiveArr);
              const targetObj = next[idx] && typeof next[idx] === 'object' && !Array.isArray(next[idx])
                ? next[idx]
                : {};
              targetObj[subName] = subVal;
              next[idx] = targetObj;
              commitArr(next, merge);
            },
            itemObj[subName] !== undefined
              ? () => {
                  const next = structuredClone(effectiveArr);
                  if (next[idx] && typeof next[idx] === 'object') delete next[idx][subName];
                  commitArr(next);
                }
              : null,
          );
        }
      } else {
        itemBox.append(renderScalarOrLeaf(
          itemPath,
          itemsDef,
          itemVal,
          (subVal, merge) => {
            const next = structuredClone(effectiveArr);
            next[idx] = subVal;
            commitArr(next, merge);
          },
          null,
        ));
      }

      listWrap.append(itemBox);
    });

    details.append(listWrap);
    container.append(details);
  };

  const renderFieldNode = (container, path, fieldDef, stored, customCommit = null, customReset = null) => {
    if (!fieldDef || fieldDef.hidden) return;
    if (fieldDef.type === 'array') {
      renderArrayField(container, path, fieldDef, stored, customCommit, customReset);
      return;
    }
    if (fieldDef.type === 'object' && fieldDef.fields) {
      const pathKey = path.join('.');
      const leaf = path[path.length - 1];
      const obj = stored && typeof stored === 'object' && !Array.isArray(stored) ? stored : undefined;
      const groupKey = `${doc.id}:${pathKey}`;
      const doReset = customReset || (stored !== undefined ? () => resetPath(path) : null);
      const details = el('details', {
        class: `data-object-group${stored === undefined ? ' default' : ''}`,
        open: !collapsedGroups.has(groupKey),
        'data-key': `data.${pathKey}#group`,
        'data-data-path': pathKey,
        ontoggle: (ev) => {
          if (ev.currentTarget.open) collapsedGroups.delete(groupKey);
          else collapsedGroups.add(groupKey);
        },
      },
        el('summary', { class: 'data-group-summary', title: fieldDef.doc || '' },
          el('span', { class: 'data-group-title' }, label(fieldDef, leaf)),
          el('span', { class: 'grow' }),
          doReset && !fieldDef.readonly
            ? el('button', {
                type: 'button',
                class: 'mini',
                title: t('resetDefault'),
                onclick: (ev) => {
                  ev.preventDefault();
                  ev.stopPropagation();
                  doReset();
                },
              }, '↺')
            : null,
        ),
      );
      const inner = el('div', { class: 'data-object-fields' });
      for (const [subName, subField] of Object.entries(fieldDef.fields)) {
        if (subField.hidden) continue;
        if (customCommit) {
          renderFieldNode(
            inner,
            [...path, subName],
            subField,
            obj?.[subName],
            (subVal, merge) => {
              const nextObj = structuredClone(obj || {});
              nextObj[subName] = subVal;
              customCommit(nextObj, merge);
            },
            obj?.[subName] !== undefined
              ? () => {
                  const nextObj = structuredClone(obj || {});
                  delete nextObj[subName];
                  customCommit(nextObj);
                }
              : null,
          );
        } else {
          renderFieldNode(inner, [...path, subName], subField, obj?.[subName]);
        }
      }
      details.append(inner);
      container.append(details);
      return;
    }
    container.append(renderScalarOrLeaf(path, fieldDef, stored, customCommit, customReset));
  };

  for (const [fieldName, fieldDef] of Object.entries(schema.fields || {})) {
    renderFieldNode(bodyWrap, [fieldName], fieldDef, values[fieldName]);
  }
  box.append(bodyWrap);
}
