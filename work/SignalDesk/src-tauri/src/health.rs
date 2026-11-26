use crate::config::{Config, COMPONENTS};
use serde_json::{json, Value};
pub fn number(v: &Value, p: &str) -> Option<f64> {
    v.pointer(p)?.as_f64().filter(|n| n.is_finite())
}
fn pressure(v: f64, limit: f64) -> f64 {
    (100. - (v - limit).max(0.) * 2.).clamp(0., 100.)
}
pub fn classify(n: f64) -> &'static str {
    if n >= 90. {
        "Excellent"
    } else if n >= 75. {
        "Good"
    } else if n >= 60. {
        "Fair"
    } else if n >= 40. {
        "Poor"
    } else {
        "Critical"
    }
}
pub fn assess(s: &Value, c: &Config) -> Value {
    let temp = [number(s, "/metrics/cpuTemp"), number(s, "/metrics/gpuTemp")]
        .into_iter()
        .zip([c.cpu_temp, c.gpu_temp])
        .filter_map(|(v, t)| Some(pressure(v?, t)))
        .reduce(f64::min);
    let cpu_condition = number(s, "/metrics/cpuTemp").map(|t| pressure(t, c.cpu_temp));
    let gpu_condition = number(s, "/metrics/gpuTemp").map(|t| pressure(t, c.gpu_temp));
    let drive_status = s["providers"]["driveHealth"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| match v["status"].as_str()? {
            "Healthy" => Some(100.),
            "Warning" => Some(55.),
            "Unhealthy" => Some(10.),
            _ => None,
        })
        .reduce(f64::min);
    let disk = number(s, "/metrics/disk").map(|v| pressure(v, 100. - c.disk_free));
    let scores = [
        cpu_condition,
        gpu_condition,
        number(s, "/metrics/memory").map(|v| pressure(v, c.memory_limit)),
        disk.into_iter().chain(drive_status).reduce(f64::min),
        temp,
        number(s, "/metrics/batteryHealth").map(|v| v.clamp(0., 100.)),
        s["connected"].as_bool().map(|b| if b { 100. } else { 0. }),
    ];
    let total: f64 = scores
        .iter()
        .zip(c.weights)
        .filter(|(s, _)| s.is_some())
        .map(|(_, w)| w as f64)
        .sum();
    let score = (total > 0.).then(|| {
        scores
            .iter()
            .zip(c.weights)
            .filter_map(|(s, w)| Some((*s)? * w as f64 / total))
            .sum::<f64>()
            .round()
    });
    let parts:Vec<_>=scores.iter().enumerate().map(|(i,n)|json!({"name":COMPONENTS[i],"score":n.map(f64::round),"weight":if n.is_some()&&total>0.{100.*c.weights[i] as f64/total}else{0.}})).collect();
    json!({"score":score,"label":score.map(classify).unwrap_or("Unavailable"),"parts":parts})
}
pub fn issue(
    id: &str,
    component: &str,
    severity: &str,
    title: &str,
    evidence: String,
    cause: &str,
    action: &str,
    ts: i64,
) -> Value {
    json!({"id":id,"component":component,"severity":severity,"title":title,"evidence":evidence,"cause":cause,"action":action,"ts":ts})
}
pub fn detect(s: &Value, c: &Config) -> Vec<Value> {
    let ts = s["ts"].as_i64().unwrap_or(0);
    let mut out = vec![];
    for (key, group, limit, title, cause, action, unit) in [
        (
            "cpu",
            "CPU",
            c.cpu_limit,
            "High processor load",
            "A running task may be CPU-intensive.",
            "Sort Processes by CPU and inspect the leading application.",
            "%",
        ),
        (
            "memory",
            "Memory",
            c.memory_limit,
            "Memory pressure",
            "Applications may be holding large working sets.",
            "Review the largest processes before closing unused applications.",
            "%",
        ),
        (
            "cpuTemp",
            "Cooling",
            c.cpu_temp,
            "High CPU temperature",
            "Sustained load or restricted airflow can raise temperature.",
            "Check the fan readings, vents and current processor load.",
            " °C",
        ),
        (
            "gpuTemp",
            "Cooling",
            c.gpu_temp,
            "High GPU temperature",
            "Graphics workloads or restricted airflow may raise temperature.",
            "Review GPU load and inspect the cooling system.",
            " °C",
        ),
    ] {
        if let Some(v) = number(s, &format!("/metrics/{key}")).filter(|v| *v >= limit) {
            out.push(issue(
                key,
                group,
                "Warning",
                title,
                format!("{v:.1}{unit} measured; threshold {limit:.0}{unit}."),
                cause,
                action,
                ts,
            ))
        }
    }
    for d in s["disks"].as_array().into_iter().flatten() {
        if let Some(used) = d["usedPct"].as_f64() {
            if 100. - used <= c.disk_free {
                out.push(issue(
                    &format!("disk-{}", d["mount"]),
                    "Storage",
                    "Warning",
                    "Low free space",
                    format!("{} has {:.1}% free.", d["mount"], 100. - used),
                    "Files or application caches may be filling the volume.",
                    "Review the volume contents and move files you recognize.",
                    ts,
                ))
            }
        }
    }
    if let Some(v) = number(s, "/metrics/batteryHealth").filter(|v| *v < c.battery_health) {
        out.push(issue(
            "battery",
            "Battery",
            "Warning",
            "Reduced battery capacity",
            format!("Full charge capacity is {v:.0}% of design capacity."),
            "Capacity can decline with age and use.",
            "Compare capacity history and check the manufacturer’s battery guidance.",
            ts,
        ))
    }
    if s["connected"] == false {
        out.push(issue(
            "network",
            "Network",
            "Warning",
            "No active network interface",
            "Windows reports no operational non-loopback interface.".into(),
            "The device may be intentionally offline.",
            "Inspect the adapter and Wi-Fi or Ethernet connection.",
            ts,
        ))
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn availability() {
        let c = Config::default();
        assert!(assess(&json!({}), &c)["score"].is_null());
        let s = assess(&json!({"metrics":{"cpu":100,"cpuTemp":42}}), &c);
        assert!((s["parts"][0]["weight"].as_f64().unwrap() - (20. / 35.) * 100.).abs() < 0.0001);
        assert_eq!(s["score"], 100.);
    }
    #[test]
    fn boundaries() {
        assert_eq!(classify(39.), "Critical");
        assert_eq!(classify(40.), "Poor");
        assert_eq!(classify(74.), "Fair");
        assert_eq!(classify(89.), "Good");
        assert_eq!(classify(90.), "Excellent");
    }
}
