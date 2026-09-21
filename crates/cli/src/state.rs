//! 崩溃恢复用的状态文件。
//!
//! CPU 上限随本进程退出自动解除,但**被冻结的进程不会**。因此每次冻结都记盘,
//! 正常释放时清除;若本工具被强杀(`taskkill /F`),可用 `pressctl --release` 恢复。

use std::path::{Path, PathBuf};

/// 默认状态文件位置。
pub fn default_path() -> PathBuf {
    std::env::temp_dir().join("pressctl-frozen.txt")
}

/// 覆盖写入被冻结的 pid 列表。
pub fn write(frozen: &[u32]) -> Result<(), String> {
    write_at(&default_path(), frozen)
}

/// 读取被冻结的 pid 列表(文件不存在时返回空)。
pub fn read() -> Vec<u32> {
    read_at(&default_path())
}

/// 删除状态文件(全部已释放时调用)。
pub fn clear() {
    let _ = std::fs::remove_file(default_path());
}

/// 指定路径的写入。路径参数化以便测试互不干扰(测试并行执行)。
pub fn write_at(path: &Path, frozen: &[u32]) -> Result<(), String> {
    let body = frozen
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(path, body).map_err(|e| format!("写状态文件失败: {e}"))
}

/// 指定路径的读取。
pub fn read_at(path: &Path) -> Vec<u32> {
    std::fs::read_to_string(path)
        .map(|s| {
            s.lines()
                .filter_map(|l| l.trim().parse::<u32>().ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pressctl-test-{name}-{}.txt", std::process::id()))
    }

    #[test]
    fn write_read_clear_roundtrip() {
        let p = tmp("roundtrip");
        write_at(&p, &[1234, 5678]).expect("write");
        assert_eq!(read_at(&p), vec![1234, 5678]);
        write_at(&p, &[]).expect("write empty");
        assert!(read_at(&p).is_empty());
        let _ = std::fs::remove_file(&p);
        assert!(read_at(&p).is_empty(), "不存在的文件应返回空");
    }

    #[test]
    fn read_ignores_malformed_lines() {
        let p = tmp("malformed");
        std::fs::write(&p, "42\nnot-a-pid\n\n7\n").expect("write");
        assert_eq!(read_at(&p), vec![42, 7]);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn default_path_is_stable() {
        assert!(default_path().to_string_lossy().contains("pressctl-frozen"));
    }
}
