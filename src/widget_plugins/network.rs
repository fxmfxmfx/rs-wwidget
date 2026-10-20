use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    error::Error,
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

const PROC_NET_DEV: &str = "net/dev";
const PROC_ROUTE: &str = "net/route";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Counters {
    rx: u64,
    tx: u64,
}

fn parse_net_dev(input: &str) -> Vec<(String, Counters)> {
    input
        .lines()
        .skip(2)
        .filter_map(|line| {
            let (name, rest) = line.split_once(':')?;
            let mut fields = rest.split_whitespace();
            Some((
                name.trim().to_owned(),
                Counters {
                    rx: fields.next()?.parse().ok()?,
                    tx: fields.nth(7)?.parse().ok()?,
                },
            ))
        })
        .collect()
}

fn default_route(input: &str) -> Option<String> {
    input.lines().skip(1).find_map(|line| {
        let mut fields = line.split_whitespace();
        let iface = fields.next()?;
        let destination = fields.next()?;
        let _gateway = fields.next()?;
        let flags = u16::from_str_radix(fields.next()?, 16).ok()?;
        (destination == "00000000" && flags & 1 != 0 && iface != "lo").then(|| iface.to_owned())
    })
}

fn select_interface(
    configured: &str,
    route: &str,
    counters: &[(String, Counters)],
) -> Option<String> {
    if configured != "auto" {
        return counters
            .iter()
            .any(|(n, _)| n == configured)
            .then(|| configured.to_owned());
    }
    default_route(route)
        .filter(|name| counters.iter().any(|(n, _)| n == name))
        .or_else(|| {
            counters
                .iter()
                .find(|(n, _)| n != "lo")
                .map(|(n, _)| n.clone())
        })
}

fn delta_rate(previous: Counters, current: Counters, elapsed: Duration) -> Option<(f64, f64)> {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 || current.rx < previous.rx || current.tx < previous.tx {
        return None;
    }
    Some((
        (current.rx - previous.rx) as f64 / seconds,
        (current.tx - previous.tx) as f64 / seconds,
    ))
}

fn show_rate(bytes_per_second: f64) -> String {
    if bytes_per_second >= 1024.0 * 1024.0 {
        format!("{:.1} MiB/s", bytes_per_second / (1024.0 * 1024.0))
    } else if bytes_per_second >= 1024.0 {
        format!("{:.1} KiB/s", bytes_per_second / 1024.0)
    } else {
        format!("{:.0} B/s", bytes_per_second)
    }
}

pub fn run(package: &Path) -> Result<(), Box<dyn Error>> {
    let settings = super::common::settings(package)?;
    let interface = super::common::string(&settings, "interface", "auto");
    let interval = super::common::integer(&settings, "poll_seconds", 1).clamp(1, 60) as u64;
    let scale = super::common::integer(&settings, "scale_mib_per_second", 10).max(1) as f64
        * 1024.0
        * 1024.0;
    let proc_root = settings
        .get("proc_root")
        .and_then(toml::Value::as_str)
        .map(|p| super::common::data_path(package, p))
        .unwrap_or_else(|| Path::new("/proc").to_path_buf());
    let mut history = VecDeque::from(vec![0.0; 120]);
    let mut previous: Option<(String, Counters, Instant)> = None;

    loop {
        let now = Instant::now();
        let net = fs::read_to_string(proc_root.join(PROC_NET_DEV)).unwrap_or_default();
        let route = fs::read_to_string(proc_root.join(PROC_ROUTE)).unwrap_or_default();
        let counters = parse_net_dev(&net);
        let selected = select_interface(&interface, &route, &counters);
        if selected
            .as_ref()
            .is_some_and(|name| previous.as_ref().is_some_and(|(old, _, _)| old != name))
        {
            history.clear();
            history.extend(std::iter::repeat_n(0.0, 120));
        }
        let current = selected
            .as_ref()
            .and_then(|name| counters.iter().find(|(n, _)| n == name));
        let rows: Vec<Value> = if let Some((name, value)) = current {
            let rates = previous
                .as_ref()
                .filter(|(old_name, _, _)| old_name == name)
                .and_then(|(_, old, at)| delta_rate(*old, *value, now.duration_since(*at)));
            if let Some((rx, tx)) = rates {
                history.push_back(((rx + tx) / scale).clamp(0.0, 1.0));
                if history.len() > 120 {
                    history.pop_front();
                }
                vec![
                    json!({"label":"Interface","value":name}),
                    json!({"label":"Received","value":show_rate(rx),"fraction":(rx / scale).clamp(0.0,1.0)}),
                    json!({"label":"Sent","value":show_rate(tx),"fraction":(tx / scale).clamp(0.0,1.0)}),
                    json!({"label":"Traffic","value":"RX + TX","history":history}),
                ]
            } else {
                if previous.as_ref().is_some_and(|(old_name, old, _)| {
                    old_name == name && (value.rx < old.rx || value.tx < old.tx)
                }) {
                    history.clear();
                    history.extend(std::iter::repeat_n(0.0, 120));
                }
                vec![
                    json!({"label":"Interface","value":name}),
                    json!({"label":"Received","value":"Sampling…"}),
                    json!({"label":"Sent","value":"Sampling…"}),
                ]
            }
        } else {
            previous = None;
            history.clear();
            history.extend(std::iter::repeat_n(0.0, 120));
            vec![json!({"label":"Network","value":"Unavailable (no interface data)"})]
        };
        if let (Some(name), Some((_, value))) = (selected, current) {
            previous = Some((name, *value, now));
        }
        super::common::emit("Network", rows)?;
        thread::sleep(Duration::from_secs(interval));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proc_net_dev_and_route() {
        let counters = parse_net_dev(
            "Inter-| Receive | Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\n  lo: 1 2 0 0 0 0 0 0 3 4 0 0 0 0 0 0\n eth0: 100 2 0 0 0 0 0 0 200 4 0 0 0 0 0 0\n",
        );
        assert_eq!(counters[1], ("eth0".into(), Counters { rx: 100, tx: 200 }));
        assert_eq!(
            default_route(
                "Iface Destination Gateway Flags RefCnt Use Metric\neth0 00000000 0102A8C0 0003 0 0 100\n"
            ),
            Some("eth0".into())
        );
    }

    #[test]
    fn counter_reset_has_no_rate_and_rates_are_byte_based() {
        assert_eq!(
            delta_rate(
                Counters { rx: 10, tx: 20 },
                Counters { rx: 1010, tx: 2020 },
                Duration::from_secs(2)
            ),
            Some((500.0, 1000.0))
        );
        assert_eq!(
            delta_rate(
                Counters { rx: 10, tx: 20 },
                Counters { rx: 9, tx: 21 },
                Duration::from_secs(1)
            ),
            None
        );
    }
}
