//! 策略配置的构造:把命令行参数转成 `PolicyConfig`。

use pressctl_core::metrics::PolicyConfig;

pub struct Args {
    pub json: bool,
    pub warn: f32,
    pub high: f32,
    pub critical: f32,
    pub whitelist: Vec<String>,
    pub kill_enabled: bool,
}

impl Args {
    pub fn to_policy(&self) -> PolicyConfig {
        PolicyConfig {
            warn_percent: self.warn,
            high_percent: self.high,
            critical_percent: self.critical,
            cpu_high_percent: 90.0,
            min_working_set_bytes: 200 * 1024 * 1024,
            cpu_throttle_floor: 0.25,
            kill_enabled: self.kill_enabled,
            whitelist: self.whitelist.iter().map(|s| s.to_lowercase()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(whitelist: Vec<String>, kill: bool) -> Args {
        Args {
            json: false,
            warn: 85.0,
            high: 92.0,
            critical: 97.0,
            whitelist,
            kill_enabled: kill,
        }
    }

    #[test]
    fn whitelist_is_lowercased() {
        let a = args(vec!["Node.EXE".into()], false);
        assert_eq!(a.to_policy().whitelist, vec!["node.exe".to_string()]);
    }

    #[test]
    fn kill_is_disabled_by_default() {
        assert!(!args(Vec::new(), false).to_policy().kill_enabled);
    }

    #[test]
    fn thresholds_are_passed_through() {
        let p = args(Vec::new(), false).to_policy();
        assert_eq!(p.warn_percent, 85.0);
        assert_eq!(p.high_percent, 92.0);
        assert_eq!(p.critical_percent, 97.0);
    }
}
