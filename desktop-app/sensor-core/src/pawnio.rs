//! Minimal read-only client for the PawnIO 2.2 driver protocol.
//!
//! This module belongs in the privileged app-owned sensor process. The Tauri
//! webview and ordinary GUI must never receive a driver handle or raw IOCTL API.

use std::ffi::c_void;
use std::io;
use std::os::windows::io::{FromRawHandle, OwnedHandle};

use crate::intel_package_celsius;

const DEVICE_TYPE: u32 = 41394;
#[cfg(target_arch = "x86_64")]
const IOCTL_LOAD_BINARY: u32 = (DEVICE_TYPE << 16) | (0x821 << 2);
const IOCTL_EXECUTE_FN: u32 = (DEVICE_TYPE << 16) | (0x841 << 2);
#[cfg(target_arch = "x86_64")]
const IOCTL_VERSION: u32 = (DEVICE_TYPE << 16) | (0x861 << 2);
#[cfg(target_arch = "x86_64")]
const EXPECTED_VERSION: u32 = (2 << 16) | (2 << 8);
#[cfg(target_arch = "x86_64")]
const INTEL_MSR_MODULE: &[u8] =
    include_bytes!("../../src-tauri/resources/pawnio/modules/IntelMSR.bin");

const IA32_TEMPERATURE_TARGET: u64 = 0x1a2;
const IA32_PACKAGE_THERM_STATUS: u64 = 0x1b1;

#[link(name = "kernel32")]
extern "system" {
    #[cfg(target_arch = "x86_64")]
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const c_void,
        disposition: u32,
        flags: u32,
        template: *mut c_void,
    ) -> *mut c_void;
    fn DeviceIoControl(
        handle: *mut c_void,
        control: u32,
        input: *const c_void,
        input_len: u32,
        output: *mut c_void,
        output_len: u32,
        returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
}

pub struct IntelReader {
    handle: OwnedHandle,
}

impl IntelReader {
    #[cfg(target_arch = "x86_64")]
    pub fn open() -> io::Result<Self> {
        if !is_intel_family_six() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "unsupported CPU",
            ));
        }

        let path: Vec<u16> = "\\\\.\\PawnIO".encode_utf16().chain(Some(0)).collect();
        let raw = unsafe {
            CreateFileW(
                path.as_ptr(),
                0x8000_0000 | 0x4000_0000,
                0x1 | 0x2 | 0x4,
                std::ptr::null(),
                3,
                0,
                std::ptr::null_mut(),
            )
        };
        if raw == (-1isize as *mut c_void) {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut reader = Self { handle };
        let mut version = [0u8; 4];
        reader.call(IOCTL_VERSION, &[], &mut version)?;
        if u32::from_le_bytes(version) != EXPECTED_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "incompatible PawnIO driver version",
            ));
        }
        reader.call(IOCTL_LOAD_BINARY, INTEL_MSR_MODULE, &mut [])?;
        Ok(reader)
    }

    #[cfg(not(target_arch = "x86_64"))]
    pub fn open() -> io::Result<Self> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Intel x64 only"))
    }

    pub fn package_celsius(&mut self) -> io::Result<Option<f32>> {
        let target = self.read_msr(IA32_TEMPERATURE_TARGET)?;
        let status = self.read_msr(IA32_PACKAGE_THERM_STATUS)?;
        Ok(intel_package_celsius(target, status))
    }

    fn read_msr(&mut self, index: u64) -> io::Result<u64> {
        let mut request = [0u8; 40];
        request[..14].copy_from_slice(b"ioctl_read_msr");
        request[32..].copy_from_slice(&index.to_le_bytes());
        let mut result = [0u8; 8];
        self.call(IOCTL_EXECUTE_FN, &request, &mut result)?;
        Ok(u64::from_le_bytes(result))
    }

    fn call(&mut self, code: u32, input: &[u8], output: &mut [u8]) -> io::Result<()> {
        let input_len = u32::try_from(input.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "request too large"))?;
        let output_len = u32::try_from(output.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "response too large"))?;
        let mut returned = 0u32;
        let success = unsafe {
            DeviceIoControl(
                self.handle.as_raw_handle(),
                code,
                input.as_ptr().cast(),
                input_len,
                output.as_mut_ptr().cast(),
                output_len,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if success == 0 {
            return Err(io::Error::last_os_error());
        }
        if returned != output_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "wrong response size",
            ));
        }
        Ok(())
    }
}

#[cfg(target_arch = "x86_64")]
fn is_intel_family_six() -> bool {
    let vendor = unsafe { std::arch::x86_64::__cpuid(0) };
    if (vendor.ebx, vendor.edx, vendor.ecx) != (0x756e6547, 0x49656e69, 0x6c65746e) {
        return false;
    }
    let info = unsafe { std::arch::x86_64::__cpuid(1) };
    ((info.eax >> 8) & 0xf) == 6
}

use std::os::windows::io::AsRawHandle;
