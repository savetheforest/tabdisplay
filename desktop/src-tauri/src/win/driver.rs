//! Installs, restarts and removes the Virtual Display Driver's device ("Root\MttVDD") through SetupAPI.
//! Needs admin: runs from the installer (`--install-driver`, `--uninstall-driver`) or from the app
//! behind a UAC prompt (`--restart-driver`).
use super::display;
use std::fs;
use std::io;
use std::path::PathBuf;
use windows::core::{Result, GUID, HSTRING, PCWSTR};
use windows::Win32::Devices::DeviceAndDriverInstallation::*;

const HWID: &str = r"Root\MttVDD";
const DIR: &str = r"C:\VirtualDisplayDriver";
/// Present only if we created the device, so uninstalling TabDisplay leaves a user's own VDD alone.
const MARKER: &str = r"C:\VirtualDisplayDriver\installed-by-tabdisplay";

/// Handles a driver command-line flag, returning the process exit code; None if `arg` isn't one.
pub fn cli(arg: &str) -> Option<i32> {
    let result = match arg {
        "--install-driver" => install(),
        "--uninstall-driver" => uninstall(),
        "--restart-driver" => restart(),
        "--service" => return Some(super::service::run()),
        _ => return None,
    };
    Some(match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{arg}: {e}");
            1
        }
    })
}

/// Resources ship next to the executable (Tauri puts bundle resources in the install dir).
pub fn resource(name: &str) -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .with_file_name(name)
}

/// Makes sure the driver is installed with our mode list and running with it.
fn install() -> io::Result<()> {
    fs::create_dir_all(DIR)?;
    let settings = PathBuf::from(DIR).join("vdd_settings.xml");
    if !settings.exists() {
        // Our template keeps the mode count low; the driver creates no monitor past ~100 modes.
        fs::copy(resource(r"vdd\vdd_settings.xml"), &settings)?;
    }
    display::ensure_modes(&[])?;
    if !installed()? {
        create_device().map_err(io::Error::other)?;
        fs::write(MARKER, "")?;
    } else {
        restart()?; // already installed (e.g. by VDD Control): reload so it reads the mode list
    }
    Ok(())
}

fn uninstall() -> io::Result<()> {
    if fs::metadata(MARKER).is_err() {
        return Ok(());
    }
    for_each_device(|set, dev| unsafe { SetupDiCallClassInstaller(DIF_REMOVE, set, Some(dev)) })?;
    let _ = fs::remove_file(MARKER);
    Ok(())
}

/// Disables and re-enables the device: clears Code 43 and makes the driver re-read its settings.
pub fn restart() -> io::Result<()> {
    set_enabled(false)?;
    set_enabled(true)
}

/// Enabling the device plugs the virtual monitor in; disabling unplugs it (and stops the driver).
pub fn set_enabled(on: bool) -> io::Result<()> {
    for_each_device(|set, dev| unsafe {
        let params = SP_PROPCHANGE_PARAMS {
            ClassInstallHeader: SP_CLASSINSTALL_HEADER {
                cbSize: size_of::<SP_CLASSINSTALL_HEADER>() as u32,
                InstallFunction: DIF_PROPERTYCHANGE,
            },
            StateChange: if on { DICS_ENABLE } else { DICS_DISABLE },
            Scope: DICS_FLAG_GLOBAL,
            HwProfile: 0,
        };
        SetupDiSetClassInstallParamsW(
            set,
            Some(dev),
            Some(&params.ClassInstallHeader),
            size_of::<SP_PROPCHANGE_PARAMS>() as u32,
        )?;
        SetupDiCallClassInstaller(DIF_PROPERTYCHANGE, set, Some(dev))
    })
}

/// Creates the root-enumerated device and installs the bundled, signed driver on it
/// (what `devcon install MttVDD.inf Root\MttVDD` does).
fn create_device() -> Result<()> {
    let inf = HSTRING::from(resource(r"vdd\MttVDD.inf").as_os_str());
    unsafe {
        let mut guid = GUID::zeroed();
        let mut class = [0u16; 64];
        SetupDiGetINFClassW(&inf, &mut guid, &mut class, None)?;
        let set = SetupDiCreateDeviceInfoList(Some(&guid), None)?;
        let mut dev = SP_DEVINFO_DATA {
            cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        let created = (|| {
            SetupDiCreateDeviceInfoW(
                set,
                PCWSTR(class.as_ptr()),
                &guid,
                PCWSTR::null(),
                None,
                DICD_GENERATE_ID,
                Some(&mut dev),
            )?;
            let hwid: Vec<u8> = HWID
                .encode_utf16()
                .chain([0, 0])
                .flat_map(u16::to_le_bytes)
                .collect(); // REG_MULTI_SZ
            SetupDiSetDeviceRegistryPropertyW(set, &mut dev, SPDRP_HARDWAREID, Some(&hwid))?;
            SetupDiCallClassInstaller(DIF_REGISTERDEVICE, set, Some(&dev))
        })();
        let _ = SetupDiDestroyDeviceInfoList(set);
        created?;
        UpdateDriverForPlugAndPlayDevicesW(
            None,
            &HSTRING::from(HWID),
            &inf,
            INSTALLFLAG_FORCE,
            None,
        )
    }
}

/// Whether any device (present or not) has our hardware ID.
fn installed() -> io::Result<bool> {
    let mut any = false;
    for_each_device(|_, _| {
        any = true;
        Ok(())
    })?;
    Ok(any)
}

fn for_each_device(mut f: impl FnMut(HDEVINFO, &SP_DEVINFO_DATA) -> Result<()>) -> io::Result<()> {
    unsafe {
        let set = SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_ALLCLASSES)
            .map_err(io::Error::other)?;
        let mut result = Ok(());
        let mut dev = SP_DEVINFO_DATA {
            cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        let mut i = 0;
        while result.is_ok() && SetupDiEnumDeviceInfo(set, i, &mut dev).is_ok() {
            i += 1;
            let mut buf = [0u8; 1024];
            if SetupDiGetDeviceRegistryPropertyW(
                set,
                &dev,
                SPDRP_HARDWAREID,
                None,
                Some(&mut buf),
                None,
            )
            .is_err()
            {
                continue;
            }
            let ids = String::from_utf16_lossy(
                &buf.chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect::<Vec<_>>(),
            );
            if ids.split('\0').any(|id| id.eq_ignore_ascii_case(HWID)) {
                result = f(set, &dev).map_err(io::Error::other);
            }
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
        result
    }
}
