use serde_json::{json, Value};
use std::{
    ffi::OsString,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::atomic::{fence, Ordering},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{FILETIME, HANDLE, WAIT_OBJECT_0},
    Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG},
    System::{
        Com::CoTaskMemFree,
        Memory::{MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ},
        Threading::{
            CreateEventW, GetCurrentProcess, GetProcessTimes, ResetEvent, SetEvent,
            WaitForSingleObject,
        },
    },
    UI::{
        Shell::{
            FOLDERID_ProgramFilesX64, SHGetKnownFolderPath, ShellExecuteExW, SEE_MASK_NOASYNC,
            SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
        },
        WindowsAndMessaging::SW_HIDE,
    },
};

const MAPPING_BYTES: usize = 65_536;

fn wide(value: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain([0]).collect()
}

pub fn program_files() -> Option<PathBuf> {
    let mut raw = null_mut();
    // The Shell allocates a UTF-16 path in COM task memory on success.
    if unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFilesX64, 0, null_mut(), &mut raw) } < 0
        || raw.is_null()
    {
        return None;
    }
    let path = unsafe {
        let mut end = 0;
        while *raw.add(end) != 0 {
            end += 1;
        }
        let value = OsString::from_wide(std::slice::from_raw_parts(raw, end));
        CoTaskMemFree(raw.cast());
        PathBuf::from(value)
    };
    Some(path)
}

fn helper_path() -> Option<PathBuf> {
    let programs = program_files()?;
    let protected = std::fs::canonicalize(&programs).ok()?;
    let relative = Path::new("Silicon Desk")
        .join("sensor-runtime")
        .join("ThermalReader.exe");
    let candidate = programs.join(&relative);
    let actual = std::fs::canonicalize(&candidate).ok()?;
    (actual == protected.join(relative) && actual.is_file()).then_some(candidate)
}

pub struct Bridge {
    helper: Option<PathBuf>,
    running: Option<Helper>,
    detail: Option<String>,
}
struct Helper {
    process: OwnedHandle,
    pause: OwnedHandle,
    name: Vec<u16>,
    started: Instant,
    paused: bool,
}
impl Bridge {
    pub fn new() -> Self {
        Self {
            helper: None,
            running: None,
            detail: None,
        }
    }

    pub fn configure(&mut self) {
        self.helper = helper_path();
    }

    pub fn start(&mut self, paused: bool) -> Result<(), String> {
        if let Some(helper) = &self.running {
            if !exited(&helper.process) {
                return Err("The sensor helper is already running for this session. Restart Silicon Desk after changing its driver.".into());
            }
            self.running = None;
        }
        let path = self.helper.as_ref().ok_or(
            "Install Silicon Desk in the standard Program Files location to use the bundled sensor helper."
        )?;
        let mut random = [0u8; 16];
        if unsafe {
            BCryptGenRandom(
                null_mut(),
                random.as_mut_ptr(),
                random.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        } != 0
        {
            return Err("Windows could not create a private sensor channel.".into());
        }
        let key = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let map = format!(r"Local\SiliconDesk.Thermals.{key}");
        let event = wide(format!("{map}.Pause"));
        let pause = owned(unsafe { CreateEventW(null(), 1, paused.into(), event.as_ptr()) })?;
        let parent_start = process_start()?;
        let args = wide(format!("{key} {} {parent_start}", std::process::id()));
        let verb = wide("runas");
        let working = wide(path.parent().ok_or("Invalid helper installation path.")?);
        let file = wide(path);
        let mut info: SHELLEXECUTEINFOW = unsafe { zeroed() };
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = args.as_ptr();
        info.lpDirectory = working.as_ptr();
        info.nShow = SW_HIDE;
        // Only the known installed executable is elevated. Arguments contain a random channel and
        // the identity of this process; the helper offers no privileged command interface.
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            let error = std::io::Error::last_os_error();
            let detail = if error.raw_os_error() == Some(1223) {
                "Administrator access was declined. Enable the helper again when ready.".into()
            } else {
                format!("Windows could not launch the sensor helper: {error}")
            };
            self.detail = Some(detail.clone());
            return Err(detail);
        }
        let process = owned(info.hProcess)?;
        self.running = Some(Helper {
            process,
            pause,
            name: wide(map),
            started: Instant::now(),
            paused,
        });
        self.detail = None;
        Ok(())
    }

    pub fn read(&mut self, paused: bool) -> Value {
        let installed = self.helper.is_some();
        if let Some(helper) = &mut self.running {
            if exited(&helper.process) {
                self.running = None;
                self.detail = Some("The sensor helper exited before sending live readings. Approve it using the same Windows account and check the driver.".into());
            } else {
                if helper.paused != paused {
                    let event = helper.pause.as_raw_handle();
                    unsafe {
                        if paused {
                            SetEvent(event);
                        } else {
                            ResetEvent(event);
                        }
                    }
                    helper.paused = paused;
                }
                if paused {
                    return json!({"state":"paused","installed":installed,"detail":"Hardware polling is paused."});
                }
                if let Some(mut data) = frame(&helper.name) {
                    if data["timestamp"]
                        .as_i64()
                        .is_some_and(|ts| crate::now().abs_diff(ts) <= 7)
                    {
                        data["installed"] = json!(installed);
                        return data;
                    }
                }
                let (state, detail) = if helper.started.elapsed() < Duration::from_secs(14) {
                    ("starting", "Waiting for the sensor helper to report.")
                } else {
                    ("offline", "The sensor helper stopped reporting. Check driver access and restart Silicon Desk.")
                };
                return json!({"state":state,"installed":installed,"detail":detail});
            }
        }
        if !installed {
            return json!({"state":"notInstalled","installed":false,
                "detail":"The Setup EXE installs the sensor helper. The portable app can also read a running LibreHardwareMonitor provider."});
        }
        json!({"state":"needsAdmin","installed":true,"detail":self.detail.as_deref().unwrap_or(
            "Enable the bundled sensor helper to read CPU hardware registers with Administrator access.")})
    }
}

fn owned(raw: HANDLE) -> Result<OwnedHandle, String> {
    if raw.is_null() {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
    }
}
fn exited(handle: &OwnedHandle) -> bool {
    unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) == WAIT_OBJECT_0 }
}
fn process_start() -> Result<u64, String> {
    let mut creation: FILETIME = unsafe { zeroed() };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    if unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(u64::from(creation.dwHighDateTime) << 32 | u64::from(creation.dwLowDateTime))
}

fn frame(name: &[u16]) -> Option<Value> {
    let handle = owned(unsafe { OpenFileMappingW(FILE_MAP_READ, 0, name.as_ptr()) }).ok()?;
    let view = unsafe { MapViewOfFile(handle.as_raw_handle(), FILE_MAP_READ, 0, 0, MAPPING_BYTES) };
    if view.Value.is_null() {
        return None;
    }
    let base = view.Value.cast::<u8>();
    let result = unsafe {
        let sequence = base.cast::<u32>();
        let first = sequence.read_volatile();
        fence(Ordering::Acquire);
        let len = base.add(4).cast::<u32>().read_volatile() as usize;
        if first == 0 || first & 1 != 0 || len == 0 || len > MAPPING_BYTES - 8 {
            None
        } else {
            let bytes: Vec<u8> = (0..len).map(|i| base.add(8 + i).read_volatile()).collect();
            fence(Ordering::Acquire);
            if sequence.read_volatile() != first {
                None
            } else {
                serde_json::from_slice::<Value>(&bytes)
                    .ok()
                    .filter(Value::is_object)
            }
        }
    };
    unsafe {
        UnmapViewOfFile(view);
    }
    result
}
