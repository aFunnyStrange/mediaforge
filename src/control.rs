//! Cooperative queue control and Windows child-process suspension.
use crate::{Error, Result};
use std::{
    process::Child,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

#[derive(Default)]
pub struct Control {
    pub cancelled: AtomicBool,
    pub paused: AtomicBool,
}
impl Control {
    pub fn wait_ready(&self) -> bool {
        while self.paused.load(Ordering::SeqCst) && !self.cancelled.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(50));
        }
        !self.cancelled.load(Ordering::SeqCst)
    }
}

#[cfg(windows)]
pub(crate) fn suspend(child: &Child, paused: bool) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    #[link(name = "ntdll")]
    extern "system" {
        fn NtSuspendProcess(process: *mut std::ffi::c_void) -> i32;
        fn NtResumeProcess(process: *mut std::ffi::c_void) -> i32;
    }
    // Only our owned FFmpeg child is affected. A single worker owns each suspend/resume pair.
    let status = unsafe {
        if paused {
            NtSuspendProcess(child.as_raw_handle())
        } else {
            NtResumeProcess(child.as_raw_handle())
        }
    };
    if status < 0 {
        return Err(Error::Invalid(format!(
            "Windows 暂停/恢复失败（NTSTATUS {status:#x}），已停止此任务。"
        )));
    }
    Ok(())
}
#[cfg(not(windows))]
pub(crate) fn suspend(_child: &Child, _paused: bool) -> Result<()> {
    Err(Error::Invalid("进程暂停目前仅支持 Windows".into()))
}

/// Windows closes this job on application exit, including crashes and forced termination.
#[cfg(windows)]
pub(crate) struct ProcessJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl ProcessJob {
    pub fn attach(child: &Child) -> Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            let job = Self(handle);
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&info) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle()) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(job)
        }
    }
}
#[cfg(windows)]
impl Drop for ProcessJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    #[test]
    #[ignore = "requires MEDIAFORGE_TEST_FFMPEG"]
    fn native_pause_resume_and_job_cleanup() {
        use std::os::windows::process::CommandExt;
        let exe = std::env::var_os("MEDIAFORGE_TEST_FFMPEG").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let progress = dir.path().join("progress.txt");
        let mut child = Command::new(exe)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-re",
                "-f",
                "lavfi",
                "-i",
                "sine=duration=10",
                "-progress",
            ])
            .arg(&progress)
            .args(["-f", "null", "-"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()
            .unwrap();
        let job = ProcessJob::attach(&child).unwrap();
        let start = std::time::Instant::now();
        while std::fs::metadata(&progress).map(|m| m.len()).unwrap_or(0) == 0 {
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(50));
        }
        suspend(&child, true).unwrap();
        thread::sleep(Duration::from_millis(200));
        let size = std::fs::metadata(&progress).unwrap().len();
        thread::sleep(Duration::from_millis(800));
        assert_eq!(
            std::fs::metadata(&progress).unwrap().len(),
            size,
            "paused FFmpeg must stop progressing"
        );
        assert!(child.try_wait().unwrap().is_none());
        suspend(&child, false).unwrap();
        thread::sleep(Duration::from_millis(800));
        assert!(std::fs::metadata(&progress).unwrap().len() > size);
        suspend(&child, true).unwrap();
        assert!(child.try_wait().unwrap().is_none());
        let stopped = std::time::Instant::now();
        drop(job); // Even a paused child must die when the owning job is closed.
        child.wait().unwrap(); // Job-object termination may report exit code zero.
        assert!(stopped.elapsed() < Duration::from_secs(2));
    }
}
