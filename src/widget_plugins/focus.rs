//! Native protocol-v2 countdown; one process/thread, monotonic time, no renderer.
use super::{common, events::Events};
use serde_json::{Value, json};
use std::{
    error::Error,
    io::{self, Write},
    path::Path,
    time::{Duration, Instant},
};

struct Timer {
    total: Duration,
    remaining: Duration,
    end: Option<Instant>,
}
impl Timer {
    fn new(seconds: u64) -> Self {
        let total = Duration::from_secs(seconds.clamp(1, 86_400));
        Self {
            total,
            remaining: total,
            end: None,
        }
    }
    fn update(&mut self, now: Instant) {
        if let Some(end) = self.end {
            self.remaining = end.saturating_duration_since(now);
            if self.remaining.is_zero() {
                self.end = None;
            }
        }
    }
    fn toggle(&mut self, now: Instant) {
        self.update(now);
        if self.end.take().is_none() {
            if self.remaining.is_zero() {
                self.remaining = self.total;
            }
            self.end = Some(now + self.remaining);
        }
    }
    fn reset(&mut self) {
        self.remaining = self.total;
        self.end = None;
    }
    fn adjust(&mut self, seconds: i64) {
        self.total = Duration::from_secs(
            self.total
                .as_secs()
                .saturating_add_signed(seconds)
                .clamp(1, 86_400),
        );
        self.reset();
    }
    fn handle(&mut self, event: &Value, now: Instant) {
        let action = match event["type"].as_str() {
            Some("pointer")
                if event["kind"] == "button"
                    && event["pressed"] == true
                    && event["button"] == 272
                    && event["drag_handle"] != true =>
            {
                event["target"].as_str()
            }
            Some("pointer") if event["kind"] == "scroll" && event["drag_handle"] != true => {
                if let Some(value) = event["value"].as_f64() {
                    self.adjust(if value < 0.0 { 60 } else { -60 });
                }
                None
            }
            Some("key") if event["pressed"] == true => match event["key"].as_u64() {
                Some(57) => Some("toggle"),
                Some(19) => Some("reset"),
                Some(105) => Some("less"),
                Some(106) => Some("more"),
                _ => None,
            },
            Some("custom") => {
                if event["name"] == "set"
                    && let Some(seconds) = event["data"]["seconds"].as_u64()
                {
                    *self = Self::new(seconds);
                }
                event["name"].as_str()
            }
            _ => None,
        };
        match action {
            Some("toggle") => self.toggle(now),
            Some("start") if self.end.is_none() => self.toggle(now),
            Some("pause") if self.end.is_some() => self.toggle(now),
            Some("reset") => self.reset(),
            Some("less") => self.adjust(-60),
            Some("more") => self.adjust(60),
            _ => {}
        }
    }
    fn frame(&self) -> Value {
        let seconds = self.remaining.as_secs() + u64::from(self.remaining.subsec_nanos() != 0);
        let active = self.end.is_some();
        let mut elements = vec![
            json!({"bounds":[20,52,300,58],"content":{"type":"text","text":format!("{:02}:{:02}",seconds/60,seconds%60),"size":48,"align":"center","bold":true}}),
            json!({"bounds":[20,116,300,24],"content":{"type":"text","text":if active {"FOCUS"} else if seconds==0 {"DONE"} else {"READY / PAUSED"},"size":12,"align":"center","color":"muted"}}),
            json!({"bounds":[24,150,292,12],"content":{"type":"bar","value":1.0-self.remaining.as_secs_f64()/self.total.as_secs_f64(),"radius":6,"color":"green"}}),
            json!({"bounds":[20,238,300,20],"content":{"type":"text","text":"Space: start/pause / R: reset / scroll: duration","size":10,"color":"muted","align":"center"}}),
        ];
        for (id, label, x, width) in [
            ("less", "-1m", 20, 56),
            ("toggle", if active { "PAUSE" } else { "START" }, 84, 108),
            ("reset", "RESET", 200, 64),
            ("more", "+1m", 272, 48),
        ] {
            elements.push(json!({"id":id,"bounds":[x,180,width,38],"content":{"type":"rect","radius":6,"color":if id=="toggle" {"green"} else {"bar_background"}}}));
            elements.push(json!({"bounds":[x,191,width,22],"content":{"type":"text","text":label,"size":12,"align":"center","color":if id=="toggle" {"background"} else {"foreground"}}}));
        }
        json!({"version":2,"title":"Focus timer","elements":elements})
    }
}

pub fn run(package: &Path) -> Result<(), Box<dyn Error>> {
    let settings = common::settings(package)?;
    let mut timer = Timer::new(common::integer(&settings, "seconds", 1500).clamp(1, 86_400) as u64);
    let mut events = Events::new()?;
    loop {
        timer.update(Instant::now());
        {
            let mut out = io::stdout().lock();
            serde_json::to_writer(&mut out, &timer.frame())?;
            out.write_all(b"\n")?;
            out.flush()?;
        }
        let timeout = timer.end.map_or(Duration::from_secs(5), |_| {
            let fraction = timer.remaining.subsec_nanos();
            Duration::from_nanos(if fraction == 0 {
                1_000_000_000
            } else {
                u64::from(fraction)
            })
        });
        if let Some(event) = events.poll(timeout)? {
            timer.handle(&event, Instant::now());
        }
        if events.closed() {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn countdown_is_monotonic_pause_preserves_fraction_and_limits_are_bounded() {
        let now = Instant::now();
        let mut timer = Timer::new(10);
        timer.toggle(now);
        timer.update(now + Duration::from_millis(1250));
        assert_eq!(timer.remaining, Duration::from_millis(8750));
        timer.toggle(now + Duration::from_millis(1250));
        timer.update(now + Duration::from_secs(99));
        assert_eq!(timer.remaining, Duration::from_millis(8750));
        timer.toggle(now + Duration::from_secs(100));
        timer.update(now + Duration::from_secs(110));
        assert!(timer.remaining.is_zero());
        assert!(timer.end.is_none());
        timer.adjust(-999);
        assert_eq!(timer.total, Duration::from_secs(1));
        timer.adjust(i64::MAX);
        assert_eq!(timer.total, Duration::from_secs(86_400));
        assert_eq!(timer.frame()["version"], 2);
    }
}
