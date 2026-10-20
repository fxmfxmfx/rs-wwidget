use super::common::{emit, settings};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_RESPONSE: usize = 256 * 1024;
const HEARTBEAT: Duration = Duration::from_secs(30);
const REFRESH: Duration = Duration::from_secs(600);
const LOCATION_TTL: u64 = 24 * 60 * 60;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Weather {
    temperature: f64,
    apparent: f64,
    rain: f64,
    chance: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Location {
    latitude: f64,
    longitude: f64,
    city: String,
    country: String,
    fetched: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WeatherCache {
    latitude: f64,
    longitude: f64,
    city: String,
    country: String,
    fetched: u64,
    weather: Weather,
}

fn parse_weather(v: &Value) -> Option<Weather> {
    let temp = v.pointer("/current/temperature_2m")?.as_f64()?;
    let apparent = v.pointer("/current/apparent_temperature")?.as_f64()?;
    let rain = v.pointer("/current/rain")?.as_f64()?;
    let chance = v
        .pointer("/hourly/precipitation_probability/0")
        .and_then(Value::as_f64);
    Some(Weather {
        temperature: temp,
        apparent,
        rain,
        chance,
    })
}

fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(std::env::temp_dir)
        .join("rs-wwidget")
}
fn state_path(name: &str) -> PathBuf {
    state_dir().join(name)
}
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn write_cache<T: Serialize>(path: &Path, value: &T) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    if let Ok(data) = serde_json::to_vec(value)
        && fs::write(&tmp, data).is_ok()
    {
        let _ = fs::rename(tmp, path);
    }
}

fn load_location(path: &Path) -> Option<Location> {
    let l: Location = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    (l.latitude.is_finite()
        && (-90.0..=90.0).contains(&l.latitude)
        && l.longitude.is_finite()
        && (-180.0..=180.0).contains(&l.longitude))
    .then_some(l)
}

fn read_bounded(mut child: Child) -> Result<Value, Box<dyn Error>> {
    let mut out = Vec::with_capacity(8192);
    let read_result = child
        .stdout
        .take()
        .ok_or("curl stdout unavailable")?
        .take((MAX_RESPONSE + 1) as u64)
        .read_to_end(&mut out);
    if read_result.is_err() || out.len() > MAX_RESPONSE {
        let _ = child.kill();
        let _ = child.wait();
        if out.len() > MAX_RESPONSE {
            return Err("HTTP response exceeds 256 KiB".into());
        }
        return Err("could not read HTTP response".into());
    }
    if !child.wait()?.success() {
        return Err("HTTP request failed".into());
    }
    Ok(serde_json::from_slice(&out)?)
}

fn request(url: &str) -> Result<Value, Box<dyn Error>> {
    let child = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "15",
            "--max-filesize",
            "262144",
            "--",
            url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    read_bounded(child)
}

fn locate() -> Result<Location, Box<dyn Error>> {
    let v = request("https://ipwho.is/")?;
    parse_location(&v).ok_or_else(|| "IP geolocation response was unsuccessful or invalid".into())
}

fn parse_location(v: &Value) -> Option<Location> {
    if v.get("success").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let l = Location {
        latitude: v.get("latitude")?.as_f64()?,
        longitude: v.get("longitude")?.as_f64()?,
        city: v
            .get("city")
            .and_then(Value::as_str)
            .unwrap_or("Unknown city")
            .chars()
            .take(128)
            .collect(),
        country: v
            .get("country")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .take(128)
            .collect(),
        fetched: unix_now(),
    };
    (l.latitude.is_finite()
        && (-90.0..=90.0).contains(&l.latitude)
        && l.longitude.is_finite()
        && (-180.0..=180.0).contains(&l.longitude))
    .then_some(l)
}

fn timestamp(epoch: u64) -> String {
    unsafe {
        let stamp = epoch as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&stamp, &mut tm).is_null() {
            return epoch.to_string();
        }
        let mut buf = [0i8; 24];
        if libc::strftime(buf.as_mut_ptr(), buf.len(), c"%H:%M".as_ptr(), &tm) == 0 {
            return epoch.to_string();
        }
        std::ffi::CStr::from_ptr(buf.as_ptr())
            .to_string_lossy()
            .into_owned()
    }
}

fn rows(weather: &Weather, fetched: u64, stale: bool, city: &str, country: &str) -> Vec<Value> {
    let mut result = vec![
        json!({"label":"Location", "value":if country.is_empty(){city.to_owned()}else{format!("{city}, {country}")}}),
        json!({"label":"Temperature", "value":format!("{:.1}°C", weather.temperature)}),
        json!({"label":"Feels like", "value":format!("{:.1}°C", weather.apparent)}),
        json!({"label":"Rain now", "value":format!("{:.1} mm", weather.rain)}),
        json!({"label":"Next hour", "value":weather.chance.map_or("Chance unavailable".into(), |p| format!("{p:.0}% rain"))}),
        json!({"label":"Updated", "value":if stale{format!("Stale · {}",timestamp(fetched))}else{timestamp(fetched)}}),
    ];
    result.truncate(12);
    result
}

fn sleep_heartbeat() {
    thread::sleep(HEARTBEAT);
}

pub fn run(package: &Path) -> Result<(), Box<dyn Error>> {
    let cfg = settings(package)?;
    let number = |key: &str| {
        cfg.get(key)
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|n| n as f64)))
    };
    let configured_lat = number("latitude");
    let configured_lon = number("longitude");
    if configured_lat.is_some() != configured_lon.is_some() {
        return Err(
            "configure both latitude and longitude, or leave both unset for IP auto-location"
                .into(),
        );
    }
    if let (Some(lat), Some(lon)) = (configured_lat, configured_lon)
        && (!lat.is_finite()
            || !(-90.0..=90.0).contains(&lat)
            || !lon.is_finite()
            || !(-180.0..=180.0).contains(&lon))
    {
        return Err("weather coordinates are out of range".into());
    }
    let configured_city = cfg
        .get("city")
        .and_then(toml::Value::as_str)
        .unwrap_or("Configured location")
        .chars()
        .take(128)
        .collect::<String>();
    let location_path = state_path("weather-location.json");
    let weather_path = state_path("weather.json");
    let mut location = load_location(&location_path);
    let mut cache: Option<WeatherCache> = fs::read(&weather_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .filter(|c: &WeatherCache| {
            c.weather.temperature.is_finite()
                && c.weather.apparent.is_finite()
                && c.weather.rain.is_finite()
                && c.weather.chance.is_none_or(f64::is_finite)
        });
    let mut next_location_attempt = Instant::now();
    let mut next_weather_attempt = Instant::now();

    loop {
        let (lat, lon, city, country, location_known) =
            if let (Some(lat), Some(lon)) = (configured_lat, configured_lon) {
                (lat, lon, configured_city.clone(), String::new(), true)
            } else {
                let expired = location
                    .as_ref()
                    .is_none_or(|l| unix_now().saturating_sub(l.fetched) >= LOCATION_TTL);
                if expired && Instant::now() >= next_location_attempt {
                    next_location_attempt = Instant::now() + REFRESH;
                    emit(
                        "Weather",
                        vec![json!({"label":"Status","value":"Locating…"})],
                    )?;
                    if let Ok(found) = locate() {
                        write_cache(&location_path, &found);
                        location = Some(found);
                    }
                }
                match &location {
                    Some(l) => (
                        l.latitude,
                        l.longitude,
                        l.city.clone(),
                        l.country.clone(),
                        true,
                    ),
                    None => (0.0, 0.0, String::new(), String::new(), false),
                }
            };

        if !location_known {
            emit(
                "Weather",
                vec![
                    json!({"label":"Status","value":"Location unavailable; retrying in 10 minutes"}),
                ],
            )?;
            sleep_heartbeat();
            continue;
        }

        if Instant::now() >= next_weather_attempt {
            next_weather_attempt = Instant::now() + REFRESH;
            let matching_before_fetch = cache.as_ref().is_some_and(|c| {
                (c.latitude - lat).abs() < 1e-8 && (c.longitude - lon).abs() < 1e-8
            });
            if !matching_before_fetch {
                emit(
                    "Weather",
                    vec![
                        json!({"label":"Location","value":if country.is_empty(){city.clone()}else{format!("{city}, {country}")}}),
                        json!({"label":"Status","value":"Fetching weather…"}),
                    ],
                )?;
            }
            if let Ok(v) = request(&format!(
                "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,apparent_temperature,rain&hourly=precipitation_probability&forecast_hours=1&timezone=auto"
            )) && let Some(weather) = parse_weather(&v)
            {
                let fetched = unix_now();
                let next = WeatherCache {
                    latitude: lat,
                    longitude: lon,
                    city: city.clone(),
                    country: country.clone(),
                    fetched,
                    weather,
                };
                write_cache(&weather_path, &next);
                cache = Some(next);
            }
        }
        let matching = cache
            .as_ref()
            .filter(|c| (c.latitude - lat).abs() < 1e-8 && (c.longitude - lon).abs() < 1e-8);
        if let Some(c) = matching {
            let stale = unix_now().saturating_sub(c.fetched) >= REFRESH.as_secs();
            emit(
                "Weather",
                rows(&c.weather, c.fetched, stale, &city, &country),
            )?;
        } else {
            let mut rows = vec![
                json!({"label":"Location","value":if country.is_empty(){city.clone()}else{format!("{city}, {country}")}}),
                json!({"label":"Status","value":"Weather unavailable; retrying in 10 minutes"}),
            ];
            rows.truncate(12);
            emit("Weather", rows)?;
        }
        sleep_heartbeat();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_current_and_hourly_precipitation() {
        let v = json!({"current":{"temperature_2m":12.5,"apparent_temperature":10.0,"rain":0.1},"hourly":{"precipitation_probability":[35]}});
        assert_eq!(
            parse_weather(&v),
            Some(Weather {
                temperature: 12.5,
                apparent: 10.0,
                rain: 0.1,
                chance: Some(35.0)
            })
        );
    }
    #[test]
    fn location_success_requires_valid_coordinate_fields() {
        assert!(parse_location(&json!({"success":true,"latitude":42.0,"longitude":-71.0,"city":"Boston","country":"US"})).is_some());
        assert!(
            parse_location(
                &json!({"success":true,"latitude":91.0,"longitude":0.0,"city":"x","country":"y"})
            )
            .is_none()
        );
    }
}
