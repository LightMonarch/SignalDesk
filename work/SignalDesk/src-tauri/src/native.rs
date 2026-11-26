use ::windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
};
use serde_json::{json, Value};
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
#[derive(Clone, Copy)]
pub struct CpuTimes {
    idle: u64,
    kernel: u64,
    user: u64,
}
impl CpuTimes {
    pub fn get() -> Option<Self> {
        use windows_sys::Win32::{Foundation::FILETIME, System::Threading::GetSystemTimes};
        let mut idle = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut kernel = idle;
        let mut user = idle;
        // The pointers are writable FILETIME structures valid for the full call.
        if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
            return None;
        }
        let ticks = |t: FILETIME| u64::from(t.dwHighDateTime) << 32 | u64::from(t.dwLowDateTime);
        Some(Self {
            idle: ticks(idle),
            kernel: ticks(kernel),
            user: ticks(user),
        })
    }
    pub fn usage_since(self, prior: Self) -> Option<f64> {
        let idle = self.idle.checked_sub(prior.idle)?;
        let total = self
            .kernel
            .checked_sub(prior.kernel)?
            .checked_add(self.user.checked_sub(prior.user)?)?;
        let busy = total.checked_sub(idle)?;
        (total > 0).then(|| 100. * busy as f64 / total as f64)
    }
}

pub fn firmware() -> (Value, Value) {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    let Ok(key) =
        RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"HARDWARE\DESCRIPTION\System\BIOS")
    else {
        return (Value::Null, Value::Null);
    };
    let prop = |name: &str| {
        key.get_value::<String, _>(name)
            .ok()
            .filter(|s| !s.trim().is_empty())
    };
    (
        json!({"Manufacturer":prop("SystemManufacturer"),"Model":prop("SystemProductName"),"SystemType":std::env::consts::ARCH}),
        json!({"Manufacturer":prop("BIOSVendor"),"SMBIOSBIOSVersion":prop("BIOSVersion")}),
    )
}
