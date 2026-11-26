use std::{
    mem::{offset_of, size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::{
    core::GUID,
    Win32::{
        Devices::DeviceAndDriverInstallation::*,
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
        System::IO::DeviceIoControl,
    },
};

pub struct Device(HANDLE);
impl Device {
    pub fn open(path: &[u16], access: u32) -> std::io::Result<Self> {
        debug_assert_eq!(path.last(), Some(&0));
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(Self(handle))
        }
    }

    pub fn query(&self, code: u32, input: &[u8], capacity: usize) -> std::io::Result<Vec<u8>> {
        // Windows copies METHOD_BUFFERED requests into an aligned kernel buffer.
        let mut output = vec![0u8; capacity];
        let mut read = 0;
        let success = unsafe {
            DeviceIoControl(
                self.0,
                code,
                if input.is_empty() {
                    null()
                } else {
                    input.as_ptr().cast()
                },
                input.len() as u32,
                output.as_mut_ptr().cast(),
                output.len() as u32,
                &mut read,
                null_mut(),
            )
        };
        if success == 0 {
            return Err(std::io::Error::last_os_error());
        }
        output.truncate(read as usize);
        Ok(output)
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

struct InterfaceSet(HDEVINFO);
impl Drop for InterfaceSet {
    fn drop(&mut self) {
        unsafe {
            SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}

pub fn interfaces(guid: &GUID) -> std::io::Result<Vec<Vec<u16>>> {
    let handle = unsafe {
        SetupDiGetClassDevsW(
            guid,
            null(),
            null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };
    if handle == INVALID_HANDLE_VALUE as HDEVINFO {
        return Err(std::io::Error::last_os_error());
    }
    let set = InterfaceSet(handle);
    let mut found = Vec::new();
    for index in 0..128 {
        let mut entry: SP_DEVICE_INTERFACE_DATA = unsafe { zeroed() };
        entry.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
        if unsafe { SetupDiEnumDeviceInterfaces(set.0, null(), guid, index, &mut entry) } == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(259) {
                break;
            } // ERROR_NO_MORE_ITEMS
            return Err(error);
        }
        let mut bytes = 0;
        unsafe {
            SetupDiGetDeviceInterfaceDetailW(set.0, &entry, null_mut(), 0, &mut bytes, null_mut());
        }
        let path_offset = offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
        if !(size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>()..=32768).contains(&(bytes as usize)) {
            continue;
        }
        let mut buffer = vec![0u32; (bytes as usize).div_ceil(4)];
        let detail = buffer
            .as_mut_ptr()
            .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
        buffer[0] = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        let ok = unsafe {
            SetupDiGetDeviceInterfaceDetailW(set.0, &entry, detail, bytes, null_mut(), null_mut())
        };
        if ok != 0 {
            // DevicePath is a flexible array within the output allocation.
            let chars = unsafe {
                std::slice::from_raw_parts(
                    detail.cast::<u8>().add(path_offset).cast::<u16>(),
                    (bytes as usize - path_offset) / 2,
                )
            };
            if let Some(end) = chars.iter().position(|&v| v == 0).filter(|&p| p > 0) {
                found.push(chars[..=end].to_vec());
            }
        }
    }
    Ok(found)
}

pub fn word(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}
pub fn dword(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}
pub fn request(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}
