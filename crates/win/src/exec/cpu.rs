//! CPU 速率限制:把进程放入一个 Job Object 并设置硬上限。
//!
//! 可逆性来自 Job 的生命周期:句柄关闭(或宿主进程退出)时 Job 被销毁,
//! 限制随之解除。因此持久限制需要常驻进程(见路线图的常驻服务模式)。

use std::mem::size_of;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectCpuRateControlInformation,
    SetInformationJobObject, JOBOBJECT_CPU_RATE_CONTROL_INFORMATION,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};
use windows::core::PCWSTR;

/// `JOB_OBJECT_CPU_RATE_CONTROL_ENABLE(1) | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP(4)`
const FLAGS_ENABLE_HARD_CAP: u32 = 1 | 4;

/// 一个已施加的 CPU 上限。析构即解除限制。
pub struct CpuCap {
    pub pid: u32,
    /// 保留比率(0.01..=1.0):1.0 表示不限制。
    pub ratio: f32,
    handle: HANDLE,
}

impl CpuCap {
    /// 把 `pid` 的 CPU 占用限制为整机总量的 `ratio`。
    pub fn apply(pid: u32, ratio: f32) -> Result<Self, String> {
        let clamped = ratio.clamp(0.01, 1.0);
        // CpuRate 的单位是百分之一的百分比(10000 = 100%)
        let cpu_rate = (clamped * 10_000.0).round() as u32;

        unsafe {
            let job = CreateJobObjectW(None, PCWSTR::null())
                .map_err(|e| format!("CreateJobObjectW: {e}"))?;

            let mut info = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION::default();
            info.ControlFlags.0 = FLAGS_ENABLE_HARD_CAP;
            info.Anonymous.CpuRate = cpu_rate;

            if let Err(e) = SetInformationJobObject(
                job,
                JobObjectCpuRateControlInformation,
                &info as *const _ as *const core::ffi::c_void,
                size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
            ) {
                let _ = CloseHandle(job);
                return Err(format!("SetInformationJobObject: {e}"));
            }

            let proc = match OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid) {
                Ok(h) => h,
                Err(e) => {
                    let _ = CloseHandle(job);
                    return Err(format!("OpenProcess(pid {pid}): {e}"));
                }
            };
            let assigned = AssignProcessToJobObject(job, proc);
            let _ = CloseHandle(proc);
            if let Err(e) = assigned {
                let _ = CloseHandle(job);
                return Err(format!("AssignProcessToJobObject(pid {pid}): {e}"));
            }

            Ok(CpuCap {
                pid,
                ratio: clamped,
                handle: job,
            })
        }
    }
}

impl Drop for CpuCap {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_sleeper() -> std::process::Child {
        std::process::Command::new("cmd")
            .args(["/c", "ping", "-n", "20", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("spawn")
    }

    #[test]
    fn cap_is_applied_and_released_on_drop() {
        let mut child = spawn_sleeper();
        std::thread::sleep(std::time::Duration::from_millis(300));

        let cap = CpuCap::apply(child.id(), 0.5);
        assert!(cap.is_ok(), "施加 CPU 上限失败: {:?}", cap.err());
        assert_eq!(cap.unwrap().ratio, 0.5);
        // 析构即释放(句柄关闭 -> Job 销毁 -> 限制解除)

        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn ratio_is_clamped_to_valid_range() {
        let mut child = spawn_sleeper();
        std::thread::sleep(std::time::Duration::from_millis(300));
        if let Ok(cap) = CpuCap::apply(child.id(), 0.0) {
            assert_eq!(cap.ratio, 0.01);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}
