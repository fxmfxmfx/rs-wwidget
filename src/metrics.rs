use std::{
    ffi::CString,
    fs::File,
    io::{BufRead, BufReader, Read},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
}
impl Date {
    pub fn now() -> Self {
        // libc owns timezone conversion; tm is stack allocated and initialized by localtime_r.
        unsafe {
            let timestamp = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            libc::localtime_r(&timestamp, &mut tm);
            Self {
                year: tm.tm_year + 1900,
                month: (tm.tm_mon + 1) as u32,
                day: tm.tm_mday as u32,
                weekday: tm.tm_wday as u32,
                hour: tm.tm_hour as u32,
                minute: tm.tm_min as u32,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Display {
    pub date: Date,
    pub cpu: Option<f64>,
    pub memory: Option<(f64, f64)>,
    pub disk: Option<(f64, f64)>,
}
impl Display {
    pub fn changes(self, after: Self) -> [bool; 3] {
        let date = self.date.year != after.date.year
            || self.date.month != after.date.month
            || self.date.day != after.date.day;
        [
            self.date != after.date,
            date,
            self.cpu != after.cpu || self.memory != after.memory || self.disk != after.disk,
        ]
    }
}

pub fn month_days(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => 28 + u32::from(year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)),
        _ => 0,
    }
}
pub fn month_start(year: i32, month: u32) -> u32 {
    // Gregorian weekday arithmetic: Monday = 0, independent of local timezone/DST.
    let y = i64::from(year) - i64::from(month < 3);
    let offsets = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    ((y + y.div_euclid(4) - y.div_euclid(100)
        + y.div_euclid(400)
        + offsets[month.clamp(1, 12) as usize - 1]
        + 6
        + 1)
    .rem_euclid(7)) as u32
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CpuSample {
    pub total: u64,
    pub idle: u64,
}
pub fn parse_cpu(line: &str) -> Option<CpuSample> {
    let mut fields = line.split_ascii_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }
    let mut parts = [0u64; 8];
    let mut count = 0;
    for (target, field) in parts.iter_mut().zip(fields) {
        *target = field.parse().ok()?;
        count += 1;
    }
    if count < 4 {
        return None;
    }
    Some(CpuSample {
        total: parts.into_iter().try_fold(0u64, |a, b| a.checked_add(b))?,
        idle: parts[3].checked_add(parts[4])?,
    })
}
pub fn cpu_percent(old: CpuSample, now: CpuSample) -> f64 {
    let Some(total) = now.total.checked_sub(old.total).filter(|n| *n > 0) else {
        return 0.0;
    };
    let Some(idle) = now.idle.checked_sub(old.idle).filter(|n| *n <= total) else {
        return 0.0;
    };
    100.0 * (total - idle) as f64 / total as f64
}
pub fn parse_memory(text: &str) -> Option<(f64, f64)> {
    let (mut total, mut available) = (None, None);
    for line in text.lines() {
        let mut f = line.split_ascii_whitespace();
        match f.next()? {
            "MemTotal:" => total = f.next()?.parse::<u64>().ok(),
            "MemAvailable:" => available = f.next()?.parse::<u64>().ok(),
            _ => {}
        }
    }
    let (total, available) = (total?, available?);
    if total == 0 || available > total {
        return None;
    }
    Some((
        (total - available) as f64 / 1048576.0,
        total as f64 / 1048576.0,
    ))
}

#[derive(Default)]
pub struct Sampler {
    previous: Option<CpuSample>,
    scratch: String,
}
impl Sampler {
    pub fn sample(&mut self) -> Display {
        let mut line = [0u8; 256];
        // Read only the aggregate CPU line, never all per-core statistics.
        let cpu = File::open("/proc/stat")
            .ok()
            .and_then(|mut f| f.read(&mut line).ok())
            .and_then(|n| {
                let end = line[..n].iter().position(|b| *b == b'\n')?;
                parse_cpu(std::str::from_utf8(&line[..end]).ok()?)
            });
        let percent = cpu.and_then(|current| self.previous.map(|old| cpu_percent(old, current)));
        self.previous = cpu.or(self.previous);
        self.scratch.clear();
        let memory = File::open("/proc/meminfo")
            .ok()
            .and_then(|mut f| f.read_to_string(&mut self.scratch).ok())
            .and_then(|_| parse_memory(&self.scratch));
        let home = CString::new(crate::config::home().as_os_str().as_encoded_bytes()).ok();
        let disk = home.and_then(|home| unsafe {
            let mut fs: libc::statvfs = std::mem::zeroed();
            if libc::statvfs(home.as_ptr(), &mut fs) != 0 || fs.f_blocks == 0 {
                return None;
            }
            let used = fs.f_blocks.saturating_sub(fs.f_bfree) as f64;
            let available = fs.f_bavail as f64;
            Some((
                available * fs.f_frsize as f64 / 1073741824.0,
                if used + available > 0.0 {
                    used / (used + available)
                } else {
                    0.0
                },
            ))
        });
        Display {
            date: Date::now(),
            cpu: percent,
            memory,
            disk,
        }
    }
}

pub fn memory_usage() -> (u64, u64) {
    let mut values = (0, 0);
    if let Ok(f) = File::open("/proc/self/smaps_rollup") {
        for line in BufReader::new(f).lines().map_while(Result::ok) {
            if let Some(v) = line.strip_prefix("Rss:") {
                values.0 = v
                    .split_ascii_whitespace()
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
            }
            if let Some(v) = line.strip_prefix("Pss:") {
                values.1 = v
                    .split_ascii_whitespace()
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
            }
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar() {
        assert_eq!(month_days(2024, 2), 29);
        assert_eq!(month_days(1900, 2), 28);
        assert_eq!(month_days(2000, 2), 29);
        assert_eq!(month_start(2026, 10), 3);
        assert_eq!(month_start(2024, 2), 3);
        assert_eq!(month_start(2024, 1), 0);
    }
    #[test]
    fn cpu_guests_overflow_and_reset() {
        assert_eq!(
            parse_cpu("cpu 10 2 3 40 5 6 7 8 999"),
            Some(CpuSample {
                total: 81,
                idle: 45
            })
        );
        for s in [
            "cpu0 1 2 3 4",
            "cpu 1 2",
            "cpu -1 2 3 4",
            "cpu 18446744073709551615 1 0 0",
        ] {
            assert!(parse_cpu(s).is_none());
        }
        let old = CpuSample {
            total: 100,
            idle: 60,
        };
        assert_eq!(
            cpu_percent(
                old,
                CpuSample {
                    total: 200,
                    idle: 80
                }
            ),
            80.0
        );
        assert_eq!(
            cpu_percent(
                old,
                CpuSample {
                    total: 50,
                    idle: 30
                }
            ),
            0.0
        );
        assert_eq!(
            cpu_percent(
                old,
                CpuSample {
                    total: 110,
                    idle: 90
                }
            ),
            0.0
        );
    }
    #[test]
    fn memory_unavailable() {
        assert_eq!(
            parse_memory("MemTotal: 2097152 kB\nMemAvailable: 1048576 kB\n"),
            Some((1.0, 2.0))
        );
        assert!(parse_memory("MemTotal: 42 kB").is_none());
        assert!(parse_memory("MemTotal: 42 kB\nMemAvailable: 100 kB").is_none());
    }
    #[test]
    fn refresh_only_visible_changes() {
        let mut a = Display {
            date: Date {
                year: 2026,
                month: 10,
                day: 19,
                ..Date::default()
            },
            ..Display::default()
        };
        let b = a;
        assert_eq!(a.changes(b), [false; 3]);
        a.date.minute = 1;
        assert_eq!(b.changes(a), [true, false, false]);
        a.cpu = Some(10.0);
        assert_eq!(b.changes(a), [true, false, true]);
        a.date.day = 20;
        assert_eq!(b.changes(a), [true, true, true]);
    }
}
