//! Read-only check for the Windows service that overrides the mouse LEDs
//! (Dynamic Lighting through Logitech's LampArray service). Never starts,
//! stops or reconfigures it.

pub const LAMPARRAY_SERVICE: &str = "logi_lamparray_service";

#[cfg(windows)]
pub fn service_running(name: &str) -> bool {
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_MANAGER_CONNECT,
        SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS,
    };
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    unsafe {
        let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return false;
        }
        let svc = OpenServiceW(scm, wide.as_ptr(), SERVICE_QUERY_STATUS);
        let mut running = false;
        if !svc.is_null() {
            let mut st: SERVICE_STATUS = std::mem::zeroed();
            if QueryServiceStatus(svc, &mut st) != 0 {
                running = st.dwCurrentState == SERVICE_RUNNING;
            }
            CloseServiceHandle(svc);
        }
        CloseServiceHandle(scm);
        running
    }
}

#[cfg(not(windows))]
pub fn service_running(_: &str) -> bool {
    false
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn missing_service_is_not_running() {
        assert!(!super::service_running("mouseforge_no_such_service"));
    }
}
