//! Presentation through the window's `CAMetalLayer`: `acquire` takes the layer's next drawable
//! and registers its texture as a swapchain texture; `present` encodes the frame, schedules
//! the drawable's presentation, commits and keeps at most two frames in flight.
use super::ffi;
use super::*;
use crate::apple::objc::CGSize;
use crate::gpu::{Encoder, TextureUsage};
use crate::window::Window;

pub(super) struct Surface<'w> {
    window: &'w Window,
    /// The window's CAMetalLayer (owned by the window).
    layer: Id,
    format: Format,
    size: (u32, u32),
    /// The acquired drawable and the texture id registered for it.
    drawable: Option<(Obj, TextureId)>,
}

impl<'w> Gpu<'w> {
    /// A device presenting to `window`'s Metal layer.
    pub fn for_window(window: &'w Window) -> Result<Self> {
        autoreleasepool(|| {
            let mut gpu = Self::with_device(Gpu::pick_device()?)?;
            let layer = window.metal_layer();
            if layer.is_null() {
                return Err("the window has no CAMetalLayer".into());
            }
            let format = Format::Bgra8Srgb;
            let size = window.size();
            // SAFETY: configuring the window's live CAMetalLayer on the main thread.
            unsafe {
                send!(layer, "setDevice:", gpu.device.id() => Id; ());
                send!(
                    layer,
                    "setPixelFormat:",
                    resources::pixel_format(format) as NSUInteger => NSUInteger;
                    ()
                );
                // Blits may write into the drawable, so it cannot be framebuffer-only.
                send!(layer, "setFramebufferOnly:", false => bool; ());
                send!(layer, "setMaximumDrawableCount:", 3usize => NSUInteger; ());
                let s = CGSize {
                    width: f64::from(size.0.max(1)),
                    height: f64::from(size.1.max(1)),
                };
                send!(layer, "setDrawableSize:", s => CGSize; ());
            }
            gpu.present = Some(Surface {
                window,
                layer,
                format,
                size,
                drawable: None,
            });
            Ok(gpu)
        })
    }

    pub fn surface_format(&self) -> Format {
        self.present
            .as_ref()
            .map_or(Format::Bgra8Srgb, |p| p.format)
    }

    pub fn surface_size(&self) -> (u32, u32) {
        self.present.as_ref().map_or((0, 0), |p| p.size)
    }

    pub fn acquire(&mut self) -> Result<Option<TextureId>> {
        let Some(surface) = self.present.as_mut() else {
            return Err("acquire: this device has no window to present to".into());
        };
        if let Some((_, id)) = &surface.drawable {
            return Ok(Some(*id));
        }
        if !surface.window.visible() {
            return Ok(None);
        }
        let size = surface.window.size();
        if size.0 == 0 || size.1 == 0 {
            return Ok(None);
        }
        let (layer, format) = (surface.layer, surface.format);
        let resized = size != surface.size;
        surface.size = size;
        autoreleasepool(|| {
            if resized {
                let s = CGSize {
                    width: f64::from(size.0),
                    height: f64::from(size.1),
                };
                // SAFETY: resizing the drawables of the live layer.
                unsafe { send!(layer, "setDrawableSize:", s => CGSize; ()) };
            }
            // SAFETY: nextDrawable returns an autoreleased drawable or nil after a timeout;
            // both the drawable and its texture are retained past this pool.
            let (drawable, texture) = unsafe {
                let Some(drawable) = Obj::retained(send!(layer, "nextDrawable"; Id)) else {
                    return Ok(None);
                };
                let texture = Obj::retained(send!(drawable.id(), "texture"; Id))
                    .ok_or("the drawable has no texture")?;
                (drawable, texture)
            };
            let desc = TextureDesc::new(
                size.0,
                size.1,
                format,
                TextureUsage::COLOR_TARGET | TextureUsage::COPY_DST,
            );
            let raw = self.textures.insert(Texture {
                obj: texture,
                desc,
                pixel_format: resources::pixel_format(format),
                ty: ffi::texture_type::D2,
                sw_decompress: None,
                views: Vec::new(),
                swapchain: true,
            })?;
            let id = TextureId::from_raw(raw);
            if let Some(s) = self.present.as_mut() {
                s.drawable = Some((drawable, id));
            }
            Ok(Some(id))
        })
    }

    pub fn present(&mut self, e: &Encoder) -> Result<()> {
        e.finish()?;
        let (drawable, id) = self
            .present
            .as_mut()
            .ok_or("present: this device has no window to present to")?
            .drawable
            .take()
            .ok_or("present: call acquire() first (it returned no texture)")?;
        let result = autoreleasepool(|| {
            self.collect()?;
            let cb = self.command_buffer()?;
            self.record(cb.id(), e, Some(id))?;
            // SAFETY: scheduling the drawable on the command buffer that renders into it.
            unsafe { send!(cb.id(), "presentDrawable:", drawable.id() => Id; ()) };
            let serial = self.commit(cb);
            self.mark_used(e, serial);
            self.frame_serials[self.frame] = serial;
            self.frame = (self.frame + 1) % FRAMES;
            // Two frames in flight: the frame that used the next slot must be done.
            self.wait_serial(self.frame_serials[self.frame])
        });
        // The command buffer retains the drawable until it is presented.
        autoreleasepool(|| {
            self.textures.remove(id.raw());
            drop(drawable);
        });
        result
    }
}
