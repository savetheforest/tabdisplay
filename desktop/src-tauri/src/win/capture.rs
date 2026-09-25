use windows::core::{Interface, Result};
use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::UI::WindowsAndMessaging::{GetCursorInfo, CURSORINFO, CURSOR_SHOWING};

/// Desktop Duplication of one monitor, copied to CPU as tightly packed BGRA, with the mouse cursor drawn in.
pub struct Capture {
    dup: IDXGIOutputDuplication,
    ctx: ID3D11DeviceContext,
    staging: ID3D11Texture2D,
    /// Last desktop image without the cursor, so the cursor can move without a new desktop frame.
    clean: Vec<u8>,
    cursor: Cursor,
    pub width: usize,
    pub height: usize,
    /// Monitor rect in desktop coordinates: left, top, right, bottom.
    pub rect: (i32, i32, i32, i32),
}

impl Capture {
    /// Duplicates the monitor with GDI name `device` (e.g. `\\.\DISPLAY5`), or the primary one if None.
    pub fn open(device: Option<&str>) -> Result<Self> {
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
            let r = output.GetDesc()?.DesktopCoordinates;
            let (width, height) = ((r.right - r.left) as usize, (r.bottom - r.top) as usize);
            let dup = output.DuplicateOutput(&device)?;

            let desc = D3D11_TEXTURE2D_DESC {
                Width: width as u32,
                Height: height as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_STAGING,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                ..Default::default()
            };
            let mut staging = None;
            device.CreateTexture2D(&desc, None, Some(&mut staging))?;

            Ok(Self {
                dup,
                ctx: ctx.unwrap(),
                staging: staging.unwrap(),
                clean: Vec::new(),
                cursor: Cursor::default(),
                width,
                height,
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
            if image {
                let tex: ID3D11Texture2D = res.unwrap().cast()?;
                self.ctx.CopyResource(&self.staging, &tex);
            }
            if info.PointerShapeBufferSize > 0 {
                // Only valid until ReleaseFrame.
                let c = &mut self.cursor;
                c.shape.resize(info.PointerShapeBufferSize as usize, 0);
                let mut needed = 0;
                self.dup.GetFramePointerShape(c.shape.len() as u32, c.shape.as_mut_ptr().cast(), &mut needed, &mut c.info)?;
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

            if image {
                let mut map = D3D11_MAPPED_SUBRESOURCE::default();
                self.ctx.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut map))?;
                let row = self.width * 4;
                self.clean.resize(row * self.height, 0);
                let src = std::slice::from_raw_parts(map.pData as *const u8, map.RowPitch as usize * self.height);
                for (y, dst) in self.clean.chunks_exact_mut(row).enumerate() {
                    let s = y * map.RowPitch as usize;
                    dst.copy_from_slice(&src[s..s + row]);
                }
                self.ctx.Unmap(&self.staging, 0);
            }
            buf.clone_from(&self.clean);
            if self.cursor.visible {
                self.cursor.draw(buf, self.width, self.height);
            }
            Ok(true)
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
        let mut ci = CURSORINFO { cbSize: size_of::<CURSORINFO>() as u32, ..Default::default() };
        let ok = unsafe { GetCursorInfo(&mut ci) }.is_ok();
        let p = ci.ptScreenPos;
        let visible = ok && ci.flags == CURSOR_SHOWING && (l..r).contains(&p.x) && (t..b).contains(&p.y);
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
        let (sw, sh, pitch) = (i.Width as i32, if mono { i.Height / 2 } else { i.Height } as i32, i.Pitch as usize);
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
                let name = String::from_utf16_lossy(&n[..n.iter().position(|&c| c == 0).unwrap_or(n.len())]);
                all.push((adapter.clone(), output.cast()?, name, desc.DesktopCoordinates));
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
    outputs()?
        .into_iter()
        .find(|(_, _, name, _)| device.is_none_or(|d| d == name))
        .map(|(a, o, _, _)| (a, o))
        .ok_or_else(|| windows::core::Error::new(DXGI_ERROR_NOT_FOUND, "monitor não encontrado"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor(kind: DXGI_OUTDUPL_POINTER_SHAPE_TYPE, w: u32, h: u32, pitch: u32, shape: Vec<u8>, x: i32, y: i32) -> Cursor {
        let info = DXGI_OUTDUPL_POINTER_SHAPE_INFO { Type: kind.0 as u32, Width: w, Height: h, Pitch: pitch, ..Default::default() };
        Cursor { visible: true, x, y, shape, info }
    }

    #[test]
    fn draws_color_cursor_with_alpha_and_clipping() {
        let mut frame = vec![100u8; 2 * 2 * 4];
        // 2x1 cursor: opaque red, fully transparent. Placed at (1, 1) so its second pixel is off-frame.
        let c = cursor(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR, 2, 1, 8, vec![0, 0, 255, 255, 9, 9, 9, 0], 1, 1);
        c.draw(&mut frame, 2, 2);
        assert_eq!(&frame[12..15], &[0, 0, 255]);
        assert_eq!(&frame[..12], &[100; 12]);
    }

    #[test]
    fn draws_monochrome_cursor() {
        let mut frame = vec![100u8; 2 * 4];
        // 2x1 cursor: AND row 0b01.., XOR row 0b11.. -> pixel 0 = 0^FF (white), pixel 1 = 100^FF (inverted).
        let c = cursor(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME, 2, 2, 1, vec![0b0100_0000, 0b1100_0000], 0, 0);
        c.draw(&mut frame, 2, 1);
        assert_eq!(&frame[0..3], &[255; 3]);
        assert_eq!(&frame[4..7], &[155; 3]);
    }
}
