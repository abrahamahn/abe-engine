// ABE web host: creates the WebGPU device and canvas context, provides the imports of the
// wasm module (crates/abe-web, which links abe-engine's WebGPU backend), executes the command
// streams it submits, forwards input and drives the frame loop.
//
// Imports (wasm module "abe"): gpu_* calls of src/gpu/webgpu/ffi.rs, plus log and status for
// the player. GPU objects live in `objects`; Rust holds their indices (see ffi.rs for the
// lifetime and synchronization rules). Everything here runs on the page's main thread.

const FORMATS = [null, 'rgba8unorm', 'rgba8unorm-srgb', 'bgra8unorm', 'bgra8unorm-srgb',
  'rgba16float', 'rg16float', 'r16float', 'r8unorm', 'r32float', 'rgba32float', 'depth32float'];
const SURFACE = 0xFFFFFFFF;
const EMPTY_GROUP = 0xFFFFFFFE;
const PUSH_RENDER = 0xFFFFFFFD;
const PUSH_COMPUTE = 0xFFFFFFFC;
const PUSH_SLOT = 256;
const VIEW_DIMENSIONS = ['2d', '2d-array', 'cube'];
const SAMPLE_TYPES = ['float', 'unfilterable-float', 'depth', 'sint', 'uint'];
const COMPARES = [undefined, 'never', 'less', 'less-equal', 'equal', 'greater', 'greater-equal',
  'not-equal', 'always'];
const ADDRESS = ['repeat', 'clamp-to-edge', 'mirror-repeat'];

const BLIT_WGSL = `
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f };
@vertex fn vs(@builtin(vertex_index) i: u32) -> Out {
  let uv = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
  var o: Out;
  o.pos = vec4f(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
  o.uv = uv;
  return o;
}
@fragment fn fs(i: Out) -> @location(0) vec4f { return textureSampleLevel(src, smp, i.uv, 0.0); }
`;

/** Creates the WebGPU device and, if `canvas` is given, configures its context. */
export async function createGpu(canvas) {
  if (!navigator.gpu) {
    throw new Error('This browser has no WebGPU. Use Chrome or Edge 113+ or Safari 26+, ' +
      'served over https or from localhost.');
  }
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
  if (!adapter) throw new Error('WebGPU is available but no adapter was found.');
  const wanted = ['depth-clip-control', 'float32-filterable', 'float32-blendable'];
  const requiredFeatures = wanted.filter((f) => adapter.features.has(f));
  const A = adapter.limits;
  const device = await adapter.requestDevice({
    requiredFeatures,
    requiredLimits: {
      maxBufferSize: A.maxBufferSize,
      maxStorageBufferBindingSize: A.maxStorageBufferBindingSize,
      maxTextureDimension2D: A.maxTextureDimension2D,
    },
  });
  const info = adapter.info || {};
  const adapterName = ['WebGPU', info.vendor, info.architecture, info.device, info.description]
    .filter(Boolean).join(' ');
  let context = null;
  let format = null;
  if (canvas) {
    context = canvas.getContext('webgpu');
    format = navigator.gpu.getPreferredCanvasFormat();
    context.configure({
      device,
      format,
      alphaMode: 'opaque',
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
    });
  }
  return { adapter, device, adapterName, features: requiredFeatures, canvas, context, format };
}

/** The import object for the wasm module and the host state behind it. */
export function createHost(gpu, { onLog = () => {}, onStatus = () => {}, onError = () => {} } = {}) {
  const { device, canvas, context } = gpu;
  const objects = [null];
  const freeList = [];
  const errors = [];
  let lastError = '';
  let memory = null;
  let surfaceTexture = null;
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();

  const report = (message) => {
    errors.push(message);
    onError(message);
  };
  device.addEventListener('uncapturederror', (e) => report(`WebGPU: ${e.error.message}`));
  device.lost.then((i) => report(`WebGPU device lost (${i.reason}): ${i.message}`));

  const put = (obj) => {
    const h = freeList.length ? freeList.pop() : objects.length;
    objects[h] = obj;
    return h;
  };
  const u8 = (ptr, len) => new Uint8Array(memory.buffer, ptr >>> 0, len >>> 0);
  const u32 = (ptr, n) => new Uint32Array(memory.buffer, ptr >>> 0, n >>> 0);
  const str = (ptr, len) => decoder.decode(u8(ptr, len));
  const writeStr = (s, out, cap) => {
    const bytes = encoder.encode(s);
    const n = Math.min(bytes.length, cap >>> 0);
    u8(out, n).set(bytes.subarray(0, n));
    return n;
  };
  const guard = (what, f) => (...args) => {
    try {
      return f(...args);
    } catch (e) {
      lastError = `${what}: ${e.message}`;
      return 0;
    }
  };

  const currentTexture = () => {
    if (!surfaceTexture) surfaceTexture = context.getCurrentTexture();
    return surfaceTexture;
  };
  const texture = (h) => (h >>> 0 === SURFACE
    ? { texture: currentTexture(), format: gpu.format, views: new Map() }
    : objects[h >>> 0]);
  const viewOf = (t, dimension, baseMipLevel, mipLevelCount, baseArrayLayer, arrayLayerCount) => {
    const key = `${dimension}/${baseMipLevel}/${mipLevelCount}/${baseArrayLayer}/${arrayLayerCount}`;
    let v = t.views.get(key);
    if (!v) {
      v = t.texture.createView({ dimension, baseMipLevel, mipLevelCount, baseArrayLayer, arrayLayerCount });
      t.views.set(key, v);
    }
    return v;
  };
  const attachment = (h, mip, layer) => {
    if (h >>> 0 === SURFACE) return currentTexture().createView();
    return viewOf(objects[h], '2d', mip, 1, layer, 1);
  };

  // Shared layouts and groups: the empty group and the push-constant ring.
  const emptyLayout = device.createBindGroupLayout({ entries: [] });
  const emptyGroup = device.createBindGroup({ layout: emptyLayout, entries: [] });
  const pushLayout = (visibility) => device.createBindGroupLayout({
    entries: [{ binding: 0, visibility, buffer: { type: 'uniform', hasDynamicOffset: true } }],
  });
  const pushRenderLayout = pushLayout(GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT);
  const pushComputeLayout = pushLayout(GPUShaderStage.COMPUTE);
  let ring = null;
  const ensureRing = (len) => {
    if (ring && ring.size >= len) return;
    let size = 65536;
    while (size < len) size *= 2;
    if (ring) ring.buffer.destroy();
    const buffer = device.createBuffer({ size, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    const group = (layout) => device.createBindGroup({
      layout, entries: [{ binding: 0, resource: { buffer, offset: 0, size: PUSH_SLOT } }],
    });
    ring = { buffer, size, render: group(pushRenderLayout), compute: group(pushComputeLayout) };
  };
  ensureRing(1);
  const group = (h) => {
    switch (h >>> 0) {
      case EMPTY_GROUP: return emptyGroup;
      case PUSH_RENDER: return ring.render;
      case PUSH_COMPUTE: return ring.compute;
      default: return objects[h >>> 0];
    }
  };

  // Blits and mip generation are render passes.
  const blitModule = device.createShaderModule({ code: BLIT_WGSL });
  const blitPipelines = new Map();
  const blitSamplers = [device.createSampler({}),
    device.createSampler({ magFilter: 'linear', minFilter: 'linear' })];
  const blitPipeline = (format, linear) => {
    const key = `${format}/${linear}`;
    let p = blitPipelines.get(key);
    if (!p) {
      const layout = device.createBindGroupLayout({
        entries: [
          { binding: 0, visibility: GPUShaderStage.FRAGMENT,
            texture: { sampleType: linear ? 'float' : 'unfilterable-float' } },
          { binding: 1, visibility: GPUShaderStage.FRAGMENT,
            sampler: { type: linear ? 'filtering' : 'non-filtering' } },
        ],
      });
      const pipeline = device.createRenderPipeline({
        layout: device.createPipelineLayout({ bindGroupLayouts: [layout] }),
        vertex: { module: blitModule, entryPoint: 'vs' },
        fragment: { module: blitModule, entryPoint: 'fs', targets: [{ format }] },
        primitive: { topology: 'triangle-list' },
      });
      p = { pipeline, layout };
      blitPipelines.set(key, p);
    }
    return p;
  };
  const blit = (enc, src, srcMip, srcLayer, dst, dstMip, dstLayer, linear) => {
    const s = objects[src];
    const dstFormat = dst >>> 0 === SURFACE ? gpu.format : objects[dst].format;
    const p = blitPipeline(dstFormat, linear);
    const bindGroup = device.createBindGroup({
      layout: p.layout,
      entries: [
        { binding: 0, resource: viewOf(s, '2d', srcMip, 1, srcLayer, 1) },
        { binding: 1, resource: blitSamplers[linear ? 1 : 0] },
      ],
    });
    const pass = enc.beginRenderPass({
      colorAttachments: [{ view: attachment(dst, dstMip, dstLayer), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 0] }],
    });
    pass.setPipeline(p.pipeline);
    pass.setBindGroup(0, bindGroup);
    pass.draw(3);
    pass.end();
  };

  const execute = (cmdPtr, words, pushPtr, pushLen) => {
    const c = u32(cmdPtr, words);
    const f = new Float32Array(memory.buffer, cmdPtr >>> 0, words >>> 0);
    pushLen >>>= 0;
    if (pushLen) {
      ensureRing(pushLen);
      device.queue.writeBuffer(ring.buffer, 0, memory.buffer, pushPtr >>> 0, pushLen);
    }
    const enc = device.createCommandEncoder();
    let pass = null;
    const u64 = (i) => c[i] + c[i + 1] * 4294967296;
    const offsets = (i, n) => Array.from(c.subarray(i, i + n));
    let i = 0;
    while (i < c.length) {
      const op = c[i++];
      switch (op) {
        case 1: { // BEGIN_RENDER
          const colors = c[i++];
          const hasDepth = c[i++];
          const colorAttachments = [];
          for (let k = 0; k < colors; k++) {
            colorAttachments.push({
              view: attachment(c[i], c[i + 1], c[i + 2]),
              loadOp: c[i + 3] ? 'clear' : 'load',
              storeOp: c[i + 4] ? 'store' : 'discard',
              clearValue: [f[i + 5], f[i + 6], f[i + 7], f[i + 8]],
            });
            i += 9;
          }
          let depthStencilAttachment;
          if (hasDepth) {
            depthStencilAttachment = {
              view: attachment(c[i], c[i + 1], c[i + 2]),
              depthLoadOp: c[i + 3] ? 'clear' : 'load',
              depthStoreOp: c[i + 4] ? 'store' : 'discard',
              depthClearValue: f[i + 5],
            };
            i += 6;
          }
          pass = enc.beginRenderPass({ colorAttachments, depthStencilAttachment });
          break;
        }
        case 2: pass.end(); pass = null; break;
        case 3: pass.setPipeline(objects[c[i++]]); break;
        case 4: {
          const index = c[i]; const g = c[i + 1]; const n = c[i + 2];
          pass.setBindGroup(index, group(g), offsets(i + 3, n));
          i += 3 + n;
          break;
        }
        case 5: pass.setVertexBuffer(c[i], objects[c[i + 1]], u64(i + 2)); i += 4; break;
        case 6: pass.setIndexBuffer(objects[c[i]], 'uint32', u64(i + 1)); i += 3; break;
        case 7: pass.setViewport(f[i], f[i + 1], f[i + 2], f[i + 3], 0, 1); i += 4; break;
        case 8: pass.setScissorRect(c[i], c[i + 1], c[i + 2], c[i + 3]); i += 4; break;
        case 9: pass.draw(c[i], c[i + 1], c[i + 2], c[i + 3]); i += 4; break;
        case 10: pass.drawIndexed(c[i], c[i + 1], c[i + 2], c[i + 3] | 0, c[i + 4]); i += 5; break;
        case 11: { // DISPATCH
          const cp = enc.beginComputePass();
          cp.setPipeline(objects[c[i]]);
          const n = c[i + 1];
          i += 2;
          for (let k = 0; k < n; k++) {
            const index = c[i]; const g = c[i + 1]; const no = c[i + 2];
            cp.setBindGroup(index, group(g), offsets(i + 3, no));
            i += 3 + no;
          }
          cp.dispatchWorkgroups(c[i], c[i + 1], c[i + 2]);
          i += 3;
          cp.end();
          break;
        }
        case 12: // COPY_BUFFER
          enc.copyBufferToBuffer(objects[c[i]], u64(i + 1), objects[c[i + 3]], u64(i + 4), u64(i + 6));
          i += 8;
          break;
        case 13: { // COPY_TEXTURE
          const t = texture(c[i]);
          enc.copyTextureToBuffer(
            { texture: t.texture, mipLevel: c[i + 1], origin: [0, 0, c[i + 2]] },
            { buffer: objects[c[i + 5]], offset: u64(i + 6), bytesPerRow: c[i + 8], rowsPerImage: c[i + 4] },
            [c[i + 3], c[i + 4], 1],
          );
          i += 9;
          break;
        }
        case 14: // BLIT
          blit(enc, c[i], c[i + 1], c[i + 2], c[i + 3], c[i + 4], c[i + 5], c[i + 6]);
          i += 7;
          break;
        default:
          throw new Error(`unknown command ${op} at word ${i - 1}`);
      }
    }
    device.queue.submit([enc.finish()]);
  };

  const imports = {
    gpu_info(out, cap) {
      const L = device.limits;
      const features = (device.features.has('depth-clip-control') ? 1 : 0)
        | (device.features.has('float32-filterable') ? 2 : 0)
        | (device.features.has('float32-blendable') ? 4 : 0);
      const v = [L.maxTextureDimension2D, L.maxTextureArrayLayers, L.maxBindGroups,
        L.minUniformBufferOffsetAlignment, L.minStorageBufferOffsetAlignment,
        L.maxUniformBufferBindingSize, Math.min(L.maxStorageBufferBindingSize, 0xFFFFFFFF),
        Math.min(L.maxBufferSize, 0xFFFFFFFF), L.maxColorAttachments, features,
        L.maxComputeWorkgroupsPerDimension, L.maxVertexBufferArrayStride, L.maxVertexAttributes,
        0, 0, 0];
      const n = Math.min(v.length, cap >>> 0);
      u32(out, n).set(v.slice(0, n));
      return n;
    },
    gpu_adapter_name: (out, cap) => writeStr(gpu.adapterName, out, cap),
    gpu_last_error: (out, cap) => writeStr(lastError, out, cap),
    gpu_error_count: () => errors.length,
    gpu_error: (i, out, cap) => writeStr(errors[i >>> 0] || '', out, cap),
    gpu_canvas(out) {
      if (!canvas) return 0;
      const code = Math.max(1, FORMATS.indexOf(gpu.format));
      u32(out, 3).set([canvas.width, canvas.height, code]);
      return 1;
    },
    gpu_create_buffer: guard('createBuffer', (size, usage) => put(device.createBuffer({ size, usage }))),
    gpu_write_buffer(h, offset, ptr, len) {
      device.queue.writeBuffer(objects[h >>> 0], offset, memory.buffer, ptr >>> 0, len >>> 0);
    },
    gpu_create_texture: guard('createTexture', (width, height, layers, mips, format, usage) => put({
      texture: device.createTexture({
        size: [width, height, layers], mipLevelCount: mips, format: FORMATS[format], usage,
      }),
      format: FORMATS[format],
      views: new Map(),
    })),
    gpu_write_texture(h, mip, layer, width, height, bytesPerRow, ptr, len) {
      device.queue.writeTexture(
        { texture: objects[h >>> 0].texture, mipLevel: mip, origin: [0, 0, layer] },
        memory.buffer,
        { offset: ptr >>> 0, bytesPerRow, rowsPerImage: height },
        [width, height, 1],
      );
    },
    gpu_generate_mips(h) {
      const t = objects[h >>> 0];
      const enc = device.createCommandEncoder();
      for (let layer = 0; layer < t.texture.depthOrArrayLayers; layer++) {
        for (let mip = 1; mip < t.texture.mipLevelCount; mip++) {
          blit(enc, h, mip - 1, layer, h, mip, layer, 1);
        }
      }
      device.queue.submit([enc.finish()]);
    },
    gpu_create_sampler: guard('createSampler', (ptr, words) => {
      const d = u32(ptr, words);
      const lod = new Float32Array(memory.buffer, (ptr >>> 0) + 20, 1)[0];
      const desc = {
        magFilter: d[0] ? 'linear' : 'nearest',
        minFilter: d[0] ? 'linear' : 'nearest',
        mipmapFilter: d[1] ? 'linear' : 'nearest',
        addressModeU: ADDRESS[d[2]],
        addressModeV: ADDRESS[d[2]],
        addressModeW: ADDRESS[d[2]],
        maxAnisotropy: d[3],
        lodMaxClamp: lod,
      };
      if (d[4]) desc.compare = COMPARES[d[4]];
      return put(device.createSampler(desc));
    }),
    gpu_create_shader: guard('createShaderModule', (ptr, len) => {
      const code = str(ptr, len);
      const module = device.createShaderModule({ code });
      module.getCompilationInfo().then((info) => {
        const lines = code.split('\n');
        for (const m of info.messages) {
          const where = m.lineNum ? `${m.lineNum}:${m.linePos} ${lines[m.lineNum - 1].trim()}` : '';
          const text = `WGSL ${m.type}: ${m.message} ${where}`;
          if (m.type === 'error') report(text);
          else onLog(1, text);
        }
      });
      return put(module);
    }),
    gpu_create_bind_group_layout: guard('createBindGroupLayout', (ptr, words) => {
      const w = u32(ptr, words);
      const entries = [];
      for (let i = 0; i < w.length; i += 6) {
        const [binding, visibility, type, a, b] = w.subarray(i, i + 5);
        const e = { binding, visibility };
        switch (type) {
          case 0: e.buffer = { type: 'uniform', minBindingSize: b }; break;
          case 1: e.buffer = { type: 'read-only-storage' }; break;
          case 2: e.buffer = { type: 'storage' }; break;
          case 3: e.sampler = { type: a === 2 ? 'comparison' : a === 1 ? 'non-filtering' : 'filtering' }; break;
          case 4: e.texture = { sampleType: SAMPLE_TYPES[a], viewDimension: VIEW_DIMENSIONS[b] }; break;
          case 5: e.storageTexture = { access: 'write-only', format: FORMATS[a], viewDimension: VIEW_DIMENSIONS[b] }; break;
          default: throw new Error(`binding type ${type}`);
        }
        entries.push(e);
      }
      return put(device.createBindGroupLayout({ entries }));
    }),
    gpu_create_pipeline_layout: guard('createPipelineLayout', (ptr, count) => {
      const bindGroupLayouts = Array.from(u32(ptr, count), (h) => {
        switch (h) {
          case 0: return emptyLayout;
          case PUSH_RENDER: return pushRenderLayout;
          case PUSH_COMPUTE: return pushComputeLayout;
          default: return objects[h];
        }
      });
      return put(device.createPipelineLayout({ bindGroupLayouts }));
    }),
    gpu_create_render_pipeline: guard('createRenderPipeline', (ptr, len) => {
      const d = JSON.parse(str(ptr, len));
      d.layout = objects[d.layout];
      d.vertex.module = objects[d.vertex.module];
      if (d.fragment) d.fragment.module = objects[d.fragment.module];
      return put(device.createRenderPipeline(d));
    }),
    gpu_create_compute_pipeline: guard('createComputePipeline', (ptr, len) => {
      const d = JSON.parse(str(ptr, len));
      d.layout = objects[d.layout];
      d.compute.module = objects[d.compute.module];
      return put(device.createComputePipeline(d));
    }),
    gpu_create_bind_group: guard('createBindGroup', (layout, ptr, words) => {
      const w = u32(ptr, words);
      const entries = [];
      for (let i = 0; i < w.length; i += 8) {
        const [binding, kind, h, a, b, c, d, e] = w.subarray(i, i + 8);
        let resource;
        if (kind === 0) {
          resource = { buffer: objects[h], offset: a + b * 4294967296, size: c + d * 4294967296 };
        } else if (kind === 1) {
          resource = viewOf(objects[h], VIEW_DIMENSIONS[a], b, c, d, e);
        } else {
          resource = objects[h];
        }
        entries.push({ binding, resource });
      }
      return put(device.createBindGroup({ layout: objects[layout >>> 0], entries }));
    }),
    gpu_submit: guard('submit', (cmdPtr, words, pushPtr, pushLen) => {
      execute(cmdPtr, words, pushPtr, pushLen);
      return 1;
    }),
    gpu_release(h) {
      h >>>= 0;
      const o = objects[h];
      if (!o) return;
      if (o.texture && o.texture.destroy) o.texture.destroy();
      else if (o instanceof GPUBuffer) o.destroy();
      objects[h] = null;
      freeList.push(h);
    },
    log: (level, ptr, len) => onLog(level, str(ptr, len)),
    status: (ptr, len) => onStatus(str(ptr, len)),
  };

  return {
    imports: { abe: imports },
    errors,
    attach(instance) { memory = instance.exports.memory; },
    /** Forgets the canvas texture of the last frame (call once per animation frame). */
    nextFrame() { surfaceTexture = null; },
    /** The canvas texture of the current frame, if one was rendered. */
    frameTexture() { return surfaceTexture; },
  };
}

const KEY_CODES = { ArrowLeft: 1, ArrowRight: 2, ArrowUp: 3, ArrowDown: 4, ShiftLeft: 5, ShiftRight: 5, Space: 32 };
function keyCode(e) {
  if (e.code in KEY_CODES) return KEY_CODES[e.code];
  let m = /^Key([A-Z])$/.exec(e.code);
  if (m) return m[1].toLowerCase().charCodeAt(0);
  m = /^(?:Digit|Numpad)([0-9])$/.exec(e.code);
  if (m) return m[1].charCodeAt(0);
  return 0;
}

/**
 * Streams `url` into wasm memory from `abe_alloc`, reporting progress (0..1) and bytes. With
 * an uncompressed Content-Length the bytes go straight into wasm memory; otherwise (unknown
 * length, or a compressed response that turns out longer) they are buffered and copied once.
 */
async function fetchInto(url, exports, onProgress) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status} ${response.statusText}`);
  const encoding = response.headers.get('content-encoding');
  const announced = Number(response.headers.get('content-length')) || 0;
  const reader = response.body.getReader();
  let ptr = 0;
  let cap = 0;
  let offset = 0;
  let chunks = null;
  if (announced > 0 && (!encoding || encoding === 'identity')) {
    cap = announced;
    ptr = exports.abe_alloc(cap) >>> 0;
  } else {
    chunks = [];
  }
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    if (!chunks && offset + value.length > cap) {
      // More than announced: keep what arrived and buffer the rest.
      chunks = [new Uint8Array(exports.memory.buffer, ptr, offset).slice()];
      exports.abe_free(ptr, cap);
      ptr = 0;
    }
    if (chunks) chunks.push(value);
    else new Uint8Array(exports.memory.buffer, ptr + offset, value.length).set(value);
    offset += value.length;
    onProgress(announced ? Math.min(offset / announced, 1) : 0, offset);
  }
  if (!chunks) {
    if (offset !== cap) {
      exports.abe_free(ptr, cap);
      throw new Error(`${url}: received ${offset} of ${cap} bytes`);
    }
    return { ptr, len: cap };
  }
  ptr = exports.abe_alloc(offset) >>> 0;
  let at = 0;
  for (const chunk of chunks) {
    new Uint8Array(exports.memory.buffer, ptr + at, chunk.length).set(chunk);
    at += chunk.length;
  }
  return { ptr, len: offset };
}

const nextFrame = () => new Promise((r) => requestAnimationFrame(() => r()));

/** WebGPU, the host and the wasm module; shared by the player and the self-test. */
async function boot(options) {
  const { canvas, onPhase = () => {}, onLog = () => {} } = options;
  const errors = [];
  const fail = (text) => {
    errors.push(text);
    (options.onError || (() => {}))(text);
  };
  onPhase('Starting WebGPU', 0);
  const gpu = await createGpu(canvas);
  onLog(0, `${gpu.adapterName}; features: ${gpu.features.join(', ') || 'none'}; canvas ${gpu.format}`);
  const host = createHost(gpu, { onLog, onStatus: options.onStatus || (() => {}), onError: fail });
  onPhase('Loading the engine', 0);
  const wasmResponse = await fetch(options.wasm);
  if (!wasmResponse.ok) throw new Error(`${options.wasm}: HTTP ${wasmResponse.status}`);
  let instance;
  try {
    ({ instance } = await WebAssembly.instantiateStreaming(wasmResponse.clone(), host.imports));
  } catch (_) {
    ({ instance } = await WebAssembly.instantiate(await wasmResponse.arrayBuffer(), host.imports));
  }
  host.attach(instance);
  return { gpu, host, x: instance.exports, errors, fail };
}

/** Reads `texture` (the canvas texture of the current frame) back as RGBA8 pixels. */
async function readPixels(gpu, texture) {
  const width = texture.width; const height = texture.height;
  const bytesPerRow = Math.ceil(width * 4 / 256) * 256;
  const buffer = gpu.device.createBuffer({ size: bytesPerRow * height, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
  const enc = gpu.device.createCommandEncoder();
  enc.copyTextureToBuffer({ texture }, { buffer, bytesPerRow }, [width, height, 1]);
  gpu.device.queue.submit([enc.finish()]);
  await buffer.mapAsync(GPUMapMode.READ);
  const src = new Uint8Array(buffer.getMappedRange());
  const rgba = new Uint8ClampedArray(width * height * 4);
  const bgra = gpu.format.startsWith('bgra');
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const s = y * bytesPerRow + x * 4; const d = (y * width + x) * 4;
      rgba[d] = src[s + (bgra ? 2 : 0)]; rgba[d + 1] = src[s + 1];
      rgba[d + 2] = src[s + (bgra ? 0 : 2)]; rgba[d + 3] = 255;
    }
  }
  buffer.unmap();
  buffer.destroy();
  return { width, height, rgba };
}

/**
 * Runs the GPU-layer self-test of the wasm module (compute, copies, mips, a blit into the
 * canvas) and resolves with the canvas pixels and any errors.
 */
export async function startSelfTest(options) {
  const { gpu, host, x, errors, fail } = await boot(options);
  if (options.sizeCanvas) options.sizeCanvas();
  const pixels = await new Promise((resolve, reject) => requestAnimationFrame(() => {
    host.nextFrame();
    let ok = 0;
    try {
      ok = x.abe_selftest();
    } catch (e) {
      fail(`the self-test stopped: ${e.message}`);
    }
    const texture = host.frameTexture();
    if (!ok || !texture) {
      reject(new Error(errors[errors.length - 1] || 'the self-test presented nothing'));
      return;
    }
    readPixels(gpu, texture).then(resolve, reject);
  }));
  // Let asynchronous WebGPU errors arrive.
  await gpu.device.queue.onSubmittedWorkDone();
  await nextFrame();
  return {
    adapter: gpu.adapterName,
    format: gpu.format,
    pixels,
    errors: () => [...errors, ...host.errors.filter((e) => !errors.includes(e))],
  };
}

/**
 * Loads the player: WebGPU, the wasm module, the world and the Look file; then runs frames.
 * `options`: canvas, wasm (URL), scene (URL), look (URL or ''), query (string),
 * onPhase(text, progress), onLog(level, text), onStatus(text), onError(text), sizeCanvas().
 * Returns a controller for input and tests.
 */
export async function startPlayer(options) {
  const { canvas, onPhase = () => {} } = options;
  const { gpu, host, x, errors, fail } = await boot(options);
  const call = (what, f) => {
    try {
      return f();
    } catch (e) {
      throw new Error(`${what} stopped: ${e.message}${errors.length ? ` (${errors[errors.length - 1]})` : ''}`);
    }
  };

  const world = await fetchInto(options.scene, x, (p, bytes) =>
    onPhase(`Downloading the world (${(bytes / 1048576).toFixed(1)} MB)`, p));
  onPhase('Decoding the world', 1);
  await nextFrame();
  if (!call('decoding', () => x.abe_load_world(world.ptr, world.len))) {
    throw new Error(errors[errors.length - 1] || 'the world could not be decoded');
  }
  let lookText = '';
  if (options.look) {
    onPhase('Loading the look', 1);
    const r = await fetch(options.look);
    if (!r.ok) throw new Error(`${options.look}: HTTP ${r.status} ${r.statusText}`);
    lookText = await r.text();
  }
  let views = [];
  try {
    views = (JSON.parse(lookText || '{}').views || []).map((v) => String(v.name));
  } catch (_) {
    // The engine reports Look file errors.
  }
  const toWasm = (text) => {
    const bytes = new TextEncoder().encode(text);
    const ptr = x.abe_alloc(bytes.length) >>> 0;
    new Uint8Array(x.memory.buffer, ptr, bytes.length).set(bytes);
    return { ptr, len: bytes.length };
  };
  const look = toWasm(lookText);
  const query = toWasm(options.query || '');
  onPhase('Compiling shaders and uploading', 1);
  await nextFrame();
  if (options.sizeCanvas) options.sizeCanvas();
  const started = call('start', () => x.abe_start(look.ptr, look.len, query.ptr, query.len));
  x.abe_free(look.ptr, look.len);
  x.abe_free(query.ptr, query.len);
  if (!started) throw new Error(errors[errors.length - 1] || 'the player could not start');
  onPhase('', 1);

  // Input.
  const keys = new Set();
  const key = (code, down) => {
    if (!code) return;
    if (down) keys.add(code); else keys.delete(code);
    x.abe_key(code, down ? 1 : 0);
  };
  window.addEventListener('keydown', (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const code = keyCode(e);
    if (!code) return;
    if (code <= 4 || code === 32) e.preventDefault();
    if (!e.repeat) key(code, true);
  });
  window.addEventListener('keyup', (e) => key(keyCode(e), false));
  window.addEventListener('blur', () => { keys.clear(); x.abe_blur(); });
  const pointers = new Map();
  let pinch = null;
  const centroid = () => {
    let sx = 0; let sy = 0;
    for (const p of pointers.values()) { sx += p.x; sy += p.y; }
    return { x: sx / pointers.size, y: sy / pointers.size };
  };
  canvas.addEventListener('pointerdown', (e) => {
    if (e.pointerType === 'mouse' && e.button !== 0) return;
    canvas.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 2) pinch = centroid();
    e.preventDefault();
  });
  canvas.addEventListener('pointermove', (e) => {
    const p = pointers.get(e.pointerId);
    if (!p) return;
    const dx = e.clientX - p.x; const dy = e.clientY - p.y;
    p.x = e.clientX; p.y = e.clientY;
    if (pointers.size === 1) {
      x.abe_look(dx * (e.pointerType === 'touch' ? 1.4 : 1), dy * (e.pointerType === 'touch' ? 1.4 : 1));
    } else if (pointers.size === 2 && pinch) {
      const c = centroid();
      const scale = Math.min(canvas.clientWidth, canvas.clientHeight) * 0.25;
      x.abe_move((pinch.y - c.y) / scale, (c.x - pinch.x) / scale);
    }
  });
  const lift = (e) => {
    pointers.delete(e.pointerId);
    if (pointers.size < 2 && pinch) { pinch = null; x.abe_move(0, 0); }
  };
  canvas.addEventListener('pointerup', lift);
  canvas.addEventListener('pointercancel', lift);

  // Frames.
  let last = performance.now();
  let running = true;
  let captures = [];
  const capture = (texture) => readPixels(gpu, texture);
  const tick = (now) => {
    if (!running) return;
    if (options.sizeCanvas) options.sizeCanvas();
    const dt = Math.min(Math.max((now - last) / 1000, 0), 0.25);
    last = now;
    host.nextFrame();
    let ok = 0;
    try {
      ok = x.abe_frame(dt);
    } catch (e) {
      fail(`the engine stopped: ${e.message}`);
    }
    if (!ok) {
      running = false;
      if (!errors.length) fail('the engine stopped');
      return;
    }
    const texture = host.frameTexture();
    if (captures.length && texture) {
      const pending = captures;
      captures = [];
      capture(texture).then((img) => pending.forEach((r) => r.resolve(img)),
        (e) => pending.forEach((r) => r.reject(e)));
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);

  return {
    adapter: gpu.adapterName,
    format: gpu.format,
    views,
    exports: x,
    errors: () => [...errors, ...host.errors.filter((e) => !errors.includes(e))],
    frames: () => x.abe_frames(),
    key,
    stop() { running = false; },
    /** The next rendered frame as RGBA pixels. */
    capture: () => new Promise((resolve, reject) => captures.push({ resolve, reject })),
  };
}
