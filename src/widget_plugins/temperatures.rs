use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

fn sensor_kind(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    if lower.contains("k10temp")
        || lower.contains("coretemp")
        || lower.contains("cpu")
        || lower.contains("zenpower")
    {
        Some("CPU")
    } else if lower.contains("amdgpu") || lower.contains("nvidia") || lower.contains("gpu") {
        Some("GPU")
    } else {
        None
    }
}

fn has_fan_sensor(name: &str, kind: Option<&str>) -> bool {
    let lower = name.to_ascii_lowercase();
    kind.is_some()
        || lower.starts_with("nct")
        || lower.starts_with("it87")
        || lower.starts_with("it86")
        || lower.starts_with("it68")
}

fn read_number(path: &Path) -> Option<i64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn sensor_rows(hwmon_root: &Path) -> Vec<Value> {
    let mut cpu_temps: Vec<(String, f64)> = Vec::new();
    let mut gpu_temps: Vec<(String, f64)> = Vec::new();
    let mut fan_rows: Vec<Value> = Vec::new();
    let Ok(entries) = fs::read_dir(hwmon_root) else {
        return Vec::new();
    };
    let mut devices: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    devices.sort();
    for device in devices {
        let name = fs::read_to_string(device.join("name")).unwrap_or_default();
        let name = name.trim();
        let kind = sensor_kind(name);
        if kind.is_none() && !has_fan_sensor(name, None) {
            continue;
        }
        let Ok(files) = fs::read_dir(&device) else {
            continue;
        };
        let mut inputs: Vec<PathBuf> = files
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with("temp") && s.ends_with("_input"))
            })
            .collect();
        inputs.sort();
        if let Some(kind) = kind {
            for input in inputs {
                let Some(milli_c) = read_number(&input) else {
                    continue;
                };
                let stem = input
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("temp1_input");
                let prefix = stem.strip_suffix("_input").unwrap_or(stem);
                let label = fs::read_to_string(device.join(format!("{prefix}_label")))
                    .unwrap_or_else(|_| {
                        let index = prefix.strip_prefix("temp").unwrap_or("1");
                        format!("{kind} {index}")
                    });
                let label = label.trim();
                let value = milli_c as f64 / 1000.0;
                let label = if label.is_empty() {
                    format!("{kind} temperature")
                } else {
                    label.to_owned()
                };
                match kind {
                    "CPU" => cpu_temps.push((label, value)),
                    "GPU" => gpu_temps.push((label, value)),
                    _ => {}
                }
            }
        }
        if has_fan_sensor(name, kind)
            && let Ok(files) = fs::read_dir(&device)
        {
            let mut fans: Vec<PathBuf> = files
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.starts_with("fan") && s.ends_with("_input"))
                })
                .collect();
            fans.sort();
            for fan in fans {
                if let Some(rpm) = read_number(&fan) {
                    let stem = fan
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("fan1_input");
                    let prefix = stem.strip_suffix("_input").unwrap_or(stem);
                    let label = fs::read_to_string(device.join(format!("{prefix}_label")))
                        .ok()
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| format!("{} {}", kind.unwrap_or("Board"), prefix));
                    fan_rows.push(json!({"label":label,"value":format!("{rpm} RPM")}));
                }
            }
        }
    }
    let representative = |kind: &str, values: &mut Vec<(String, f64)>| {
        values
            .drain(..)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(label, value)| {
                json!({
                    "label": if label.to_ascii_lowercase().contains("package") { label } else { format!("{kind} max") },
                    "value": format!("{value:.1} °C"),
                    "fraction": (value / 100.0).clamp(0.0, 1.0)
                })
            })
    };
    let mut rows = Vec::with_capacity(6);
    if let Some(row) = representative("CPU", &mut cpu_temps) {
        rows.push(row);
    }
    if let Some(row) = representative("GPU", &mut gpu_temps) {
        rows.push(row);
    }
    fan_rows.sort_by(|a, b| a["label"].as_str().cmp(&b["label"].as_str()));
    rows.extend(fan_rows.into_iter().take(6 - rows.len()));
    rows
}

pub fn run(package: &Path) -> Result<(), Box<dyn Error>> {
    let settings = super::common::settings(package)?;
    let interval = super::common::integer(&settings, "poll_seconds", 5).clamp(1, 60) as u64;
    let sysfs_root = settings
        .get("sysfs_root")
        .and_then(toml::Value::as_str)
        .map(|p| super::common::data_path(package, p))
        .unwrap_or_else(|| Path::new("/sys").to_path_buf());
    loop {
        let mut rows = sensor_rows(&sysfs_root.join("class/hwmon"));
        if rows.is_empty() {
            rows.push(json!({"label":"Sensors","value":"Unavailable (no CPU/GPU hwmon sensors)"}));
        }
        super::common::emit("Temperatures", rows)?;
        thread::sleep(Duration::from_secs(interval));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "rs-wwidget-hwmon-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn selects_cpu_gpu_and_fan_sensors_from_fixture() {
        let root = temp_dir();
        let cpu = root.join("hwmon0");
        let gpu = root.join("hwmon1");
        fs::create_dir_all(&cpu).unwrap();
        fs::create_dir_all(&gpu).unwrap();
        fs::write(cpu.join("name"), "k10temp\n").unwrap();
        fs::write(cpu.join("temp1_input"), "47260\n").unwrap();
        fs::write(cpu.join("temp1_label"), "Tctl\n").unwrap();
        fs::write(cpu.join("fan1_input"), "900\n").unwrap();
        fs::write(gpu.join("name"), "amdgpu\n").unwrap();
        fs::write(gpu.join("temp1_input"), "61000\n").unwrap();
        let rows = sensor_rows(&root);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["value"], "47.3 °C");
        assert_eq!(rows[1]["value"], "61.0 °C");
        assert_eq!(rows[2]["value"], "900 RPM");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unrelated_hwmon_devices_are_ignored() {
        assert_eq!(sensor_kind("acpitz"), None);
        assert_eq!(sensor_kind("coretemp"), Some("CPU"));
        assert_eq!(sensor_kind("amdgpu"), Some("GPU"));
    }

    #[test]
    fn representative_temperatures_and_board_fans_fit_host_row_limit() {
        let root = temp_dir();
        let cpu = root.join("hwmon0");
        let board = root.join("hwmon1");
        fs::create_dir_all(&cpu).unwrap();
        fs::create_dir_all(&board).unwrap();
        fs::write(cpu.join("name"), "coretemp\n").unwrap();
        for i in 1..=16 {
            fs::write(
                cpu.join(format!("temp{i}_input")),
                format!("{}\n", 40000 + i * 1000),
            )
            .unwrap();
        }
        fs::write(board.join("name"), "nct6798\n").unwrap();
        for i in 1..=8 {
            fs::write(
                board.join(format!("fan{i}_input")),
                format!("{}\n", 800 + i),
            )
            .unwrap();
        }
        let rows = sensor_rows(&root);
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0]["label"], "CPU max");
        assert_eq!(rows[0]["value"], "56.0 °C");
        assert_eq!(rows[1]["value"], "801 RPM");
        fs::remove_dir_all(root).unwrap();
    }
}
