use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use winreg::{enums::*, RegKey};

pub const COMPONENTS: [&str; 7] = [
    "CPU", "GPU", "Memory", "Storage", "Cooling", "Battery", "Network",
];
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub interval: u64,
    pub retention: u64,
    pub cooldown: u64,
    pub notifications: bool,
    pub tray: bool,
    pub startup: bool,
    pub theme: String,
    pub accent: String,
    pub fahrenheit: bool,
    pub weights: [u32; 7],
    pub cpu_limit: f64,
    pub memory_limit: f64,
    pub cpu_temp: f64,
    pub gpu_temp: f64,
    pub disk_free: f64,
    pub battery_health: f64,
    pub duration: u64,
    pub low_resource: bool,
    pub benchmark_limit: f64,
    pub bench_seconds: u64,
    pub connectivity_target: String,
    pub offline: bool,
    pub widget_order: Vec<String>,
    pub hidden_widgets: Vec<String>,
    pub wide_widgets: Vec<String>,
    pub chart_refresh: u64,
    pub ai: bool,
    pub model: String,
    pub database_dir: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            interval: 2,
            retention: 30,
            cooldown: 30,
            notifications: false,
            tray: true,
            startup: false,
            theme: "dark".into(),
            accent: "mint".into(),
            fahrenheit: false,
            weights: [20, 15, 15, 20, 15, 10, 5],
            cpu_limit: 90.,
            memory_limit: 90.,
            cpu_temp: 85.,
            gpu_temp: 85.,
            disk_free: 10.,
            battery_health: 70.,
            duration: 12,
            low_resource: false,
            benchmark_limit: 90.,
            bench_seconds: 3,
            connectivity_target: String::new(),
            offline: true,
            widget_order: vec![],
            hidden_widgets: vec![],
            wide_widgets: vec![],
            chart_refresh: 2,
            ai: false,
            model: "llama3.2".into(),
            database_dir: String::new(),
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=60).contains(&self.interval)
            || !(1..=365).contains(&self.retention)
            || !(1..=1440).contains(&self.cooldown)
        {
            return Err("Sampling, retention or cooldown is outside the supported range.".into());
        }
        if !(0..=600).contains(&self.duration)
            || !(1..=60).contains(&self.chart_refresh)
            || !(1..=20).contains(&self.bench_seconds)
            || !(40.0..=110.0).contains(&self.benchmark_limit)
        {
            return Err("Invalid monitoring duration, chart interval or benchmark limit.".into());
        }
        if !self.connectivity_target.is_empty()
            && self
                .connectivity_target
                .parse::<std::net::IpAddr>()
                .is_err()
        {
            return Err("Connectivity target must be an IPv4 or IPv6 address.".into());
        }
        for items in [&self.widget_order, &self.hidden_widgets, &self.wide_widgets] {
            if items.len() > 10
                || items.iter().any(|v| {
                    ![
                        "health", "cpu", "gpu", "memory", "storage", "battery", "network", "chart",
                        "sensors", "alerts",
                    ]
                    .contains(&v.as_str())
                })
            {
                return Err("Invalid dashboard layout.".into());
            }
        }
        if self.weights.iter().all(|v| *v == 0) || self.weights.iter().any(|v| *v > 100) {
            return Err("Set at least one positive health weight, up to 100 per component.".into());
        }
        if [
            self.cpu_limit,
            self.memory_limit,
            self.disk_free,
            self.battery_health,
        ]
        .iter()
        .any(|v| !(1.0..=100.0).contains(v))
            || [self.cpu_temp, self.gpu_temp]
                .iter()
                .any(|v| !(20.0..=120.0).contains(v))
        {
            return Err("Check the percentage and temperature thresholds.".into());
        }
        if !["light", "dark", "system"].contains(&self.theme.as_str())
            || !["mint", "blue", "violet"].contains(&self.accent.as_str())
            || self.model.trim().is_empty()
            || self.model.len() > 160
            || self.model.ends_with(":cloud")
            || self.model.ends_with("-cloud")
        {
            return Err("Choose a supported theme, accent and local model.".into());
        }
        Ok(())
    }
}
pub fn read(path: &Path) -> Result<Config, String> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let c: Config = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("Invalid settings: {e}"))?;
    c.validate()?;
    Ok(c)
}
pub fn save(path: &Path, c: &Config) -> Result<(), String> {
    use std::io::Write;
    let mut tmp =
        tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(|e| e.to_string())?;
    tmp.write_all(&serde_json::to_vec_pretty(c).unwrap())
        .map_err(|e| e.to_string())?;
    tmp.as_file().sync_all().map_err(|e| e.to_string())?;
    tmp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn autorun(enabled: bool) -> Result<(), String> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
        .map_err(|e| e.to_string())?;
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        key.set_value("SignalDesk", &format!("\"{}\" --background", exe.display()))
            .map_err(|e| e.to_string())
    } else {
        match key.delete_value("SignalDesk") {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }
}
