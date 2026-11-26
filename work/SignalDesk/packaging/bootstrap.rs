#![windows_subsystem = "windows"]
use std::{
    ffi::OsStr, fs, io::{self, Write}, os::windows::ffi::OsStrExt, path::{Path, PathBuf},
    process::Command, time::{SystemTime, UNIX_EPOCH},
};

static MSI: &[u8] = include_bytes!(env!("SIGNALDESK_MSI"));

#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(window: isize, text: *const u16, caption: *const u16, flags: u32) -> i32;
}
fn message(text: &str, flags: u32) {
    let caption: Vec<u16> = OsStr::new("Silicon Desk Setup").encode_wide().chain([0]).collect();
    let text: Vec<u16> = OsStr::new(text).encode_wide().chain([0]).collect();
    // Both strings are NUL-terminated and valid for the duration of the call.
    unsafe { MessageBoxW(0, text.as_ptr(), caption.as_ptr(), flags) };
}
struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.0.join("SiliconDesk.msi"));
        let _ = fs::remove_dir(&self.0);
    }
}
impl Staging {
    fn new() -> io::Result<Self> {
        let temp = std::env::temp_dir();
        let prefix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        for attempt in 0..16 {
            let path = temp.join(format!("signaldesk-{}-{prefix}-{attempt}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(err),
            }
        }
        Err(io::Error::new(io::ErrorKind::AlreadyExists, "could not create a staging directory"))
    }
}
fn extract(path: &Path) -> io::Result<()> {
    let mut file = fs::OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(MSI)?;
    file.sync_all()
}
fn install(quiet: bool) -> io::Result<i32> {
    let staging = Staging::new()?;
    let package = staging.0.join("SiliconDesk.msi");
    extract(&package)?;
    let windows = std::env::var_os("SystemRoot").ok_or_else(|| io::Error::new(io::ErrorKind::NotFound,"SystemRoot is unavailable"))?;
    let mut command = Command::new(PathBuf::from(windows).join(r"System32\msiexec.exe"));
    command.arg("/i").arg(&package).arg("/norestart");
    if quiet { command.arg("/qn"); }
    Ok(command.status()?.code().unwrap_or(1))
}
fn main() {
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    let quiet = first.as_deref() == Some(OsStr::new("/quiet"));
    let result = match first.as_deref() {
        None => install(false),
        Some(arg) if arg == "/quiet" && args.next().is_none() => install(true),
        Some(arg) if arg == "--extract-msi" => match (args.next(), args.next()) {
            (Some(path), None) => extract(Path::new(&path)).map(|_| 0),
            _ => Err(io::Error::new(io::ErrorKind::InvalidInput, "supply a new destination file")),
        },
        _ => Err(io::Error::new(io::ErrorKind::InvalidInput, "unsupported setup argument")),
    };
    let code = match result {
        Ok(0) => 0,
        Ok(3010) => {
            if !quiet { message("Silicon Desk was installed. Windows requires a restart to finish setup.", 0x40); }
            3010
        }
        Ok(1602) => 1602,
        Ok(code) => {
            if !quiet { message(&format!("Windows Installer returned error {code}."), 0x10); }
            code
        }
        Err(err) => {
            if !quiet { message(&format!("Setup could not start. {err}."), 0x10); }
            1
        }
    };
    std::process::exit(code);
}
