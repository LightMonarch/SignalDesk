use ::windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
};
use serde_json::{json, Value};
use std::{os::windows::process::CommandExt, path::PathBuf, process::Command};
use windows_sys::Win32::{
    Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS},
    NetworkManagement::{
        IpHelper::{FreeMibTable, GetIfTable2, IF_TYPE_SOFTWARE_LOOPBACK},
        Ndis::{IfOperStatusUp, MediaConnectStateConnected},
    },
};

fn utf16(value: &[u16]) -> String {
    String::from_utf16_lossy(&value[..value.iter().position(|v| *v == 0).unwrap_or(value.len())])
}

pub fn graphics() -> (Vec<Value>, Vec<Value>) {
    let mut adapters = vec![];
    let mut displays = vec![];
    // DXGI owns the factory and adapter handles. A failed index means enumeration ended.
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return (adapters, displays);
        };
        for index in 0..64 {
            let Ok(adapter) = factory.EnumAdapters1(index) else {
                break;
            };
            let Ok(d) = adapter.GetDesc1() else { continue };
            if d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let vendor = match d.VendorId {
                0x10de => "NVIDIA",
                0x1002 | 0x1022 => "AMD",
                0x8086 => "Intel",
                0x1414 => "Microsoft",
                0x5143 => "Qualcomm",
                _ => "Other",
            };
            adapters.push(json!({"Name":utf16(&d.Description),"vendor":vendor,"vendorId":d.VendorId,"deviceId":d.DeviceId,"luid":format!("0x{:08x}_0x{:08x}",d.AdapterLuid.HighPart as u32,d.AdapterLuid.LowPart),"dedicated":d.DedicatedVideoMemory,"shared":d.SharedSystemMemory}));
            for output in 0..64 {
                let Ok(monitor) = adapter.EnumOutputs(output) else {
                    break;
                };
                let Ok(desc) = monitor.GetDesc() else {
                    continue;
                };
                if !desc.AttachedToDesktop.as_bool() {
                    continue;
                }
                let mut mode: DEVMODEW = std::mem::zeroed();
                mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
                let has_mode = EnumDisplaySettingsW(
                    desc.DeviceName.as_ptr(),
                    ENUM_CURRENT_SETTINGS,
                    &mut mode,
                ) != 0;
                displays.push(json!({"Name":utf16(&desc.DeviceName),"ScreenWidth":if has_mode{mode.dmPelsWidth as i32}else{desc.DesktopCoordinates.right-desc.DesktopCoordinates.left},"ScreenHeight":if has_mode{mode.dmPelsHeight as i32}else{desc.DesktopCoordinates.bottom-desc.DesktopCoordinates.top},"RefreshRate":(has_mode&&mode.dmDisplayFrequency>1).then_some(mode.dmDisplayFrequency),"Adapter":utf16(&d.Description)}));
            }
        }
    }
    (adapters, displays)
}

pub fn network_connected() -> Option<bool> {
    let mut table = std::ptr::null_mut();
    // The system allocates a variable-length table. The slice is borrowed until FreeMibTable.
    unsafe {
        if GetIfTable2(&mut table) != 0 || table.is_null() {
            return None;
        }
        let count = (*table).NumEntries as usize;
        let rows =
            std::slice::from_raw_parts(std::ptr::addr_of!((*table).Table).cast::<_>(), count);
        let mut present = false;
        let connected = rows
            .iter()
            .filter(
                |r: &&windows_sys::Win32::NetworkManagement::IpHelper::MIB_IF_ROW2| {
                    matches!(r.Type, 6 | 71 | 243 | 244) && r.Type != IF_TYPE_SOFTWARE_LOOPBACK
                },
            )
            .any(|r| {
                present = true;
                r.OperStatus == IfOperStatusUp && r.MediaConnectState == MediaConnectStateConnected
            });
        FreeMibTable(table.cast());
        present.then_some(connected)
    }
}
pub fn nvidia() -> Vec<Value> {
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::var_os("SystemRoot") {
        candidates.push(PathBuf::from(dir).join(r"System32\nvidia-smi.exe"));
    }
    if let Some(dir) = std::env::var_os("ProgramFiles") {
        candidates.push(PathBuf::from(dir).join(r"NVIDIA Corporation\NVSMI\nvidia-smi.exe"));
    }
    let Some(path) = candidates.into_iter().find(|p| p.is_file()) else {
        return vec![];
    };
    let Ok(result)=Command::new(path).creation_flags(0x08000000).args(["--query-gpu=index,name,driver_version,utilization.gpu,memory.used,memory.total,temperature.gpu,clocks.gr,clocks.mem,power.draw,fan.speed,utilization.encoder,utilization.decoder","--format=csv,noheader,nounits"]).output() else{return vec![]};
    if !result.status.success() {
        return vec![];
    }
    String::from_utf8_lossy(&result.stdout).lines().filter_map(|line|{
  let x:Vec<_>=line.split(',').map(str::trim).collect();if x.len()!=13{return None}
  let n=|i:usize|x[i].parse::<f64>().ok();
  Some(json!({"index":n(0),"name":x[1],"driver":x[2],"utilization":n(3),"vramUsed":n(4).map(|v|v*1048576.),"vramTotal":n(5).map(|v|v*1048576.),"temperature":n(6),"coreClock":n(7),"memoryClock":n(8),"power":n(9),"fan":n(10),"encode":n(11),"decode":n(12)}))
 }).collect()
}
