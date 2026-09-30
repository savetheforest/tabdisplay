use windows::core::{Interface, Result, PCWSTR};
use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_PRIMARY_DEVICE,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorInfo, CURSORINFO, CURSOR_SHOWING};

/// Desktop Duplication of one monitor, copied to CPU as tightly packed BGRA, with the mouse cursor drawn in.
pub struct Capture {
    dup: IDXGIOutputDuplication,
    ctx: ID3D11DeviceContext,
    staging: ID3D11Texture2D,
    /// Last desktop image without the cursor, so the cursor can move without a new desktop frame.
    clean: Vec<u8>,
    cursor: Cursor,
    /// Native monitor size; `width`/`height` below are what `next` outputs (native, or downscaled to fit the tablet).
    native: (usize, usize),
    /// Scratch for the native frame + cursor when downscaling.
    full: Vec<u8>,
    /// Optional D3D11 video-processor resize path. It is only used after a capability check;
    /// a runtime device/driver error disables it for the rest of this capture and uses CPU resize.
    gpu_scaler: Option<GpuScaler>,
    pub width: usize,
    pub height: usize,
    /// Monitor rect in desktop coordinates: left, top, right, bottom.
    pub rect: (i32, i32, i32, i32),
}

impl Capture {
    /// Duplicates the monitor with GDI name `device` (e.g. `\\.\DISPLAY5`), or the primary one if None.
    /// A monitor bigger than `fit` allows (e.g. 4K mirrored to a tablet capped at 2304x1440) is
    /// resized by D3D11 VideoProcessor when the adapter advertises BGRA input/output support,
    /// otherwise the bounded CPU bilinear fallback is used.
    pub fn open(
        device: Option<&str>,
        fps: u32,
        fit: impl Fn((u32, u32)) -> (u32, u32),
    ) -> Result<Self> {
        unsafe {
            let (adapter, output) = find_output(device)?;
            let (mut device, mut ctx) = (None, None);
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut ctx),
            )?;
            let device: ID3D11Device = device.unwrap();
            let ctx: ID3D11DeviceContext = ctx.unwrap();
            let r = output.GetDesc()?.DesktopCoordinates;
            let (width, height) = ((r.right - r.left) as usize, (r.bottom - r.top) as usize);
            let dup = output.DuplicateOutput(&device)?;
            let (ow, oh) = fit((width as u32, height as u32));

            let desc = D3D11_TEXTURE2D_DESC {
                Width: width as u32,
                Height: height as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                ..Default::default()
            };
            let mut staging = None;
            device.CreateTexture2D(&desc, None, Some(&mut staging))?;

            let gpu_scaler = if (ow as usize, oh as usize) != (width, height) {
                match GpuScaler::new(
                    &device,
                    &ctx,
                    (width, height),
                    (ow as usize, oh as usize),
                    fps,
                ) {
                    Ok(scaler) => Some(scaler),
                    Err(e) => {
                        crate::telemetry::warn(format!(
                            "D3D11 video resize unavailable ({e}), using CPU resize"
                        ));
                        None
                    }
                }
            } else {
                None
            };

            Ok(Self {
                dup,
                ctx,
                staging: staging.unwrap(),
                clean: Vec::new(),
                cursor: Cursor::default(),
                native: (width, height),
                full: Vec::new(),
                gpu_scaler,
                width: ow as usize,
                height: oh as usize,
                rect: (r.left, r.top, r.right, r.bottom),
            })
        }
    }

    /// Waits up to `timeout_ms` for a screen or cursor update and writes the composed frame to `buf`.
    /// Returns false on timeout.
    pub fn next(&mut self, buf: &mut Vec<u8>, timeout_ms: u32) -> Result<bool> {
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut res = None;
            let acquired = match self.dup.AcquireNextFrame(timeout_ms, &mut info, &mut res) {
                Ok(()) => true,
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => false, // the cursor may still have moved
                Err(e) => return Err(e),
            };
            let image = info.LastPresentTime != 0;
            let pointer = info.LastMouseUpdateTime != 0;
            let scaled = (self.width, self.height) != self.native;
            let mut gpu_resized = false;
            if image {
                let tex: ID3D11Texture2D = res.unwrap().cast()?;
                if scaled {
                    if let Some(scaler) = self.gpu_scaler.as_mut() {
                        match scaler.resize(&tex, &mut self.clean) {
                            Ok(()) => gpu_resized = true,
                            Err(e) => {
                                crate::telemetry::warn(format!(
                                    "D3D11 video resize failed ({e}), reverting to CPU resize"
                                ));
                                self.gpu_scaler = None;
                            }
                        }
                    }
                }
                if !gpu_resized {
                    self.ctx.CopyResource(&self.staging, &tex);
                    let mut map = D3D11_MAPPED_SUBRESOURCE::default();
                    self.ctx
                        .Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut map))?;
                    let (nw, nh) = self.native;
                    let row = nw * 4;
                    self.clean.resize(row * nh, 0);
                    let src = std::slice::from_raw_parts(
                        map.pData as *const u8,
                        map.RowPitch as usize * nh,
                    );
                    for (y, dst) in self.clean.chunks_exact_mut(row).enumerate() {
                        let s = y * map.RowPitch as usize;
                        dst.copy_from_slice(&src[s..s + row]);
                    }
                    self.ctx.Unmap(&self.staging, 0);
                }
            }
            if info.PointerShapeBufferSize > 0 {
                // Only valid until ReleaseFrame.
                let c = &mut self.cursor;
                c.shape.resize(info.PointerShapeBufferSize as usize, 0);
                let mut needed = 0;
                self.dup.GetFramePointerShape(
                    c.shape.len() as u32,
                    c.shape.as_mut_ptr().cast(),
                    &mut needed,
                    &mut c.info,
                )?;
            }
            if acquired {
                self.dup.ReleaseFrame()?;
            }
            // Position/visibility come from the OS: with the virtual display driver's hardware cursor,
            // duplication reports the pointer as hidden right after it moves onto the monitor.
            let moved = self.cursor.update_position(self.rect);
            if !image && !pointer && !moved || !image && self.clean.is_empty() {
                return Ok(false);
            }

            let (nw, nh) = self.native;
            if scaled
                && self.gpu_scaler.is_some()
                && self.clean.len() == self.width * self.height * 4
            {
                buf.resize(self.width * self.height * 4, 0);
                buf.clone_from(&self.clean);
                if self.cursor.visible {
                    self.cursor
                        .draw_scaled(buf, nw, nh, self.width, self.height);
                }
                return Ok(true);
            } else if scaled {
                let frame = &mut self.full;
                frame.clone_from(&self.clean);
                if self.cursor.visible {
                    self.cursor.draw(frame, nw, nh);
                }
                buf.resize(self.width * self.height * 4, 0);
                downscale(&self.full, (nw, nh), buf, (self.width, self.height));
                return Ok(true);
            } else {
                let frame = &mut *buf;
                frame.clone_from(&self.clean);
                if self.cursor.visible {
                    self.cursor.draw(frame, nw, nh);
                }
                return Ok(true);
            };
        }
    }
}

/// D3D11 VideoProcessor resize bridge. The result is read back only because the current encoder
/// API accepts CPU BGRA. Keeping this boundary explicit prevents claiming a zero-copy encoder path.
struct GpuScaler {
    ctx: ID3D11DeviceContext,
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    processor: ID3D11VideoProcessor,
    enumerator: ID3D11VideoProcessorEnumerator,
    input_size: (usize, usize),
    output: ID3D11Texture2D,
    output_staging: ID3D11Texture2D,
    output_view: ID3D11VideoProcessorOutputView,
    size: (usize, usize),
}

impl GpuScaler {
    fn new(
        device: &ID3D11Device,
        ctx: &ID3D11DeviceContext,
        input: (usize, usize),
        output: (usize, usize),
        fps: u32,
    ) -> Result<Self> {
        unsafe {
            let video_device: ID3D11VideoDevice = device.cast()?;
            let video_context: ID3D11VideoContext = ctx.cast()?;
            let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                InputFrameRate: DXGI_RATIONAL {
                    Numerator: fps.max(1),
                    Denominator: 1,
                },
                InputWidth: input.0 as u32,
                InputHeight: input.1 as u32,
                OutputFrameRate: DXGI_RATIONAL {
                    Numerator: fps.max(1),
                    Denominator: 1,
                },
                OutputWidth: output.0 as u32,
                OutputHeight: output.1 as u32,
                Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
            };
            let enumerator = video_device.CreateVideoProcessorEnumerator(&desc)?;
            let support = enumerator.CheckVideoProcessorFormat(DXGI_FORMAT_B8G8R8A8_UNORM)?;
            let required = (D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT.0
                | D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0) as u32;
            if support & required != required {
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    "D3D11 BGRA video processor format is not supported",
                ));
            }
            let processor = video_device.CreateVideoProcessor(&enumerator, 0)?;
            let tex_desc = D3D11_TEXTURE2D_DESC {
                Width: output.0 as u32,
                Height: output.1 as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                ..Default::default()
            };
            let mut output_texture = None;
            device.CreateTexture2D(&tex_desc, None, Some(&mut output_texture))?;
            let output_texture = output_texture.unwrap();
            let output_resource: ID3D11Resource = output_texture.cast()?;
            let view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
                },
            };
            let mut output_view = None;
            video_device.CreateVideoProcessorOutputView(
                &output_resource,
                &enumerator,
                &view_desc,
                Some(&mut output_view as *mut _),
            )?;
            let output_view = output_view.ok_or_else(|| {
                windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    "D3D11 output view missing",
                )
            })?;
            let staging_desc = D3D11_TEXTURE2D_DESC {
                Usage: D3D11_USAGE_STAGING,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                BindFlags: 0,
                MiscFlags: 0,
                ..tex_desc
            };
            let mut output_staging = None;
            device.CreateTexture2D(&staging_desc, None, Some(&mut output_staging))?;
            Ok(Self {
                ctx: ctx.clone(),
                video_device,
                video_context,
                processor,
                enumerator,
                input_size: input,
                output: output_texture,
                output_staging: output_staging.unwrap(),
                output_view,
                size: output,
            })
        }
    }

    fn resize(&mut self, input: &ID3D11Texture2D, dst: &mut Vec<u8>) -> Result<()> {
        unsafe {
            let input_resource: ID3D11Resource = input.cast()?;
            let view_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPIV {
                        MipSlice: 0,
                        ArraySlice: 0,
                    },
                },
            };
            let mut input_view = None;
            self.video_device.CreateVideoProcessorInputView(
                &input_resource,
                &self.enumerator,
                &view_desc,
                Some(&mut input_view as *mut _),
            )?;
            let input_view = input_view.ok_or_else(|| {
                windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    "D3D11 input view missing",
                )
            })?;
            let source = RECT {
                left: 0,
                top: 0,
                right: self.input_size.0 as i32,
                bottom: self.input_size.1 as i32,
            };
            let target = RECT {
                left: 0,
                top: 0,
                right: self.size.0 as i32,
                bottom: self.size.1 as i32,
            };
            self.video_context.VideoProcessorSetStreamFrameFormat(
                &self.processor,
                0,
                D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            );
            self.video_context.VideoProcessorSetStreamSourceRect(
                &self.processor,
                0,
                true,
                Some(&source),
            );
            self.video_context.VideoProcessorSetStreamDestRect(
                &self.processor,
                0,
                true,
                Some(&target),
            );
            let stream = D3D11_VIDEO_PROCESSOR_STREAM {
                Enable: true.into(),
                pInputSurface: std::mem::ManuallyDrop::new(Some(input_view)),
                ..Default::default()
            };
            self.video_context.VideoProcessorBlt(
                &self.processor,
                &self.output_view,
                0,
                &[stream],
            )?;
            self.ctx.CopyResource(&self.output_staging, &self.output);
            let mut map = D3D11_MAPPED_SUBRESOURCE::default();
            self.ctx
                .Map(&self.output_staging, 0, D3D11_MAP_READ, 0, Some(&mut map))?;
            let row = self.size.0 * 4;
            dst.resize(row * self.size.1, 0);
            let src = std::slice::from_raw_parts(
                map.pData as *const u8,
                map.RowPitch as usize * self.size.1,
            );
            for (y, line) in dst.chunks_exact_mut(row).enumerate() {
                line.copy_from_slice(
                    &src[y * map.RowPitch as usize..y * map.RowPitch as usize + row],
                );
            }
            self.ctx.Unmap(&self.output_staging, 0);
            Ok(())
        }
    }
}

/// Bilinear resize of tightly packed BGRA `src` to `dst` (both sizes given as (w, h)).
fn downscale(src: &[u8], (sw, sh): (usize, usize), dst: &mut [u8], (dw, dh): (usize, usize)) {
    let (kx, ky) = (sw as f32 / dw as f32, sh as f32 / dh as f32);
    // Per output column: left source pixel index and right-neighbour weight (0..=256).
    let cols: Vec<(usize, usize, u32)> = (0..dw)
        .map(|x| {
            let f = ((x as f32 + 0.5) * kx - 0.5).clamp(0.0, (sw - 1) as f32);
            let x0 = f as usize;
            (
                x0 * 4,
                (x0 + 1).min(sw - 1) * 4,
                ((f - x0 as f32) * 256.0) as u32,
            )
        })
        .collect();
    for (y, out) in dst.chunks_exact_mut(dw * 4).enumerate().take(dh) {
        let f = ((y as f32 + 0.5) * ky - 0.5).clamp(0.0, (sh - 1) as f32);
        let y0 = f as usize;
        let wy = ((f - y0 as f32) * 256.0) as u32;
        let (r0, r1) = (
            &src[y0 * sw * 4..][..sw * 4],
            &src[(y0 + 1).min(sh - 1) * sw * 4..][..sw * 4],
        );
        for (px, &(a, b, wx)) in out.chunks_exact_mut(4).zip(&cols) {
            for c in 0..4 {
                let top = r0[a + c] as u32 * (256 - wx) + r0[b + c] as u32 * wx;
                let bot = r1[a + c] as u32 * (256 - wx) + r1[b + c] as u32 * wx;
                px[c] = ((top * (256 - wy) + bot * wy) >> 16) as u8;
            }
        }
    }
}

#[derive(Default)]
struct Cursor {
    visible: bool,
    /// Top-left of the cursor image, relative to the monitor.
    x: i32,
    y: i32,
    shape: Vec<u8>,
    info: DXGI_OUTDUPL_POINTER_SHAPE_INFO,
}

impl Cursor {
    /// Refreshes position and visibility for the monitor at `rect`. Returns true if either changed.
    fn update_position(&mut self, (l, t, r, b): (i32, i32, i32, i32)) -> bool {
        let mut ci = CURSORINFO {
            cbSize: size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        let ok = unsafe { GetCursorInfo(&mut ci) }.is_ok();
        let p = ci.ptScreenPos;
        let visible =
            ok && ci.flags == CURSOR_SHOWING && (l..r).contains(&p.x) && (t..b).contains(&p.y);
        let (x, y) = (p.x - l - self.info.HotSpot.x, p.y - t - self.info.HotSpot.y);
        let changed = visible != self.visible || (visible && (x, y) != (self.x, self.y));
        (self.visible, self.x, self.y) = (visible, x, y);
        changed
    }

    /// Draws the cursor into a BGRA frame, clipped to its bounds.
    fn draw(&self, frame: &mut [u8], w: usize, h: usize) {
        let i = &self.info;
        let mono = i.Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0 as u32;
        // Monochrome shapes stack an AND mask on top of an XOR mask, so the image is half as tall.
        let (sw, sh, pitch) = (
            i.Width as i32,
            if mono { i.Height / 2 } else { i.Height } as i32,
            i.Pitch as usize,
        );
        for cy in 0..sh {
            let fy = self.y + cy;
            if fy < 0 || fy >= h as i32 {
                continue;
            }
            for cx in 0..sw {
                let fx = self.x + cx;
                if fx < 0 || fx >= w as i32 {
                    continue;
                }
                let d = &mut frame[(fy as usize * w + fx as usize) * 4..][..3];
                let (row, col) = (cy as usize * pitch, cx as usize);
                if mono {
                    let bit = |r: usize| self.shape[r + col / 8] >> (7 - col % 8) & 1 == 1;
                    let (and, xor) = (bit(row), bit(row + sh as usize * pitch));
                    for c in d.iter_mut() {
                        *c = (if and { *c } else { 0 }) ^ (if xor { 0xFF } else { 0 });
                    }
                    continue;
                }
                let s = &self.shape[row + col * 4..][..4];
                if i.Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32 {
                    let a = s[3] as u32;
                    for k in 0..3 {
                        d[k] = ((s[k] as u32 * a + d[k] as u32 * (255 - a)) / 255) as u8;
                    }
                } else if s[3] == 0 {
                    d.copy_from_slice(&s[..3]); // masked color: replace
                } else {
                    for k in 0..3 {
                        d[k] ^= s[k]; // masked color: invert (e.g. the text I-beam)
                    }
                }
            }
        }
    }

    /// Draws a native cursor shape over a GPU-resized frame. The scaler runs before cursor
    /// composition so a cursor move does not require a second native readback.
    fn draw_scaled(&self, frame: &mut [u8], nw: usize, nh: usize, w: usize, h: usize) {
        let i = &self.info;
        let sx = w as f32 / nw as f32;
        let sy = h as f32 / nh as f32;
        let mono = i.Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0 as u32;
        let (sw, sh, pitch) = (
            i.Width as usize,
            if mono {
                i.Height as usize / 2
            } else {
                i.Height as usize
            },
            i.Pitch as usize,
        );
        let left = (self.x as f32 * sx).floor() as i32;
        let top = (self.y as f32 * sy).floor() as i32;
        let right = ((self.x + sw as i32) as f32 * sx).ceil() as i32;
        let bottom = ((self.y + sh as i32) as f32 * sy).ceil() as i32;
        for fy in top.max(0)..bottom.min(h as i32) {
            let cy = (((fy - top) as f32 / sy).floor() as usize).min(sh.saturating_sub(1));
            for fx in left.max(0)..right.min(w as i32) {
                let cx = (((fx - left) as f32 / sx).floor() as usize).min(sw.saturating_sub(1));
                let d = &mut frame[(fy as usize * w + fx as usize) * 4..][..3];
                let row = cy * pitch;
                if mono {
                    let bit = |r: usize| self.shape[r + cx / 8] >> (7 - cx % 8) & 1 == 1;
                    let (and, xor) = (bit(row), bit(row + sh * pitch));
                    for c in d.iter_mut() {
                        *c = (if and { *c } else { 0 }) ^ (if xor { 0xFF } else { 0 });
                    }
                } else {
                    let s = &self.shape[row + cx * 4..][..4];
                    if i.Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32 {
                        let a = s[3] as u32;
                        for k in 0..3 {
                            d[k] = ((s[k] as u32 * a + d[k] as u32 * (255 - a)) / 255) as u8;
                        }
                    } else if s[3] == 0 {
                        d.copy_from_slice(&s[..3]);
                    } else {
                        for k in 0..3 {
                            d[k] ^= s[k];
                        }
                    }
                }
            }
        }
    }
}

/// Every output on every adapter, with its GDI name and desktop rect. Primary first.
fn outputs() -> Result<Vec<(IDXGIAdapter1, IDXGIOutput1, String, RECT)>> {
    let mut all = Vec::new();
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            i += 1;
            let mut j = 0;
            while let Ok(output) = adapter.EnumOutputs(j) {
                j += 1;
                let desc = output.GetDesc()?;
                let n = &desc.DeviceName;
                let name = String::from_utf16_lossy(
                    &n[..n.iter().position(|&c| c == 0).unwrap_or(n.len())],
                );
                all.push((
                    adapter.clone(),
                    output.cast()?,
                    name,
                    desc.DesktopCoordinates,
                ));
            }
        }
    }
    Ok(all)
}

/// Monitors attached to the desktop: (GDI name, width, height).
pub fn monitors() -> Vec<(String, u32, u32)> {
    outputs()
        .unwrap_or_default()
        .into_iter()
        .map(|(_, _, name, r)| (name, (r.right - r.left) as u32, (r.bottom - r.top) as u32))
        .collect()
}

/// The primary output, or the one whose GDI name is `device`.
fn find_output(device: Option<&str>) -> Result<(IDXGIAdapter1, IDXGIOutput1)> {
    let wanted = device.map(str::to_owned).or_else(primary_device_name);
    outputs()?
        .into_iter()
        .find(|(_, _, name, _)| wanted.as_deref().is_none_or(|d| d == name))
        .map(|(a, o, _, _)| (a, o))
        .ok_or_else(|| windows::core::Error::new(DXGI_ERROR_NOT_FOUND, "monitor não encontrado"))
}

/// DXGI enumeration order is not a primary-monitor contract. Ask GDI for the
/// primary device first so `None` never silently means "the first adapter".
fn primary_device_name() -> Option<String> {
    unsafe {
        let mut device = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        let mut i = 0;
        while EnumDisplayDevicesW(PCWSTR::null(), i, &mut device, 0).as_bool() {
            i += 1;
            if device.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE == DISPLAY_DEVICE_PRIMARY_DEVICE {
                let end = device
                    .DeviceName
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(device.DeviceName.len());
                return Some(String::from_utf16_lossy(&device.DeviceName[..end]));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor(
        kind: DXGI_OUTDUPL_POINTER_SHAPE_TYPE,
        w: u32,
        h: u32,
        pitch: u32,
        shape: Vec<u8>,
        x: i32,
        y: i32,
    ) -> Cursor {
        let info = DXGI_OUTDUPL_POINTER_SHAPE_INFO {
            Type: kind.0 as u32,
            Width: w,
            Height: h,
            Pitch: pitch,
            ..Default::default()
        };
        Cursor {
            visible: true,
            x,
            y,
            shape,
            info,
        }
    }

    #[test]
    fn draws_color_cursor_with_alpha_and_clipping() {
        let mut frame = vec![100u8; 2 * 2 * 4];
        // 2x1 cursor: opaque red, fully transparent. Placed at (1, 1) so its second pixel is off-frame.
        let c = cursor(
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR,
            2,
            1,
            8,
            vec![0, 0, 255, 255, 9, 9, 9, 0],
            1,
            1,
        );
        c.draw(&mut frame, 2, 2);
        assert_eq!(&frame[12..15], &[0, 0, 255]);
        assert_eq!(&frame[..12], &[100; 12]);
    }

    #[test]
    fn downscale_averages_and_keeps_size() {
        // 4x2 -> 2x1: each output pixel blends its source neighbourhood; a flat image stays flat.
        let mut dst = vec![0u8; 2 * 4];
        downscale(&vec![80u8; 4 * 2 * 4], (4, 2), &mut dst, (2, 1));
        assert_eq!(dst, vec![80u8; 8]);
        // Black left half / white right half: left output is dark, right is bright.
        let mut src = vec![0u8; 4 * 2 * 4];
        for y in 0..2 {
            for x in 2..4 {
                src[(y * 4 + x) * 4..][..4].fill(255);
            }
        }
        downscale(&src, (4, 2), &mut dst, (2, 1));
        assert!(dst[0] < 100 && dst[4] > 150);
    }

    #[test]
    fn draws_monochrome_cursor() {
        let mut frame = vec![100u8; 2 * 4];
        // 2x1 cursor: AND row 0b01.., XOR row 0b11.. -> pixel 0 = 0^FF (white), pixel 1 = 100^FF (inverted).
        let c = cursor(
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME,
            2,
            2,
            1,
            vec![0b0100_0000, 0b1100_0000],
            0,
            0,
        );
        c.draw(&mut frame, 2, 1);
        assert_eq!(&frame[0..3], &[255; 3]);
        assert_eq!(&frame[4..7], &[155; 3]);
    }

    #[test]
    fn draws_scaled_cursor_over_gpu_resize_output() {
        let c = cursor(
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR,
            1,
            1,
            4,
            vec![0, 0, 255, 255],
            0,
            0,
        );
        let mut frame = vec![100u8; 2 * 2 * 4];
        c.draw_scaled(&mut frame, 1, 1, 2, 2);
        for pixel in frame.chunks_exact(4) {
            assert_eq!(&pixel[..3], &[0, 0, 255]);
        }
    }
}
