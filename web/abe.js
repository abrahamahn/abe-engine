// ABE web host: creates the WebGPU device (or, without WebGPU, a WebGL 2 context) and the
// canvas context, provides the imports of the wasm module (crates/abe-web, which links
// abe-engine's WebGPU and WebGL 2 backends), executes the command streams it submits, forwards
// input and drives the frame loop. `api: 'webgl2'` (the page's ?api=webgl2) forces WebGL 2.
//
// Imports (wasm module "abe"): gpu_* calls of src/gpu/webgpu/ffi.rs, gl_* calls of
// src/gpu/webgl/ffi.rs, gpu_api (1 WebGPU, 2 WebGL 2), plus log and status for the player. GPU objects live in `objects`; Rust holds their indices (see ffi.rs for the
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
  return { api: 'webgpu', adapter, device, adapterName, features: requiredFeatures, canvas, context, format };
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
    gpu_api: () => 1,
  };
  // The WebGL 2 imports exist in the module too; on this host they are never reached.
  const unavailable = () => { throw new Error('WebGL 2 call on the WebGPU host'); };
  for (const name of GL_IMPORTS) imports[name] = unavailable;

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


// ---------------------------------------------------------------------------------- WebGL 2
//
// The fallback host for browsers without WebGPU: provides the gl_* imports of
// src/gpu/webgl/ffi.rs and executes the command streams of src/gpu/webgl/record.rs. Shaders
// arrive as GLSL ES 3.00 from abe-shader's GLSL emitter, which keeps Vulkan's clip space apart
// from the depth range: images lie in GL memory with row 0 at the top, so the canvas texture
// is flipped when it is copied to the canvas.

const GL_SURFACE = 0xFFFFFFFF;
const ROW_TEXELS = 1024; // abe_shader::glsl::STORAGE_ROW_TEXELS
const ROW_BYTES = ROW_TEXELS * 16;
const BUF_DATA = 1;
const BUF_INDEX = 2;
const BUF_STORAGE = 4;
const BUF_READBACK = 8;

const BLIT_VS = `#version 300 es
out vec2 uv;
void main() {
  uv = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  gl_Position = vec4(uv * 2.0 - 1.0, 0.0, 1.0);
}`;
const blitFs = (array) => `#version 300 es
precision highp float;
precision highp sampler2DArray;
uniform highp ${array ? 'sampler2DArray' : 'sampler2D'} src;
uniform float layer;
in vec2 uv;
out vec4 color;
void main() { color = ${array ? 'texture(src, vec3(uv, layer))' : 'texture(src, uv)'}; }`;

/** Creates a WebGL 2 context on `canvas` (or an offscreen one) with the extensions ABE uses. */
export function createGl(canvas) {
  const target = canvas || (typeof OffscreenCanvas !== 'undefined' ? new OffscreenCanvas(1, 1) : document.createElement('canvas'));
  const gl = target.getContext('webgl2', {
    alpha: false, antialias: false, depth: false, stencil: false, premultipliedAlpha: false,
    preserveDrawingBuffer: false, powerPreference: 'high-performance',
  });
  if (!gl) {
    throw new Error('This browser has neither WebGPU nor WebGL 2.');
  }
  const ext = {
    colorFloat: gl.getExtension('EXT_color_buffer_float'),
    floatLinear: gl.getExtension('OES_texture_float_linear'),
    floatBlend: gl.getExtension('EXT_float_blend'),
    aniso: gl.getExtension('EXT_texture_filter_anisotropic'),
    provoking: gl.getExtension('WEBGL_provoking_vertex'),
    indexed: gl.getExtension('OES_draw_buffers_indexed'),
  };
  // Flat varyings come from the first vertex, as on Vulkan and WebGPU.
  if (ext.provoking) ext.provoking.provokingVertexWEBGL(ext.provoking.FIRST_VERTEX_CONVENTION_WEBGL);
  const debug = gl.getExtension('WEBGL_debug_renderer_info');
  const renderer = debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER);
  const features = Object.entries(ext).filter(([, v]) => v).map(([k]) => k);
  return { api: 'webgl2', gl, ext, canvas, adapterName: `WebGL 2 ${renderer}`, features, format: 'rgba8unorm' };
}

/** The import object for the wasm module on WebGL 2 and the host state behind it. */
export function createGlHost(gpu, { onLog = () => {}, onStatus = () => {}, onError = () => {}, checkErrors = true } = {}) {
  const { gl, ext, canvas } = gpu;
  const objects = [null];
  const freeList = [];
  const errors = [];
  let lastError = '';
  let memory = null;
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const report = (message) => {
    errors.push(message);
    onError(message);
  };
  if (canvas && canvas.addEventListener) {
    canvas.addEventListener('webglcontextlost', () => report('WebGL 2 context lost'));
  }
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
  const glErrors = (what) => {
    if (!checkErrors) return;
    for (let e = gl.getError(), n = 0; e !== gl.NO_ERROR && n < 8; e = gl.getError(), n++) {
      report(`WebGL error 0x${e.toString(16)} in ${what}`);
    }
  };

  const FORMATS = [null,
    { internal: gl.RGBA8, format: gl.RGBA, type: gl.UNSIGNED_BYTE, size: 4 },
    { internal: gl.SRGB8_ALPHA8, format: gl.RGBA, type: gl.UNSIGNED_BYTE, size: 4 },
    { internal: gl.RGBA8, format: gl.RGBA, type: gl.UNSIGNED_BYTE, size: 4, bgra: true },
    { internal: gl.SRGB8_ALPHA8, format: gl.RGBA, type: gl.UNSIGNED_BYTE, size: 4, bgra: true },
    { internal: gl.RGBA16F, format: gl.RGBA, type: gl.HALF_FLOAT, size: 8, channels: 4, half: true },
    { internal: gl.RG16F, format: gl.RG, type: gl.HALF_FLOAT, size: 4, channels: 2, half: true },
    { internal: gl.R16F, format: gl.RED, type: gl.HALF_FLOAT, size: 2, channels: 1, half: true },
    { internal: gl.R8, format: gl.RED, type: gl.UNSIGNED_BYTE, size: 1, channels: 1, unorm: true },
    { internal: gl.R32F, format: gl.RED, type: gl.FLOAT, size: 4, channels: 1, float: true },
    { internal: gl.RGBA32F, format: gl.RGBA, type: gl.FLOAT, size: 16, channels: 4, float: true },
    { internal: gl.DEPTH_COMPONENT32F, format: gl.DEPTH_COMPONENT, type: gl.FLOAT, size: 4, depth: true },
  ];
  const COMPARES = [0, gl.NEVER, gl.LESS, gl.LEQUAL, gl.EQUAL, gl.GREATER, gl.GEQUAL, gl.NOTEQUAL, gl.ALWAYS];
  const WRAP = [gl.REPEAT, gl.CLAMP_TO_EDGE, gl.MIRRORED_REPEAT];
  const MODES = [gl.TRIANGLES, gl.TRIANGLE_STRIP, gl.LINES];

  // A texture unit the stream never uses, for uploads and mip generation.
  const scratchUnit = gl.getParameter(gl.MAX_COMBINED_TEXTURE_IMAGE_UNITS) - 1;
  let uid = 0;
  const textures = new Set();
  const bindScratch = (t) => {
    gl.activeTexture(gl.TEXTURE0 + scratchUnit);
    gl.bindTexture(t.target, t.tex);
  };
  const setLevels = (t, base, count) => {
    if (t.base === base && t.count === count) return;
    gl.texParameteri(t.target, gl.TEXTURE_BASE_LEVEL, base);
    gl.texParameteri(t.target, gl.TEXTURE_MAX_LEVEL, base + count - 1);
    t.base = base;
    t.count = count;
  };
  const makeTexture = (w, h, layers, mips, f, array) => {
    const target = array ? gl.TEXTURE_2D_ARRAY : gl.TEXTURE_2D;
    const t = { tex: gl.createTexture(), target, f, w, h, layers, mips, base: 0, count: mips, uid: ++uid };
    bindScratch(t);
    if (array) gl.texStorage3D(target, mips, f.internal, w, h, layers);
    else gl.texStorage2D(target, mips, f.internal, w, h);
    textures.add(t);
    return t;
  };

  // The canvas texture (RGBA8) and its copy to the canvas.
  let surface = null;
  const ensureSurface = () => {
    if (!canvas) return null;
    const w = canvas.width; const h = canvas.height;
    if (w === 0 || h === 0) return null;
    if (!surface || surface.w !== w || surface.h !== h) {
      if (surface) dropTexture(surface);
      surface = makeTexture(w, h, 1, 1, FORMATS[1], false);
    }
    return surface;
  };
  const texture = (h) => (h >>> 0 === GL_SURFACE ? ensureSurface() : objects[h >>> 0]);

  // Framebuffers, cached by attachments.
  const fbos = new Map();
  const attach = (point, t, mip, layer) => {
    if (t.target === gl.TEXTURE_2D_ARRAY) gl.framebufferTextureLayer(gl.FRAMEBUFFER, point, t.tex, mip, layer);
    else gl.framebufferTexture2D(gl.FRAMEBUFFER, point, gl.TEXTURE_2D, t.tex, mip);
  };
  const framebuffer = (colors, depth) => {
    const key = [...colors, depth].map((a) => (a ? `${a.t.uid}.${a.mip}.${a.layer}` : '-')).join('|');
    let f = fbos.get(key);
    if (!f) {
      f = gl.createFramebuffer();
      gl.bindFramebuffer(gl.FRAMEBUFFER, f);
      colors.forEach((a, i) => attach(gl.COLOR_ATTACHMENT0 + i, a.t, a.mip, a.layer));
      if (depth) attach(gl.DEPTH_ATTACHMENT, depth.t, depth.mip, depth.layer);
      gl.drawBuffers(colors.length ? colors.map((_, i) => gl.COLOR_ATTACHMENT0 + i) : [gl.NONE]);
      const status = gl.checkFramebufferStatus(gl.FRAMEBUFFER);
      if (status !== gl.FRAMEBUFFER_COMPLETE) {
        gl.deleteFramebuffer(f);
        throw new Error(`framebuffer incomplete (0x${status.toString(16)}) for ${key}`);
      }
      f.uids = [...colors, depth].filter(Boolean).map((a) => a.t.uid);
      fbos.set(key, f);
    } else {
      gl.bindFramebuffer(gl.FRAMEBUFFER, f);
    }
    return f;
  };
  const dropTexture = (t) => {
    for (const [k, f] of fbos) {
      if (f.uids.includes(t.uid)) { gl.deleteFramebuffer(f); fbos.delete(k); }
    }
    gl.deleteTexture(t.tex);
    textures.delete(t);
  };
  const readFbo = gl.createFramebuffer();

  // Fixed-function state, applied by difference.
  const fixed = {};
  const setFlag = (cap, on) => {
    if (fixed[cap] !== on) {
      if (on) gl.enable(cap); else gl.disable(cap);
      fixed[cap] = on;
    }
  };
  const resetFixed = () => { for (const k of Object.keys(fixed)) delete fixed[k]; };
  let current = null; // pipeline
  let currentProgram = null;
  const applyPipeline = (p) => {
    if (currentProgram !== p.program.program) {
      gl.useProgram(p.program.program);
      currentProgram = p.program.program;
    }
    current = p;
    setFlag(gl.CULL_FACE, p.cull !== 0);
    if (p.cull !== 0 && fixed.cullFace !== p.cull) { gl.cullFace(p.cull === 1 ? gl.BACK : gl.FRONT); fixed.cullFace = p.cull; }
    // Vulkan counter-clockwise is clockwise in GL window terms (see the module comment).
    const front = p.frontCcw ? gl.CW : gl.CCW;
    if (fixed.frontFace !== front) { gl.frontFace(front); fixed.frontFace = front; }
    setFlag(gl.DEPTH_TEST, p.depthTest);
    if (p.depthTest) {
      if (fixed.depthFunc !== p.depthFunc) { gl.depthFunc(p.depthFunc); fixed.depthFunc = p.depthFunc; }
      if (fixed.depthMask !== p.depthWrite) { gl.depthMask(p.depthWrite); fixed.depthMask = p.depthWrite; }
    }
    const offset = p.bias !== 0 || p.slope !== 0;
    setFlag(gl.POLYGON_OFFSET_FILL, offset);
    if (offset && (fixed.bias !== p.bias || fixed.slope !== p.slope)) {
      gl.polygonOffset(p.slope, p.bias);
      fixed.bias = p.bias; fixed.slope = p.slope;
    }
    // Targets the pipeline does not write are left out of the draw buffers.
    if (p.targets.length && fixed.drawBuffers !== p.drawBuffers) {
      gl.drawBuffers(p.targets.map((t, i) => (t.mask ? gl.COLOR_ATTACHMENT0 + i : gl.NONE)));
      fixed.drawBuffers = p.drawBuffers;
    }
    if (p.indexed) {
      // Per-target blend state and masks (OES_draw_buffers_indexed).
      const x = ext.indexed;
      p.targets.forEach((t, i) => {
        if (t.blend) x.enableiOES(gl.BLEND, i); else x.disableiOES(gl.BLEND, i);
        if (t.blend) blendFunc((...a) => x.blendFuncSeparateiOES(i, ...a), t.blend);
        x.colorMaskiOES(i, !!(t.mask & 1), !!(t.mask & 2), !!(t.mask & 4), !!(t.mask & 8));
      });
      delete fixed[gl.BLEND];
      delete fixed.blendMode;
      delete fixed.colorMask;
      return;
    }
    setFlag(gl.BLEND, p.blend !== 0);
    if (p.blend !== 0 && fixed.blendMode !== p.blend) {
      blendFunc((...a) => gl.blendFuncSeparate(...a), p.blend);
      gl.blendEquation(gl.FUNC_ADD);
      fixed.blendMode = p.blend;
    }
    if (fixed.colorMask !== p.mask) {
      gl.colorMask(!!(p.mask & 1), !!(p.mask & 2), !!(p.mask & 4), !!(p.mask & 8));
      fixed.colorMask = p.mask;
    }
  };
  const blendFunc = (set, mode) => {
    if (mode === 1) set(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    else if (mode === 2) set(gl.ONE, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    else set(gl.ONE, gl.ONE, gl.ONE, gl.ONE);
  };

  // Samplers: the host's nearest sampler for units without one (texelFetch, storage).
  const nearest = gl.createSampler();
  gl.samplerParameteri(nearest, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.samplerParameteri(nearest, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  for (const w of [gl.TEXTURE_WRAP_S, gl.TEXTURE_WRAP_T, gl.TEXTURE_WRAP_R]) gl.samplerParameteri(nearest, w, gl.CLAMP_TO_EDGE);
  const linear = gl.createSampler();
  gl.samplerParameteri(linear, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.samplerParameteri(linear, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  for (const w of [gl.TEXTURE_WRAP_S, gl.TEXTURE_WRAP_T, gl.TEXTURE_WRAP_R]) gl.samplerParameteri(linear, w, gl.CLAMP_TO_EDGE);

  // Programs.
  const compile = (type, source) => {
    const sh = gl.createShader(type);
    gl.shaderSource(sh, source);
    gl.compileShader(sh);
    if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
      const log = gl.getShaderInfoLog(sh) || '';
      gl.deleteShader(sh);
      const lines = source.split('\n');
      const where = [...log.matchAll(/ERROR: \d+:(\d+)/g)].slice(0, 3)
        .map((m) => `  ${m[1]}: ${(lines[Number(m[1]) - 1] || '').trim()}`).join('\n');
      throw new Error(`${type === gl.VERTEX_SHADER ? 'vertex' : 'fragment'} shader: ${log}${where ? `\n${where}` : ''}`);
    }
    return sh;
  };
  const link = (vs, fs) => {
    const p = gl.createProgram();
    const v = compile(gl.VERTEX_SHADER, vs);
    const f = compile(gl.FRAGMENT_SHADER, fs);
    gl.attachShader(p, v);
    gl.attachShader(p, f);
    gl.linkProgram(p);
    gl.deleteShader(v);
    gl.deleteShader(f);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) {
      const log = gl.getProgramInfoLog(p);
      gl.deleteProgram(p);
      throw new Error(`link: ${log}`);
    }
    return p;
  };
  const blitPrograms = [false, true].map((array) => {
    const program = link(BLIT_VS, blitFs(array));
    return { program, src: gl.getUniformLocation(program, 'src'), layer: gl.getUniformLocation(program, 'layer') };
  });
  const emptyVao = gl.createVertexArray();

  // The push-constant ring.
  const ring = gl.createBuffer();
  let ringSize = 0;

  // Buffers.
  const bufferTarget = (b) => (b.index ? gl.ELEMENT_ARRAY_BUFFER : gl.COPY_WRITE_BUFFER);
  const bindForWrite = (b) => {
    if (b.index) {
      gl.bindVertexArray(null);
      currentVao = null;
    }
    gl.bindBuffer(bufferTarget(b), b.gl);
  };
  const uploadRows = (b, offset, len) => {
    if (!len) return;
    const first = Math.floor(offset / ROW_BYTES);
    const last = Math.floor((offset + len - 1) / ROW_BYTES);
    gl.activeTexture(gl.TEXTURE0 + scratchUnit);
    gl.bindTexture(gl.TEXTURE_2D, b.tex);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 4);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, first, ROW_TEXELS, last - first + 1, gl.RGBA_INTEGER, gl.UNSIGNED_INT,
      new Uint32Array(b.shadow.buffer, first * ROW_BYTES, (last - first + 1) * ROW_TEXELS * 4));
  };
  /** The bytes of a buffer range (read back from GL when the buffer has no copy here). */
  const bytesOf = (b, offset, size) => {
    if (b.shadow) return b.shadow.subarray(offset, offset + size);
    if (b.host) return b.host.subarray(offset, offset + size);
    const out = new Uint8Array(size);
    gl.bindBuffer(gl.COPY_READ_BUFFER, b.gl);
    gl.getBufferSubData(gl.COPY_READ_BUFFER, offset, out);
    return out;
  };
  const writeBytes = (b, offset, data) => {
    if (b.gl) {
      bindForWrite(b);
      gl.bufferSubData(bufferTarget(b), offset, data);
    }
    if (b.shadow) {
      b.shadow.set(data, offset);
      uploadRows(b, offset, data.length);
    }
    if (b.host) b.host.set(data, offset);
  };

  // Half floats, for reading float textures back in their own format.
  const f32 = new Float32Array(1);
  const i32 = new Uint32Array(f32.buffer);
  const toHalf = (v) => {
    f32[0] = v;
    const x = i32[0];
    const sign = (x >>> 16) & 0x8000;
    const e = (x >>> 23) & 0xff;
    const m = x & 0x7fffff;
    if (e === 0xff) return sign | 0x7c00 | (m ? 0x200 : 0);
    const he = e - 127 + 15;
    if (he >= 0x1f) return sign | 0x7c00;
    if (he <= 0) {
      if (he < -10) return sign;
      const mm = (m | 0x800000) >> (1 - he);
      return sign | ((mm + 0x1000) >> 13);
    }
    return sign | (he << 10) | ((m + 0x1000) >> 13);
  };

  let currentVao = null;
  const blit = (src, srcMip, srcLayer, dst, dstMip, dstLayer, filterLinear) => {
    const s = texture(src);
    const d = texture(dst);
    const w = Math.max(1, d.w >> dstMip); const h = Math.max(1, d.h >> dstMip);
    framebuffer([{ t: d, mip: dstMip, layer: dstLayer }], null);
    const bp = blitPrograms[s.target === gl.TEXTURE_2D_ARRAY ? 1 : 0];
    gl.useProgram(bp.program);
    currentProgram = bp.program;
    gl.uniform1i(bp.src, 0);
    gl.uniform1f(bp.layer, srcLayer);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(s.target, s.tex);
    setLevels(s, srcMip, 1);
    gl.bindSampler(0, filterLinear ? linear : nearest);
    gl.bindVertexArray(emptyVao);
    currentVao = emptyVao;
    resetFixed();
    for (const cap of [gl.DEPTH_TEST, gl.BLEND, gl.CULL_FACE, gl.SCISSOR_TEST, gl.POLYGON_OFFSET_FILL]) gl.disable(cap);
    gl.colorMask(true, true, true, true);
    gl.viewport(0, 0, w, h);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    current = null;
  };

  const execute = (cmdPtr, words, pushPtr, pushLen) => {
    const c = u32(cmdPtr, words).slice();
    const f = new Float32Array(c.buffer);
    pushLen >>>= 0;
    if (pushLen) {
      gl.bindBuffer(gl.UNIFORM_BUFFER, ring);
      if (pushLen > ringSize) {
        ringSize = Math.max(pushLen, 65536);
        gl.bufferData(gl.UNIFORM_BUFFER, ringSize, gl.DYNAMIC_DRAW);
      }
      gl.bufferSubData(gl.UNIFORM_BUFFER, 0, u8(pushPtr, pushLen));
    }
    let discard = [];
    let i = 0;
    while (i < c.length) {
      const op = c[i++];
      switch (op) {
        case 1: { // BEGIN_PASS
          const colors = c[i++];
          const hasDepth = c[i++];
          const cs = [];
          for (let k = 0; k < colors; k++) {
            cs.push({ t: texture(c[i]), mip: c[i + 1], layer: c[i + 2], load: c[i + 3], store: c[i + 4],
              clear: [f[i + 5], f[i + 6], f[i + 7], f[i + 8]] });
            i += 9;
          }
          let depth = null;
          if (hasDepth) {
            depth = { t: texture(c[i]), mip: c[i + 1], layer: c[i + 2], load: c[i + 3], store: c[i + 4], clear: f[i + 5] };
            i += 6;
          }
          framebuffer(cs, depth);
          gl.drawBuffers(cs.length ? cs.map((_, k) => gl.COLOR_ATTACHMENT0 + k) : [gl.NONE]);
          fixed.drawBuffers = cs.map(() => 1).join('');
          const a = cs[0] || depth;
          gl.viewport(0, 0, Math.max(1, a.t.w >> a.mip), Math.max(1, a.t.h >> a.mip));
          setFlag(gl.SCISSOR_TEST, false);
          // Clears obey the write masks.
          const dontCare = [];
          cs.forEach((x, k) => {
            if (x.load === 1) {
              if (fixed.colorMask !== 15) { gl.colorMask(true, true, true, true); fixed.colorMask = 15; }
              gl.clearBufferfv(gl.COLOR, k, x.clear);
            } else if (x.load === 2) dontCare.push(gl.COLOR_ATTACHMENT0 + k);
          });
          if (depth) {
            if (depth.load === 1) {
              if (fixed.depthMask !== true) { gl.depthMask(true); fixed.depthMask = true; }
              gl.clearBufferfv(gl.DEPTH, 0, [depth.clear]);
            } else if (depth.load === 2) dontCare.push(gl.DEPTH_ATTACHMENT);
          }
          if (dontCare.length) gl.invalidateFramebuffer(gl.FRAMEBUFFER, dontCare);
          discard = cs.map((x, k) => (x.store ? null : gl.COLOR_ATTACHMENT0 + k)).filter((x) => x !== null);
          if (depth && !depth.store) discard.push(gl.DEPTH_ATTACHMENT);
          break;
        }
        case 2: // END_PASS
          if (discard.length) gl.invalidateFramebuffer(gl.FRAMEBUFFER, discard);
          discard = [];
          break;
        case 3: applyPipeline(objects[c[i++]]); break;
        case 4: gl.viewport(Math.round(f[i]), Math.round(f[i + 1]), Math.round(f[i + 2]), Math.round(f[i + 3])); i += 4; break;
        case 5:
          setFlag(gl.SCISSOR_TEST, true);
          gl.scissor(c[i], c[i + 1], c[i + 2], c[i + 3]);
          i += 4;
          break;
        case 6: gl.bindBufferRange(gl.UNIFORM_BUFFER, c[i], objects[c[i + 1]].gl, c[i + 2], c[i + 3]); i += 4; break;
        case 7: gl.bindBufferRange(gl.UNIFORM_BUFFER, 0, ring, c[i], c[i + 1]); i += 2; break;
        case 8: { // BIND_UNIT
          const t = texture(c[i + 1]);
          gl.activeTexture(gl.TEXTURE0 + c[i]);
          gl.bindTexture(t.target, t.tex);
          setLevels(t, c[i + 3], c[i + 4]);
          gl.bindSampler(c[i], c[i + 2] ? objects[c[i + 2]] : nearest);
          i += 5;
          break;
        }
        case 9: { // BIND_STORAGE
          gl.activeTexture(gl.TEXTURE0 + c[i]);
          gl.bindTexture(gl.TEXTURE_2D, objects[c[i + 1]].tex);
          gl.bindSampler(c[i], nearest);
          i += 2;
          break;
        }
        case 10: { // SET_HIDDEN
          const h = current.program.hidden[c[i]];
          if (h && h.loc) {
            if (h.uint) gl.uniform1ui(h.loc, c[i + 1]);
            else gl.uniform1i(h.loc, c[i + 1] | 0);
          }
          i += 2;
          break;
        }
        case 11: {
          const v = objects[c[i++]].vao;
          if (currentVao !== v) { gl.bindVertexArray(v); currentVao = v; }
          break;
        }
        case 12: gl.drawArraysInstanced(current.mode, c[i + 2], c[i], c[i + 1]); i += 3; break;
        case 13: gl.drawElementsInstanced(current.mode, c[i], gl.UNSIGNED_INT, c[i + 2] * 4, c[i + 1]); i += 3; break;
        case 14: { // COPY_BUFFER
          const s = objects[c[i]]; const d = objects[c[i + 2]];
          const so = c[i + 1]; const dO = c[i + 3]; const size = c[i + 4];
          if (s.gl && d.gl && !d.shadow && !d.host) {
            gl.bindBuffer(gl.COPY_READ_BUFFER, s.gl);
            gl.bindBuffer(gl.COPY_WRITE_BUFFER, d.gl);
            gl.copyBufferSubData(gl.COPY_READ_BUFFER, gl.COPY_WRITE_BUFFER, so, dO, size);
          } else {
            writeBytes(d, dO, bytesOf(s, so, size).slice());
          }
          i += 5;
          break;
        }
        case 15: { // COPY_TEXTURE
          const t = texture(c[i]); const mip = c[i + 1]; const layer = c[i + 2];
          const w = c[i + 3]; const h = c[i + 4]; const b = objects[c[i + 5]]; const off = c[i + 6];
          gl.bindFramebuffer(gl.READ_FRAMEBUFFER, readFbo);
          if (t.target === gl.TEXTURE_2D_ARRAY) gl.framebufferTextureLayer(gl.READ_FRAMEBUFFER, gl.COLOR_ATTACHMENT0, t.tex, mip, layer);
          else gl.framebufferTexture2D(gl.READ_FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, t.tex, mip);
          gl.readBuffer(gl.COLOR_ATTACHMENT0);
          gl.pixelStorei(gl.PACK_ALIGNMENT, 1);
          const fm = t.f;
          let out;
          if (fm.type === gl.UNSIGNED_BYTE && !fm.unorm) {
            out = new Uint8Array(w * h * 4);
            gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, out);
            if (fm.bgra) for (let k = 0; k < out.length; k += 4) { const r = out[k]; out[k] = out[k + 2]; out[k + 2] = r; }
          } else if (fm.unorm) {
            const rgba = new Uint8Array(w * h * 4);
            gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, rgba);
            out = new Uint8Array(w * h);
            for (let k = 0; k < w * h; k++) out[k] = rgba[k * 4];
          } else {
            const rgba = new Float32Array(w * h * 4);
            gl.readPixels(0, 0, w, h, gl.RGBA, gl.FLOAT, rgba);
            const n = fm.channels;
            if (fm.half) {
              const halfs = new Uint16Array(w * h * n);
              for (let k = 0; k < w * h; k++) for (let ch = 0; ch < n; ch++) halfs[k * n + ch] = toHalf(rgba[k * 4 + ch]);
              out = new Uint8Array(halfs.buffer);
            } else {
              const fl = new Float32Array(w * h * n);
              for (let k = 0; k < w * h; k++) for (let ch = 0; ch < n; ch++) fl[k * n + ch] = rgba[k * 4 + ch];
              out = new Uint8Array(fl.buffer);
            }
          }
          b.host.set(out, off);
          gl.bindFramebuffer(gl.READ_FRAMEBUFFER, null);
          i += 7;
          break;
        }
        case 16: // BLIT
          blit(c[i], c[i + 1], c[i + 2], c[i + 3], c[i + 4], c[i + 5], c[i + 6]);
          i += 7;
          break;
        default:
          throw new Error(`unknown command ${op} at word ${i - 1}`);
      }
    }
  };

  const present = () => {
    const s = ensureSurface();
    if (!s) return;
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, readFbo);
    gl.framebufferTexture2D(gl.READ_FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, s.tex, 0);
    gl.readBuffer(gl.COLOR_ATTACHMENT0);
    gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, null);
    setFlag(gl.SCISSOR_TEST, false);
    // Row 0 of the surface is the top of the image; GL's window origin is the bottom-left.
    gl.blitFramebuffer(0, 0, s.w, s.h, 0, s.h, s.w, 0, gl.COLOR_BUFFER_BIT, gl.NEAREST);
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, null);
  };

  const imports = {
    gpu_api: () => 2,
    gl_info(out, cap) {
      const P = (p) => gl.getParameter(p);
      const features = (ext.colorFloat ? 1 : 0) | (ext.floatLinear ? 2 : 0) | (ext.floatBlend ? 4 : 0)
        | (ext.indexed ? 8 : 0);
      const aniso = ext.aniso ? P(ext.aniso.MAX_TEXTURE_MAX_ANISOTROPY_EXT) : 1;
      f32[0] = aniso;
      const v = [P(gl.MAX_TEXTURE_SIZE), P(gl.MAX_ARRAY_TEXTURE_LAYERS), P(gl.MAX_UNIFORM_BLOCK_SIZE),
        P(gl.UNIFORM_BUFFER_OFFSET_ALIGNMENT), scratchUnit, P(gl.MAX_TEXTURE_IMAGE_UNITS),
        P(gl.MAX_VERTEX_TEXTURE_IMAGE_UNITS), P(gl.MAX_DRAW_BUFFERS), P(gl.MAX_VERTEX_ATTRIBS),
        P(gl.MAX_UNIFORM_BUFFER_BINDINGS), features, i32[0], 0, 0, 0, 0];
      const n = Math.min(v.length, cap >>> 0);
      u32(out, n).set(v.slice(0, n));
      return n;
    },
    gl_renderer_name: (out, cap) => writeStr(gpu.adapterName, out, cap),
    gl_last_error: (out, cap) => writeStr(lastError, out, cap),
    gl_error_count: () => errors.length,
    gl_error: (i, out, cap) => writeStr(errors[i >>> 0] || '', out, cap),
    gl_canvas(out) {
      if (!canvas) return 0;
      ensureSurface();
      u32(out, 2).set([canvas.width, canvas.height]);
      return 1;
    },
    gl_create_buffer: guard('createBuffer', (size, kinds) => {
      const b = { size, index: !!(kinds & BUF_INDEX), gl: null, shadow: null, tex: null, host: null };
      if (kinds & (BUF_DATA | BUF_INDEX)) {
        b.gl = gl.createBuffer();
        bindForWrite(b);
        gl.bufferData(bufferTarget(b), size, gl.DYNAMIC_DRAW);
      }
      if (kinds & BUF_STORAGE) {
        const rows = Math.max(1, Math.ceil(size / ROW_BYTES));
        b.shadow = new Uint8Array(rows * ROW_BYTES);
        b.tex = gl.createTexture();
        gl.activeTexture(gl.TEXTURE0 + scratchUnit);
        gl.bindTexture(gl.TEXTURE_2D, b.tex);
        gl.texStorage2D(gl.TEXTURE_2D, 1, gl.RGBA32UI, ROW_TEXELS, rows);
        uploadRows(b, 0, rows * ROW_BYTES);
      }
      if (kinds & BUF_READBACK) b.host = new Uint8Array(size);
      return put(b);
    }),
    gl_write_buffer(h, offset, ptr, len) {
      writeBytes(objects[h >>> 0], offset, u8(ptr, len));
    },
    gl_read_buffer: guard('readBuffer', (h, out, len) => {
      const b = objects[h >>> 0];
      u8(out, len).set(b.host.subarray(0, len));
      return 1;
    }),
    gl_create_texture: guard('createTexture', (w, h, layers, mips, format, array) =>
      put(makeTexture(w, h, layers, mips, FORMATS[format], !!array))),
    gl_write_texture(h, mip, layer, w, hgt, ptr, len) {
      const t = objects[h >>> 0];
      const fm = t.f;
      let data = u8(ptr, len).slice();
      if (fm.bgra) for (let k = 0; k < data.length; k += 4) { const r = data[k]; data[k] = data[k + 2]; data[k + 2] = r; }
      if (fm.type === gl.HALF_FLOAT) data = new Uint16Array(data.buffer);
      else if (fm.type === gl.FLOAT) data = new Float32Array(data.buffer);
      bindScratch(t);
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
      if (t.target === gl.TEXTURE_2D_ARRAY) gl.texSubImage3D(t.target, mip, 0, 0, layer, w, hgt, 1, fm.format, fm.type, data);
      else gl.texSubImage2D(t.target, mip, 0, 0, w, hgt, fm.format, fm.type, data);
    },
    gl_generate_mips(h) {
      const t = objects[h >>> 0];
      bindScratch(t);
      setLevels(t, 0, t.mips);
      gl.generateMipmap(t.target);
    },
    gl_create_sampler: guard('createSampler', (ptr, words) => {
      const d = u32(ptr, words);
      const lod = new Float32Array(memory.buffer, (ptr >>> 0) + 20, 1)[0];
      const s = gl.createSampler();
      const lin = d[0] !== 0;
      gl.samplerParameteri(s, gl.TEXTURE_MAG_FILTER, lin ? gl.LINEAR : gl.NEAREST);
      gl.samplerParameteri(s, gl.TEXTURE_MIN_FILTER, lin
        ? (d[1] ? gl.LINEAR_MIPMAP_LINEAR : gl.LINEAR_MIPMAP_NEAREST)
        : (d[1] ? gl.NEAREST_MIPMAP_LINEAR : gl.NEAREST_MIPMAP_NEAREST));
      for (const w of [gl.TEXTURE_WRAP_S, gl.TEXTURE_WRAP_T, gl.TEXTURE_WRAP_R]) gl.samplerParameteri(s, w, WRAP[d[2]]);
      gl.samplerParameterf(s, gl.TEXTURE_MAX_LOD, lod);
      if (d[4]) {
        gl.samplerParameteri(s, gl.TEXTURE_COMPARE_MODE, gl.COMPARE_REF_TO_TEXTURE);
        gl.samplerParameteri(s, gl.TEXTURE_COMPARE_FUNC, COMPARES[d[4]]);
      }
      if (ext.aniso && d[3] > 1) gl.samplerParameterf(s, ext.aniso.TEXTURE_MAX_ANISOTROPY_EXT, d[3]);
      return put(s);
    }),
    gl_create_program: guard('createProgram', (vp, vl, fp, fl, mp, ml) => {
      const program = link(str(vp, vl), str(fp, fl));
      const hidden = [];
      gl.useProgram(program);
      for (const line of str(mp, ml).split('\n')) {
        const [kind, name, a, b] = line.split(' ');
        if (kind === 'b') {
          const index = gl.getUniformBlockIndex(program, name);
          if (index !== gl.INVALID_INDEX) gl.uniformBlockBinding(program, index, Number(a));
        } else if (kind === 'u') {
          const loc = gl.getUniformLocation(program, name);
          if (loc) gl.uniform1i(loc, Number(a));
        } else if (kind === 'h') {
          hidden[Number(a)] = { loc: gl.getUniformLocation(program, name), uint: b === 'uint' };
        }
      }
      gl.useProgram(currentProgram);
      return put({ program, hidden });
    }),
    gl_create_pipeline: guard('createPipeline', (program, ptr, words) => {
      const w = u32(ptr, words);
      const fl = new Float32Array(w.slice().buffer);
      const targets = [];
      let k = 9;
      for (let t = 0; t < w[8]; t++, k += 2) targets.push({ blend: w[k], mask: w[k + 1] });
      const stride = w[k]; const count = w[k + 1];
      const attrs = [];
      for (let a = 0; a < count; a++) attrs.push([w[k + 2 + a * 3], w[k + 3 + a * 3], w[k + 4 + a * 3]]);
      const written = targets.filter((t) => t.mask !== 0);
      const uniform = written.every((t) => t.blend === written[0].blend && t.mask === written[0].mask);
      return put({
        program: objects[program], mode: MODES[w[0]], cull: w[1], frontCcw: w[2] !== 0,
        depthTest: w[3] !== 0, depthFunc: COMPARES[w[4]], depthWrite: w[5] !== 0, bias: fl[6], slope: fl[7],
        targets, drawBuffers: targets.map((t) => (t.mask ? 1 : 0)).join(''),
        blend: written.length ? written[0].blend : 0, mask: written.length ? written[0].mask : 15,
        indexed: !uniform, stride, attrs,
      });
    }),
    gl_create_vao: guard('createVertexArray', (pipeline, buffer, offset, index) => {
      const p = objects[pipeline];
      const vao = gl.createVertexArray();
      gl.bindVertexArray(vao);
      if (buffer) {
        gl.bindBuffer(gl.ARRAY_BUFFER, objects[buffer].gl);
        for (const [loc, fmt, off] of p.attrs) {
          gl.enableVertexAttribArray(loc);
          if (fmt <= 4) gl.vertexAttribPointer(loc, fmt, gl.FLOAT, false, p.stride, offset + off);
          else if (fmt === 5) gl.vertexAttribIPointer(loc, 1, gl.UNSIGNED_INT, p.stride, offset + off);
          else gl.vertexAttribPointer(loc, 4, gl.UNSIGNED_BYTE, true, p.stride, offset + off);
        }
      }
      if (index) gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, objects[index].gl);
      gl.bindVertexArray(null);
      currentVao = null;
      return put({ vao });
    }),
    gl_submit: guard('submit', (cmdPtr, words, pushPtr, pushLen, isPresent) => {
      execute(cmdPtr, words, pushPtr, pushLen);
      if (isPresent) present();
      glErrors(isPresent ? 'present' : 'submit');
      return 1;
    }),
    gl_release(h) {
      h >>>= 0;
      const o = objects[h];
      if (!o) return;
      if (o.tex && o.target) dropTexture(o);
      else if (o.vao) gl.deleteVertexArray(o.vao);
      else if (o.program && o.hidden) gl.deleteProgram(o.program);
      else if (o.mode !== undefined) { /* a pipeline: its program is released on its own */ } else if ('shadow' in o) {
        if (o.gl) gl.deleteBuffer(o.gl);
        if (o.tex) gl.deleteTexture(o.tex);
      } else if (o instanceof WebGLSampler) gl.deleteSampler(o);
      objects[h] = null;
      freeList.push(h);
    },
    log: (level, ptr, len) => onLog(level, str(ptr, len)),
    status: (ptr, len) => onStatus(str(ptr, len)),
  };
  // The WebGPU imports exist in the module too; on this host they are never reached.
  const unavailable = () => { throw new Error('WebGPU call on the WebGL 2 host'); };
  for (const name of GPU_IMPORTS) imports[name] = unavailable;

  return {
    imports: { abe: imports },
    errors,
    attach(instance) { memory = instance.exports.memory; },
    nextFrame() {},
    frameTexture() { return surface; },
    /** The canvas texture's pixels as RGBA rows, top row first. */
    readSurface() {
      const s = surface;
      if (!s) return null;
      gl.bindFramebuffer(gl.READ_FRAMEBUFFER, readFbo);
      gl.framebufferTexture2D(gl.READ_FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, s.tex, 0);
      gl.readBuffer(gl.COLOR_ATTACHMENT0);
      gl.pixelStorei(gl.PACK_ALIGNMENT, 1);
      const rgba = new Uint8ClampedArray(s.w * s.h * 4);
      gl.readPixels(0, 0, s.w, s.h, gl.RGBA, gl.UNSIGNED_BYTE, rgba);
      gl.bindFramebuffer(gl.READ_FRAMEBUFFER, null);
      for (let k = 3; k < rgba.length; k += 4) rgba[k] = 255;
      return { width: s.w, height: s.h, rgba };
    },
  };
}

const GPU_IMPORTS = ['gpu_info', 'gpu_adapter_name', 'gpu_last_error', 'gpu_error_count', 'gpu_error',
  'gpu_canvas', 'gpu_create_buffer', 'gpu_write_buffer', 'gpu_create_texture', 'gpu_write_texture',
  'gpu_generate_mips', 'gpu_create_sampler', 'gpu_create_shader', 'gpu_create_bind_group_layout',
  'gpu_create_pipeline_layout', 'gpu_create_render_pipeline', 'gpu_create_compute_pipeline',
  'gpu_create_bind_group', 'gpu_submit', 'gpu_release'];
const GL_IMPORTS = ['gl_info', 'gl_renderer_name', 'gl_last_error', 'gl_error_count', 'gl_error',
  'gl_canvas', 'gl_create_buffer', 'gl_write_buffer', 'gl_read_buffer', 'gl_create_texture',
  'gl_write_texture', 'gl_generate_mips', 'gl_create_sampler', 'gl_create_program',
  'gl_create_pipeline', 'gl_create_vao', 'gl_submit', 'gl_release'];

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
  // WebGPU when the browser has it (unless `api` is 'webgl2'), otherwise WebGL 2.
  const api = options.api || 'auto';
  let gpu = null;
  if (api !== 'webgl2' && typeof navigator !== 'undefined' && navigator.gpu) {
    onPhase('Starting WebGPU', 0);
    try {
      gpu = await createGpu(canvas);
    } catch (e) {
      if (api === 'webgpu') throw e;
      onLog(1, `WebGPU unavailable (${e.message}); using WebGL 2`);
    }
  } else if (api === 'webgpu') {
    throw new Error('This browser has no WebGPU. Use Chrome or Edge 113+ or Safari 26+, ' +
      'served over https or from localhost.');
  }
  if (!gpu) {
    onPhase('Starting WebGL 2', 0);
    gpu = createGl(canvas);
  }
  onLog(0, `${gpu.adapterName}; features: ${gpu.features.join(', ') || 'none'}; canvas ${gpu.format}`);
  const hostOptions = { onLog, onStatus: options.onStatus || (() => {}), onError: fail };
  const host = gpu.api === 'webgl2' ? createGlHost(gpu, hostOptions) : createHost(gpu, hostOptions);
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
async function readPixels(gpu, texture, host) {
  if (gpu.api === 'webgl2') return host.readSurface();
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
    readPixels(gpu, texture, host).then(resolve, reject);
  }));
  // Let asynchronous WebGPU errors arrive.
  if (gpu.device) await gpu.device.queue.onSubmittedWorkDone();
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
    try {
      canvas.setPointerCapture(e.pointerId);
    } catch (_) {
      // Not an active pointer (for example a synthetic event); drags still work while over the canvas.
    }
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
  const capture = (texture) => readPixels(gpu, texture, host);
  // At most two frames in flight: without vsync (or on a slow GPU) frames would otherwise queue
  // up without bound and input, readbacks and resizes would wait behind them.
  let inFlight = 0;
  const tick = (now) => {
    if (!running) return;
    if (inFlight >= 2) {
      requestAnimationFrame(tick);
      return;
    }
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
    if (gpu.device) {
      inFlight++;
      gpu.device.queue.onSubmittedWorkDone().then(() => { inFlight--; }, () => { inFlight--; });
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

// ------------------------------------------------------------------------------ Library mode

/** RGBA8 pixels of an image source: `{width, height, data}` passes through; canvases, image
 * bitmaps and images are drawn into a 2D canvas and read back (straight alpha). */
function pixelsOf(source) {
  if (source && source.data && source.width && source.height) return source;
  const width = source.naturalWidth || source.videoWidth || source.width;
  const height = source.naturalHeight || source.videoHeight || source.height;
  if (!width || !height) throw new Error('createTexture: the source has no size');
  const c = typeof OffscreenCanvas !== 'undefined'
    ? new OffscreenCanvas(width, height)
    : Object.assign(document.createElement('canvas'), { width, height });
  const ctx = c.getContext('2d', { willReadFrequently: true });
  ctx.clearRect(0, 0, width, height);
  ctx.drawImage(source, 0, 0, width, height);
  return { width, height, data: ctx.getImageData(0, 0, width, height).data };
}

/**
 * Library mode: the engine as a renderer that a host page (a game with its own interface)
 * drives. Resolves with an engine object; its methods throw with the engine's message when a
 * call fails. Handles (textures, meshes, materials, instances) are nonzero numbers.
 *
 * `options`: canvas; wasm (URL of abe_web.wasm); api ('auto', 'webgpu' or 'webgl2');
 * world (URL, ArrayBuffer or Uint8Array of a cooked `.abew`; omit for a scene of props only);
 * look (Look file as an object or JSON text; optional); quality ('realtime' or 'still');
 * onLog(level, text); onPhase(text, progress).
 *
 * The page sizes the canvas (canvas.width/height in device pixels; `fitCanvas(dpr)` helps) and
 * calls `render()` when it wants a frame. Positions are metres; matrices are 16 numbers,
 * column-major, model to world.
 */
export async function createEngine(options) {
  const { canvas } = options;
  let lastError = '';
  const onLog = options.onLog || (() => {});
  const { gpu, host, x, errors } = await boot({
    ...options,
    onLog: (level, text) => {
      if (level >= 2) lastError = text;
      onLog(level, text);
    },
    onError: (text) => {
      lastError = text;
      onLog(2, text);
    },
  });
  const fail = (what) => new Error(`${what}: ${lastError || 'the engine refused'}`);
  const ok = (what, r) => {
    if (!r) throw fail(what);
    return r;
  };

  // One scratch allocation, reused: the engine reads call arguments during the call only.
  // Its start is rounded up to 4 bytes so typed-array views of it are aligned.
  let scratch = 0;
  let scratchCap = 0;
  const reserve = (len) => {
    if (len + 4 > scratchCap) {
      if (scratch) x.abe_free(scratch, scratchCap);
      scratchCap = Math.max(len + 4, scratchCap * 2, 4096);
      scratch = x.abe_alloc(scratchCap) >>> 0;
    }
    return (scratch + 3) & ~3;
  };
  const encoder = new TextEncoder();
  /** Copies byte arrays into scratch memory, each at a 4-byte boundary; returns pointers. */
  const place = (...arrays) => {
    let total = 0;
    const offsets = arrays.map((a) => {
      const at = total;
      total += a ? (a.byteLength + 3) & ~3 : 0;
      return at;
    });
    const base = reserve(Math.max(total, 4));
    const mem = new Uint8Array(x.memory.buffer);
    return arrays.map((a, i) => {
      if (!a) return 0;
      mem.set(new Uint8Array(a.buffer, a.byteOffset, a.byteLength), base + offsets[i]);
      return base + offsets[i];
    });
  };
  const text = (s) => encoder.encode(s);
  const f32 = (a) => (a instanceof Float32Array ? a : new Float32Array(a));
  const out = (n) => new Float32Array(x.memory.buffer, reserve(n * 4), n);

  // The world and the Look.
  let world = { ptr: 0, len: 0 };
  if (typeof options.world === 'string') {
    world = await fetchInto(options.world, x, (p, bytes) =>
      (options.onPhase || (() => {}))(`Downloading the world (${(bytes / 1048576).toFixed(1)} MB)`, p));
  } else if (options.world) {
    const bytes = options.world instanceof Uint8Array ? options.world : new Uint8Array(options.world);
    const ptr = x.abe_alloc(bytes.length) >>> 0;
    new Uint8Array(x.memory.buffer, ptr, bytes.length).set(bytes);
    world = { ptr, len: bytes.length };
  }
  const lookText = options.look == null ? ''
    : (typeof options.look === 'string' ? options.look : JSON.stringify(options.look));
  const lookBytes = text(lookText);
  const [lookPtr] = place(lookBytes);
  (options.onPhase || (() => {}))('Compiling shaders and uploading', 1);
  ok('start', x.abe_rt_start(world.ptr, world.len, lookPtr, lookBytes.length,
    options.quality === 'still' ? 1 : 0));

  let inFlight = 0;
  let stopped = false;
  const size = () => {
    const p = reserve(8);
    ok('size', x.abe_rt_size(p));
    const s = new Uint32Array(x.memory.buffer, p, 2);
    return { width: s[0], height: s[1] };
  };
  /** CSS pixels per rendered pixel along x and y. */
  const cssScale = () => {
    const s = size();
    return { sx: (canvas.clientWidth || s.width) / s.width, sy: (canvas.clientHeight || s.height) / s.height };
  };

  return {
    api: gpu.api,
    adapter: gpu.adapterName,
    errors: () => [...errors, ...host.errors.filter((e) => !errors.includes(e))],
    /** Sizes the canvas to its CSS box times `dpr` (default: the device pixel ratio). */
    fitCanvas(dpr = window.devicePixelRatio || 1) {
      const w = Math.max(1, Math.round(canvas.clientWidth * dpr));
      const h = Math.max(1, Math.round(canvas.clientHeight * dpr));
      if (canvas.width !== w) canvas.width = w;
      if (canvas.height !== h) canvas.height = h;
    },
    /** The size the last frame was drawn at, in device pixels. */
    size,
    setLook(look) {
      const t = text(typeof look === 'string' ? look : JSON.stringify(look));
      const [p] = place(t);
      ok('setLook', x.abe_rt_set_look(p, t.length));
    },
    /** `{eye, target, up = [0, 1, 0], fov = 50, shift = [0, 0], near = 0.08}`. */
    setView(v) {
      const up = v.up || [0, 1, 0];
      const shift = v.shift || [0, 0];
      const [p] = place(f32([...v.eye, ...v.target, ...up, v.fov ?? 50, shift[0], shift[1],
        v.near ?? 0.08]));
      ok('setView', x.abe_rt_set_view(p));
    },
    /** Time of day in hours and the animation clock in seconds. */
    setClock(hours, seconds = 0) { ok('setClock', x.abe_rt_set_clock(hours, seconds)); },
    setWeather(preset) {
      const t = text(preset);
      const [p] = place(t);
      ok('setWeather', x.abe_rt_set_weather(p, t.length));
    },
    /** Draws and presents a frame. Returns false when it skipped (GPU busy, nothing to draw). */
    render() {
      if (stopped) return false;
      if (gpu.device && inFlight >= 2) return false;
      host.nextFrame();
      const r = x.abe_rt_render();
      if (!r) throw fail('render');
      if (r !== 1) return false;
      if (gpu.device) {
        inFlight++;
        gpu.device.queue.onSubmittedWorkDone().then(() => { inFlight--; }, () => { inFlight--; });
      }
      return true;
    },
    frames: () => x.abe_rt_frames(),
    /** The last frame's pixels (RGBA rows, top first); for tests. */
    async capture() {
      const texture = host.frameTexture();
      if (!texture) throw new Error('capture: no frame was rendered');
      return readPixels(gpu, texture, host);
    },
    /** Waits until the GPU has finished submitted work (WebGPU; immediate on WebGL 2). */
    async idle() { if (gpu.device) await gpu.device.queue.onSubmittedWorkDone(); },

    /** `source`: {width, height, data (RGBA8)} or a canvas, ImageBitmap or loaded image. */
    createTexture(source, { srgb = true } = {}) {
      const { width, height, data } = pixelsOf(source);
      const [p] = place(data);
      return ok('createTexture', x.abe_rt_texture_create(width, height, p, data.byteLength, srgb ? 1 : 0));
    },
    updateTexture(texture, source) {
      const { data } = pixelsOf(source);
      const [p] = place(data);
      ok('updateTexture', x.abe_rt_texture_update(texture, p, data.byteLength));
    },
    destroyTexture(texture) { x.abe_rt_texture_destroy(texture); },
    /** `{positions, normals?, uvs?, indices}` as flat arrays (3, 3 and 2 numbers per vertex). */
    createMesh(m) {
      const positions = f32(m.positions);
      const normals = m.normals && m.normals.length ? f32(m.normals) : null;
      const uvs = m.uvs && m.uvs.length ? f32(m.uvs) : null;
      const indices = m.indices instanceof Uint32Array ? m.indices : new Uint32Array(m.indices);
      const [p, n, u, i] = place(positions, normals, uvs, indices);
      return ok('createMesh', x.abe_rt_mesh_create(p, positions.length / 3, n, u, i, indices.length));
    },
    /** New `{positions, normals?}` for a mesh (same vertex count). */
    updateMesh(mesh, m) {
      const positions = f32(m.positions);
      const normals = m.normals && m.normals.length ? f32(m.normals) : null;
      const [p, n] = place(positions, normals);
      ok('updateMesh', x.abe_rt_mesh_update(mesh, p, n, positions.length / 3));
    },
    destroyMesh(mesh) { x.abe_rt_mesh_destroy(mesh); },
    /** See `MaterialDesc::from_json` (name, color, emissive, lamp, roughness, metallic,
     * shading, alphaCutoff, baseTexture, normalTexture, emissiveTexture, normalScale,
     * castsShadow). */
    createMaterial(desc) {
      const t = text(JSON.stringify(desc));
      const [p] = place(t);
      return ok('createMaterial', x.abe_rt_material_create(p, t.length));
    },
    updateMaterial(material, desc) {
      const t = text(JSON.stringify(desc));
      const [p] = place(t);
      ok('updateMaterial', x.abe_rt_material_update(material, p, t.length));
    },
    destroyMaterial(material) { x.abe_rt_material_destroy(material); },
    createInstance(mesh, material, matrix) {
      const [p] = place(f32(matrix));
      return ok('createInstance', x.abe_rt_instance_create(mesh, material, p));
    },
    setInstance(instance, matrix, visible = true) {
      const [p] = place(f32(matrix));
      ok('setInstance', x.abe_rt_instance_set(instance, p, visible ? 1 : 0));
    },
    /** Many at once: `[{instance, matrix, visible = true}]`. */
    setInstances(list) {
      const words = new ArrayBuffer(list.length * 72);
      const u = new Uint32Array(words);
      const f = new Float32Array(words);
      list.forEach((r, k) => {
        u[k * 18] = r.instance;
        u[k * 18 + 1] = r.visible === false ? 0 : 1;
        f.set(r.matrix, k * 18 + 2);
      });
      const [p] = place(new Uint8Array(words));
      ok('setInstances', x.abe_rt_instances_set(p, list.length));
    },
    setInstanceMaterial(instance, material) {
      ok('setInstanceMaterial', x.abe_rt_instance_material(instance, material));
    },
    destroyInstance(instance) { x.abe_rt_instance_destroy(instance); },
    /** Where a world point lands, in CSS pixels of the canvas (and depth in metres), or null
     * behind the camera. */
    project(point) {
      const { sx, sy } = cssScale();
      const p = out(3).byteOffset;
      if (!x.abe_rt_project(point[0], point[1], point[2], p)) return null;
      const [px, py, depth] = new Float32Array(x.memory.buffer, p, 3);
      return { x: px * sx, y: py * sy, depth };
    },
    /** The nearest visible prop under a CSS pixel of the canvas: `{instance, distance, point}`
     * or null. */
    pick(cssX, cssY) {
      const { sx, sy } = cssScale();
      const p = out(4).byteOffset;
      const instance = x.abe_rt_pick(cssX / sx, cssY / sy, p);
      if (!instance) return null;
      const [distance, px, py, pz] = new Float32Array(x.memory.buffer, p, 4);
      return { instance, distance, point: [px, py, pz] };
    },
    /** Releases the engine's GPU objects. */
    destroy() {
      if (stopped) return;
      stopped = true;
      x.abe_rt_stop();
      if (scratch) x.abe_free(scratch, scratchCap);
      scratch = 0;
      scratchCap = 0;
    },
  };
}
