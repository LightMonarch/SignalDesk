use crate::device::dword;
use serde_json::{json, Value};
use std::{
    ffi::{c_char, c_void, CStr, CString, OsString},
    mem::{offset_of, size_of, zeroed},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Devices::DeviceAndDriverInstallation::*,
    Foundation::{FreeLibrary, ERROR_NO_MORE_ITEMS, HMODULE, INVALID_HANDLE_VALUE},
    System::{
        LibraryLoader::{
            GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        },
        Power::CM_POWER_DATA,
        SystemInformation::GetSystemDirectoryW,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Power {
    Active,
    Sleeping,
    Unknown,
}
fn power(code: Option<u32>) -> Power {
    match code {
        Some(1) => Power::Active,       // PowerDeviceD0.
        Some(2..=4) => Power::Sleeping, // D1, D2, D3.
        _ => Power::Unknown,
    }
}
struct Adapter {
    name: String,
    vendor: &'static str,
    bus_id: Option<String>,
    power: Power,
}
struct DisplayList(HDEVINFO);
impl Drop for DisplayList {
    fn drop(&mut self) {
        unsafe {
            SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}
impl DisplayList {
    fn property(&self, item: &SP_DEVINFO_DATA, property: u32) -> Option<Vec<u8>> {
        let mut data = vec![0u8; 8192];
        let mut bytes = 0;
        let ok = unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                self.0,
                item,
                property,
                null_mut(),
                data.as_mut_ptr(),
                data.len() as u32,
                &mut bytes,
            )
        };
        if ok == 0 || bytes as usize > data.len() {
            return None;
        }
        data.truncate(bytes as usize);
        Some(data)
    }

    fn text(&self, item: &SP_DEVINFO_DATA, property: u32) -> Option<String> {
        let data = self.property(item, property)?;
        let value = String::from_utf16_lossy(
            &data
                .chunks_exact(2)
                .map(|p| u16::from_le_bytes([p[0], p[1]]))
                .take_while(|&v| v != 0)
                .collect::<Vec<_>>(),
        );
        (!value.is_empty()).then_some(value)
    }
}

fn adapters() -> Result<Vec<Adapter>, String> {
    let list =
        unsafe { SetupDiGetClassDevsW(&GUID_DEVCLASS_DISPLAY, null(), null_mut(), DIGCF_PRESENT) };
    if list == INVALID_HANDLE_VALUE as HDEVINFO {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let list = DisplayList(list);
    let mut rows = Vec::new();
    for index in 0.. {
        let mut item: SP_DEVINFO_DATA = unsafe { zeroed() };
        item.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
        if unsafe { SetupDiEnumDeviceInfo(list.0, index, &mut item) } == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_NO_MORE_ITEMS as i32) {
                break;
            }
            return Err(error.to_string());
        }
        let ids = list
            .text(&item, SPDRP_HARDWAREID)
            .unwrap_or_default()
            .to_ascii_uppercase();
        let vendor = if ids.starts_with("PCI\\VEN_10DE&") {
            "NVIDIA"
        } else if ids.starts_with("PCI\\VEN_1002&") {
            "AMD"
        } else if ids.starts_with("PCI\\VEN_8086&") {
            "Intel"
        } else {
            continue;
        };
        let name = list
            .text(&item, SPDRP_FRIENDLYNAME)
            .or_else(|| list.text(&item, SPDRP_DEVICEDESC))
            .unwrap_or_else(|| format!("{vendor} graphics adapter"));
        let value = list.property(&item, SPDRP_DEVICE_POWER_DATA);
        let state = power(value.as_ref().and_then(|data| {
            if (dword(data, 0)? as usize) < 8 {
                return None;
            }
            dword(data, offset_of!(CM_POWER_DATA, PD_MostRecentPowerState))
        }));
        let bus_id = list.property(&item, SPDRP_BUSNUMBER).and_then(|b| {
            pci_address(
                dword(&b, 0)?,
                dword(&list.property(&item, SPDRP_ADDRESS)?, 0)?,
            )
        });
        rows.push(Adapter {
            name,
            vendor,
            bus_id,
            power: state,
        });
    }
    Ok(rows)
}

fn pci_address(bus: u32, address: u32) -> Option<String> {
    // For a PCI device SPDRP_ADDRESS packs the device in the high word and function in the low word.
    let device = address >> 16;
    let function = address & 0xffff;
    (bus <= 255 && device <= 31 && function <= 7)
        .then(|| format!("00000000:{bus:02x}:{device:02x}.{function}"))
}

type Init = unsafe extern "C" fn(u32) -> i32;
type Shutdown = unsafe extern "C" fn() -> i32;
type ByPci = unsafe extern "C" fn(*const c_char, *mut *mut c_void) -> i32;
type Count = unsafe extern "C" fn(*mut u32) -> i32;
type ByIndex = unsafe extern "C" fn(u32, *mut *mut c_void) -> i32;
type Temp = unsafe extern "C" fn(*mut c_void, u32, *mut u32) -> i32;
type NvmlName = unsafe extern "C" fn(*mut c_void, *mut c_char, u32) -> i32;
struct Dll(HMODULE);
impl Drop for Dll {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.0);
        }
    }
}
struct Nvml {
    _dll: Dll,
    shutdown: Shutdown,
    by_pci: ByPci,
    count: Count,
    by_index: ByIndex,
    temperature: Temp,
    name: NvmlName,
}
impl Nvml {
    fn open() -> Result<Self, String> {
        let mut system = [0u16; 260];
        let len = unsafe { GetSystemDirectoryW(system.as_mut_ptr(), system.len() as u32) } as usize;
        let mut candidates: Vec<PathBuf> = Vec::new();
        if (1..system.len()).contains(&len) {
            candidates.push(PathBuf::from(OsString::from_wide(&system[..len])).join("nvml.dll"));
        }
        if let Some(programs) = crate::thermal::program_files() {
            candidates.push(programs.join(r"NVIDIA Corporation\NVSMI\nvml.dll"));
        }
        let dll = candidates
            .iter()
            .find_map(|path| Self::load(path))
            .ok_or("The installed NVIDIA driver did not provide NVML.")?;
        // Bind the documented NVML C ABI while holding the DLL reference.
        macro_rules! symbol {
            ($name:literal, $ty:ty) => {{
                let address = unsafe { GetProcAddress(dll.0, concat!($name, "\0").as_ptr()) }
                    .ok_or(concat!("The installed NVML does not supply ", $name, "."))?;
                unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(address) }
            }};
        }
        let initialize = symbol!("nvmlInitWithFlags", Init);
        let shutdown = symbol!("nvmlShutdown", Shutdown);
        let by_pci = symbol!("nvmlDeviceGetHandleByPciBusId_v2", ByPci);
        let count = symbol!("nvmlDeviceGetCount_v2", Count);
        let by_index = symbol!("nvmlDeviceGetHandleByIndex_v2", ByIndex);
        let temperature = symbol!("nvmlDeviceGetTemperature", Temp);
        let name = symbol!("nvmlDeviceGetName", NvmlName);
        // The caller checks that every NVIDIA adapter is in D0 before lazy initialization.
        let code = unsafe { initialize(0) };
        if code != 0 {
            return Err(format!(
                "NVIDIA NVML initialization returned status {code}."
            ));
        }
        Ok(Self {
            _dll: dll,
            shutdown,
            by_pci,
            count,
            by_index,
            temperature,
            name,
        })
    }

    fn load(path: &Path) -> Option<Dll> {
        let path: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
        let dll = unsafe {
            LoadLibraryExW(
                path.as_ptr(),
                null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        (!dll.is_null()).then_some(Dll(dll))
    }

    fn measure(
        &self,
        pci: Option<&str>,
        sole_nvidia: bool,
    ) -> Result<(u32, Option<String>), String> {
        const NOT_FOUND: i32 = 6;
        let mut handle = null_mut();
        let mut code = NOT_FOUND;
        if let Some(pci) = pci {
            let pci = CString::new(pci).map_err(|_| "Invalid GPU bus identity.")?;
            code = unsafe { (self.by_pci)(pci.as_ptr(), &mut handle) };
        }
        if code == NOT_FOUND && sole_nvidia {
            // Only an unambiguous one-to-one match may use an ordinal.
            let mut count = 0;
            let count_code = unsafe { (self.count)(&mut count) };
            if count_code == 0 && count == 1 {
                code = unsafe { (self.by_index)(0, &mut handle) };
            }
        }
        if code != 0 || handle.is_null() {
            return Err(format!(
                "NVML could not open the active GPU (status {code})."
            ));
        }
        let mut value = 0;
        let code = unsafe { (self.temperature)(handle, 0, &mut value) }; // NVML_TEMPERATURE_GPU.
        if code != 0 || value > 125 {
            return Err(format!(
                "NVML returned no valid GPU temperature (status {code})."
            ));
        }
        let mut name = [0 as c_char; 128];
        let code = unsafe { (self.name)(handle, name.as_mut_ptr(), name.len() as u32) };
        let name = if code == 0 && name.contains(&0) {
            Some(
                unsafe { CStr::from_ptr(name.as_ptr()) }
                    .to_string_lossy()
                    .into_owned(),
            )
        } else {
            None
        };
        Ok((value, name))
    }
}
impl Drop for Nvml {
    fn drop(&mut self) {
        unsafe {
            (self.shutdown)();
        }
    }
}

pub struct Monitor {
    nvml: Option<Nvml>,
    error: Option<(Instant, String)>,
}
impl Monitor {
    pub fn new() -> Self {
        Self {
            nvml: None,
            error: None,
        }
    }
    pub fn pause(&mut self) {
        self.nvml = None;
    }

    pub fn poll(&mut self) -> (Vec<Value>, Option<String>) {
        let adapters = match adapters() {
            Ok(items) => items,
            Err(error) => {
                self.pause();
                return (vec![], Some(error));
            }
        };
        let sole_nvidia = adapters.iter().filter(|gpu| gpu.vendor == "NVIDIA").count() == 1;
        let all_nvidia_active = adapters
            .iter()
            .filter(|gpu| gpu.vendor == "NVIDIA")
            .all(|gpu| gpu.power == Power::Active);
        let eligible = all_nvidia_active
            && adapters.iter().any(|gpu| {
                gpu.vendor == "NVIDIA"
                    && gpu.power == Power::Active
                    && (gpu.bus_id.is_some() || sole_nvidia)
            });
        if !eligible {
            self.pause();
        }
        if eligible
            && self.nvml.is_none()
            && self
                .error
                .as_ref()
                .is_none_or(|(t, _)| t.elapsed() >= Duration::from_secs(25))
        {
            match Nvml::open() {
                Ok(nvml) => {
                    self.nvml = Some(nvml);
                    self.error = None;
                }
                Err(error) => {
                    self.error = Some((Instant::now(), error));
                }
            }
        }
        let mut rows = Vec::with_capacity(adapters.len());
        for gpu in adapters {
            let (state,temperature,detail,name) =
                match (gpu.power,gpu.vendor,gpu.bus_id.as_deref(),self.nvml.as_ref()) {
                    (Power::Sleeping,_,_,_)=>("Sleeping",None,Some("Windows reports a low-power state. Direct polling is suspended.".into()),None),
                    (Power::Unknown,_,_,_)=>("Power state unavailable",None,Some("Direct polling waits for a confirmed active power state.".into()),None),
                    (Power::Active,"NVIDIA",_,_) if !all_nvidia_active =>("Active",None,Some("Direct NVIDIA polling waits until Windows reports all NVIDIA adapters active.".into()),None),
                    (Power::Active,"NVIDIA",pci,Some(nvml)) if pci.is_some() || sole_nvidia =>match nvml.measure(pci, sole_nvidia) {
                        Ok((temperature,name))=>("Active",Some(temperature),None,name),
                        Err(error)=>("Active",None,Some(error),None)
                    },
                    (Power::Active,"NVIDIA",None,_) if !sole_nvidia =>("Active",None,Some("Windows did not expose a PCI address to identify this NVIDIA adapter.".into()),None),
                    (Power::Active,"NVIDIA",_,_)=>("Active",None,self.error.as_ref().map(|(_,e)|e.clone()),None),
                    _=>("Active",None,Some("Temperature is read from the LibreHardwareMonitor provider when available.".into()),None)
                };
            rows.push(json!({"name":name.unwrap_or(gpu.name),"vendor":gpu.vendor,"state":state,
                "temperature":temperature,"detail":detail,"source":temperature.map(|_|"NVIDIA NVML")}));
        }
        (rows, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sleeping_and_unknown_adapters_are_distinct() {
        assert_eq!(power(Some(1)), Power::Active);
        assert_eq!(power(Some(4)), Power::Sleeping);
        assert_eq!(power(Some(0)), Power::Unknown);
        assert_eq!(power(None), Power::Unknown);
        assert_eq!(
            pci_address(5, 0x0015_0001).as_deref(),
            Some("00000000:05:15.1")
        );
        assert!(pci_address(10, u32::MAX).is_none());
    }
}
