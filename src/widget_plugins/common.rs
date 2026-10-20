use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub fn emit(title: &str, mut rows: Vec<serde_json::Value>) -> Result<(), Box<dyn Error>> {
    rows.truncate(12);
    for row in &mut rows {
        for key in ["label", "value"] {
            if let Some(value) = row.get_mut(key) {
                *value = serde_json::Value::String(
                    value.as_str().unwrap_or("").chars().take(512).collect(),
                );
            }
        }
    }
    let mut frame = serde_json::json!({"version":1,"title":title.chars().take(128).collect::<String>(),"rows":rows});
    let bytes = loop {
        let bytes = serde_json::to_vec(&frame)?;
        if bytes.len() <= 65_536 {
            break bytes;
        }
        frame["rows"].as_array_mut().unwrap().pop();
    };
    let mut stdout = io::stdout().lock();
    stdout.write_all(&bytes)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

pub fn settings(package: &Path) -> Result<toml::Table, Box<dyn Error>> {
    match fs::read_to_string(package.join("settings.toml")) {
        Ok(text) => Ok(toml::from_str(&text)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(toml::Table::new()),
        Err(e) => Err(e.into()),
    }
}

pub fn string(settings: &toml::Table, key: &str, default: &str) -> String {
    settings
        .get(key)
        .and_then(toml::Value::as_str)
        .unwrap_or(default)
        .into()
}

pub fn integer(settings: &toml::Table, key: &str, default: i64) -> i64 {
    settings
        .get(key)
        .and_then(toml::Value::as_integer)
        .unwrap_or(default)
}

pub fn data_path(package: &Path, path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(rest);
    }
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        package.join(path)
    }
}
