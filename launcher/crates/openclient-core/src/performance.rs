use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sysinfo::System;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HardwareProfile {
    pub installed_ram_mb: u64,
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub recommended_ram_mb: u64,
    pub max_ram_mb: u64,
    pub cpu_brand: String,
    pub cpu_cores: usize,
    pub recommended_gc: String,
    pub os_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct GodOptimizationConfig {
    pub allocated_ram_mb: u64,
    pub gc_preset: String,
    pub hibernate_launcher: bool,
    pub high_process_priority: bool,
    pub dedicated_gpu_force: bool,
    pub auto_sodium_stack: bool,
    pub pre_touch_memory: bool,
    pub minimize_on_launch: bool,
    pub refocus_on_exit: bool,
    pub window_width: u32,
    pub window_height: u32,
    pub fullscreen: bool,
    pub custom_jvm_args: String,
    pub java_paths: HashMap<String, String>,
}
impl Default for GodOptimizationConfig {
    fn default() -> Self {
        Self {
            allocated_ram_mb: 2048,
            gc_preset: "automatic".into(),
            hibernate_launcher: false,
            high_process_priority: false,
            dedicated_gpu_force: false,
            auto_sodium_stack: true,
            pre_touch_memory: false,
            minimize_on_launch: false,
            refocus_on_exit: false,
            window_width: 0,
            window_height: 0,
            fullscreen: false,
            custom_jvm_args: String::new(),
            java_paths: HashMap::new(),
        }
    }
}

pub fn installed_ram_mb(usable_mb: u64) -> u64 {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetPhysicallyInstalledSystemMemory(total_kb: *mut u64) -> i32;
        }
        let mut total_kb = 0;
        if unsafe { GetPhysicallyInstalledSystemMemory(&mut total_kb) } != 0 && total_kb > 0 {
            return (total_kb / 1024).max(usable_mb);
        }
    }
    usable_mb
}

pub fn validate_ram(requested: u64, installed: u64) -> Result<u64, String> {
    if requested < 1024 || requested > installed {
        return Err(format!(
            "Elige entre 1024 MB y {} MB de RAM instalada para Minecraft.",
            installed
        ));
    }
    Ok(requested)
}

pub fn detect_system_hardware() -> HardwareProfile {
    let mut sys = System::new_all();
    sys.refresh_all();
    let total = sys.total_memory() / (1024 * 1024);
    let installed = installed_ram_mb(total);
    let available = sys.available_memory() / (1024 * 1024);
    let target = if installed >= 16384 {
        6144
    } else if installed >= 8192 {
        4096
    } else {
        2048
    };
    HardwareProfile {
        installed_ram_mb: installed,
        total_ram_mb: total,
        available_ram_mb: available,
        recommended_ram_mb: target.min(installed),
        max_ram_mb: installed,
        cpu_brand: sys
            .cpus()
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_else(|| "Procesador".into()),
        cpu_cores: sys.cpus().len(),
        recommended_gc: "automatic".into(),
        os_name: System::name().unwrap_or_else(|| "Windows".into()),
    }
}

pub fn build_god_jvm_flags(config: GodOptimizationConfig, java_version: u8) -> Vec<String> {
    let ram = config.allocated_ram_mb.max(1024);
    let initial = if config.gc_preset == "low_memory" {
        512
    } else {
        1024
    };
    let mut flags = vec![
        format!("-Xms{}M", initial.min(ram)),
        format!("-Xmx{}M", ram),
    ];
    if config.pre_touch_memory {
        flags.push("-XX:+AlwaysPreTouch".into());
    }
    match config.gc_preset.as_str() {
        "generational_zgc" if java_version >= 21 => {
            flags.push("-XX:+UseZGC".into());
            if java_version < 24 {
                flags.push("-XX:+ZGenerational".into());
            }
        }
        "low_memory" => flags.extend(["-XX:+UseG1GC".into(), "-XX:+UseStringDeduplication".into()]),
        "automatic" => {}
        _ => flags.extend([
            "-XX:+UseG1GC".into(),
            "-XX:+ParallelRefProcEnabled".into(),
            "-XX:MaxGCPauseMillis=200".into(),
        ]),
    }
    for token in config.custom_jvm_args.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if token.starts_with('-')
            && !lower.contains("formatmsgnolookups")
            && !lower.contains("log4j.configurationfile")
            && !token.chars().any(char::is_control)
        {
            flags.push(token.to_owned());
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_ram_is_preserved_up_to_installed_memory() {
        assert_eq!(validate_ram(8192, 16384).unwrap(), 8192);
        assert_eq!(validate_ram(16384, 16384).unwrap(), 16384);
        assert!(validate_ram(17408, 16384).is_err());
        assert!(validate_ram(512, 16384).is_err());
    }
    #[test]
    fn flags_match_actual_java_and_options() {
        let mut config = GodOptimizationConfig::default();
        assert_eq!(
            build_god_jvm_flags(config.clone(), 25),
            vec!["-Xms1024M", "-Xmx2048M"]
        );
        config.gc_preset = "generational_zgc".into();
        assert!(build_god_jvm_flags(config.clone(), 21).contains(&"-XX:+ZGenerational".into()));
        assert!(!build_god_jvm_flags(config.clone(), 25).contains(&"-XX:+ZGenerational".into()));
        assert!(build_god_jvm_flags(config.clone(), 17).contains(&"-XX:+UseG1GC".into()));
        config.pre_touch_memory = true;
        config.gc_preset = "low_memory".into();
        let flags = build_god_jvm_flags(config, 8);
        assert!(flags.contains(&"-Xms512M".into()));
        assert!(flags.contains(&"-XX:+AlwaysPreTouch".into()));
    }
}
