//! 进程树:构建 pid -> 父 pid 映射,并计算某进程的全部后代。
//!
//! 为什么需要它:前台窗口进程(例如终端)的**后代**(它启动的 shell、构建进程)
//! 同样属于"用户正在交互"。只保护前台进程本身会导致工具去节流用户正在用的命令行。

use std::collections::HashMap;
use windows::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

/// 父链向上追溯的最大深度,防止异常数据造成死循环。
const MAX_DEPTH: usize = 32;

/// 读取一次全系统 `pid -> 父 pid` 映射。
pub fn parent_map() -> HashMap<u32, u32> {
    let mut map = HashMap::new();
    unsafe {
        let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return map,
        };
        if snap == INVALID_HANDLE_VALUE {
            return map;
        }
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                map.insert(entry.th32ProcessID, entry.th32ParentProcessID);
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    map
}

/// 返回 `root` 的全部后代(不含 root 自身)。
pub fn descendants_of(root: u32) -> Vec<u32> {
    descendants_from(&parent_map(), root)
}

/// 纯函数版本:只依赖父映射,便于用构造数据做确定性测试。
pub fn descendants_from(parents: &HashMap<u32, u32>, root: u32) -> Vec<u32> {
    let mut out = Vec::new();
    for &pid in parents.keys() {
        if pid == root {
            continue;
        }
        let mut cur = pid;
        for _ in 0..MAX_DEPTH {
            match parents.get(&cur) {
                Some(&p) if p != 0 && p != cur => {
                    if p == root {
                        out.push(pid);
                        break;
                    }
                    cur = p;
                }
                _ => break,
            }
        }
    }
    out.sort_unstable();
    out
}

/// `root` 自身 + 全部后代,去重升序。
pub fn tree_of(root: u32) -> Vec<u32> {
    let mut v = vec![root];
    v.extend(descendants_of(root));
    v.sort_unstable();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(u32, u32)]) -> HashMap<u32, u32> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn finds_direct_children() {
        let m = map(&[(1, 0), (10, 1), (11, 1), (12, 2)]);
        assert_eq!(descendants_from(&m, 1), vec![10, 11]);
    }

    #[test]
    fn finds_grandchildren_transitively() {
        let m = map(&[(1, 0), (10, 1), (100, 10), (101, 100), (12, 2)]);
        assert_eq!(descendants_from(&m, 1), vec![10, 100, 101]);
    }

    #[test]
    fn root_is_excluded_from_descendants() {
        let m = map(&[(1, 0), (10, 1)]);
        assert!(!descendants_from(&m, 1).contains(&1));
    }

    #[test]
    fn missing_parent_does_not_loop_forever() {
        // 10 的父 999 不在表里;20 自引用
        let m = map(&[(1, 0), (10, 999), (20, 20)]);
        assert!(descendants_from(&m, 1).is_empty());
    }

    #[test]
    fn spawned_child_is_found_in_descendants() {
        let mut child = std::process::Command::new("cmd")
            .args(["/c", "ping", "-n", "10", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("spawn cmd");
        std::thread::sleep(std::time::Duration::from_millis(500));
        let kids = descendants_of(std::process::id());
        let pid = child.id();
        let _ = child.kill();
        let _ = child.wait();
        assert!(
            kids.contains(&pid),
            "直接子进程 pid {pid} 未出现在后代列表 {kids:?} 中"
        );
    }
}
