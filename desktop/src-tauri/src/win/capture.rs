use windows::core::{Interface, Result};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::*;

/// Desktop Duplication of one monitor, copied to CPU as tightly packed BGRA.
pub struct Capture {
    dup: IDXGIOutputDuplication,
    ctx: ID3D11DeviceContext,
    staging: ID3D11Texture2D,
    pub width: usize,
    pub height: usize,
    /// Monitor rect in desktop coordinates: left, top, right, bottom.
    pub rect: (i32, i32, i32, i32),
}

impl Capture {
    // ponytail: always output 0 of the first adapter (primary). Phase 2 picks the virtual display's output.
    pub fn primary() -> Result<Self> {
        unsafe {
            let (mut device, mut ctx) = (None, None);
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut ctx),
            )?;
            let device: ID3D11Device = device.unwrap();
            let output: IDXGIOutput1 = device.cast::<IDXGIDevice>()?.GetAdapter()?.EnumOutputs(0)?.cast()?;
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
                width,
                height,
                rect: (r.left, r.top, r.right, r.bottom),
            })
        }
    }

    /// Waits up to `timeout_ms` for a screen update. Returns false on timeout or cursor-only updates.
    /// Note: the mouse cursor is not part of the duplicated image.
    pub fn next(&mut self, buf: &mut Vec<u8>, timeout_ms: u32) -> Result<bool> {
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut res = None;
            if let Err(e) = self.dup.AcquireNextFrame(timeout_ms, &mut info, &mut res) {
                return if e.code() == DXGI_ERROR_WAIT_TIMEOUT { Ok(false) } else { Err(e) };
            }
            let fresh = info.LastPresentTime != 0;
            if fresh {
                let tex: ID3D11Texture2D = res.unwrap().cast()?;
                self.ctx.CopyResource(&self.staging, &tex);
            }
            self.dup.ReleaseFrame()?;
            if !fresh {
                return Ok(false);
            }

            let mut map = D3D11_MAPPED_SUBRESOURCE::default();
            self.ctx.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut map))?;
            let row = self.width * 4;
            buf.resize(row * self.height, 0);
            let src = std::slice::from_raw_parts(map.pData as *const u8, map.RowPitch as usize * self.height);
            for (y, dst) in buf.chunks_exact_mut(row).enumerate() {
                let s = y * map.RowPitch as usize;
                dst.copy_from_slice(&src[s..s + row]);
            }
            self.ctx.Unmap(&self.staging, 0);
            Ok(true)
        }
    }
}
