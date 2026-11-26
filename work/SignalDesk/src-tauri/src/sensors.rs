use serde_json::{json, Value};
use std::{io::Read, time::Duration};

fn records(data: &Value) -> impl Iterator<Item = &Value> {
    data.as_array().into_iter().flatten()
}

fn cpu_sensor(row: &Value) -> bool {
    let cpu_path = |value: &Value| {
        value.as_str().is_some_and(|value| {
            let name = value.split('/').nth(1).unwrap_or("").to_ascii_lowercase();
            name == "cpu"
                || name.ends_with("cpu")
                || name.starts_with("amd") && name.contains("cpu")
        })
    };
    cpu_path(&row["Identifier"]) || cpu_path(&row["Parent"])
}
fn name(row: &Value) -> String {
    row["Name"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}
pub fn cpu_reading(items: &Value, kind: &str) -> Option<(f64, Value)> {
    let priority = |raw: &str| match kind {
        "Temperature" if matches!(raw, "cpu package" | "package") => Some(3u8),
        "Temperature"
            if matches!(
                raw,
                "core (tctl/tdie)"
                    | "cpu (tctl/tdie)"
                    | "tctl/tdie"
                    | "cpu tdie"
                    | "tdie"
                    | "core (tdie)"
            ) =>
        {
            Some(2)
        }
        "Temperature" if matches!(raw, "cpu tctl" | "tctl" | "core (tctl)") => Some(1),
        "Power"
            if matches!(
                raw,
                "cpu package"
                    | "package"
                    | "cpu package power"
                    | "package power"
                    | "cpu package (smu)"
            ) =>
        {
            Some(2)
        }
        "Power"
            if matches!(
                raw,
                "cpu ppt" | "cpu package (ppt)" | "package power tracking"
            ) =>
        {
            Some(1)
        }
        _ => None,
    };
    records(items)
        .filter(|v| cpu_sensor(v) && v["SensorType"] == kind)
        .filter_map(|v| {
            let value = v["Value"].as_f64()?;
            let valid = if kind == "Temperature" {
                (-30. ..=160.).contains(&value)
            } else {
                (0. ..=2000.).contains(&value)
            };
            valid.then_some((priority(&name(v))?, value, v))
        })
        .max_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)))
        .map(|(_, reading, row)| (reading, json!({"name":row["Name"],"source":row["Source"]})))
}
pub fn fastest_fan(items: &Value) -> Option<(f64, Value)> {
    records(items)
        .filter(|row| row["SensorType"] == "Fan")
        .filter_map(|row| {
            Some((
                row["Value"]
                    .as_f64()
                    .filter(|rpm| (0. ..=100000.).contains(rpm))?,
                row,
            ))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(rpm, row)| (rpm, json!({"name":row["Name"],"source":row["Source"]})))
}
pub fn is_drive_sensor(row: &Value) -> bool {
    row["SensorType"] == "Temperature"
        && (["Identifier", "Parent"].iter().any(|key| {
            let hardware = row[key]
                .as_str()
                .and_then(|v| v.split('/').nth(1))
                .unwrap_or("")
                .to_ascii_lowercase();
            matches!(hardware.as_str(), "storage" | "hdd" | "ssd" | "nvme")
        }))
}
pub fn merge(items: &mut Vec<Value>, incoming: Vec<Value>) {
    for row in incoming {
        let id = &row["Identifier"];
        if !id.is_null() {
            if let Some(existing) = items.iter_mut().find(|item| item["Identifier"] == *id) {
                *existing = row;
                continue;
            }
        }
        items.push(row);
    }
}

pub fn local_http() -> Result<Vec<Value>, String> {
    let client = ureq::AgentBuilder::new()
        .timeout(Duration::from_millis(900))
        .try_proxy_from_env(false)
        .redirects(0)
        .build();
    let response = client
        .get("http://127.0.0.1:8085/data.json")
        .call()
        .map_err(|_| "LibreHardwareMonitor was not reachable at 127.0.0.1:8085.".to_owned())?;
    let mut body = Vec::new();
    response
        .into_reader()
        .take(2_000_001)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() > 2_000_000 {
        return Err("LibreHardwareMonitor sent an oversized sensor response.".into());
    }
    let tree: Value = serde_json::from_slice(&body)
        .map_err(|_| "The local endpoint returned invalid sensor JSON.".to_owned())?;
    let mut out = Vec::new();
    walk(&tree, None, 0, &mut out);
    if out.is_empty() {
        return Err("The local sensor endpoint returned no supported readings.".into());
    }
    Ok(out)
}
fn walk(node: &Value, group: Option<&str>, depth: u8, out: &mut Vec<Value>) {
    if depth > 25 || out.len() >= 2048 {
        return;
    }
    let label = node["Text"].as_str().unwrap_or("");
    let explicit_group = match label {
        "Temperatures" => Some("Temperature"),
        "Powers" => Some("Power"),
        "Fans" => Some("Fan"),
        "Clocks" => Some("Clock"),
        "Loads" => Some("Load"),
        _ => None,
    };
    let group = explicit_group.or(group);
    let id = node["SensorId"].as_str().filter(|v| v.starts_with('/'));
    if let Some(id) = id {
        let kind = id
            .split('/')
            .nth_back(1)
            .and_then(|segment| match segment {
                "temperature" => Some("Temperature"),
                "power" => Some("Power"),
                "fan" => Some("Fan"),
                "clock" => Some("Clock"),
                "load" => Some("Load"),
                _ => None,
            })
            .or(group);
        let parse = |key: &str| {
            node[key].as_f64().filter(|v| v.is_finite()).or_else(|| {
                let text = node[key].as_str()?.trim();
                let numeric = text
                    .trim_start_matches('+')
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '-'))
                    .collect::<String>();
                numeric
                    .replace(',', ".")
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
            })
        };
        if let (Some(kind), Some(reading)) = (kind, parse("Value")) {
            let parent = id.split('/').take(3).collect::<Vec<_>>().join("/");
            out.push(json!({"Name":label,"Identifier":id,"Parent":parent,"SensorType":kind,"Value":reading,"Min":parse("Min"),"Max":parse("Max"),"Source":"LibreHardwareMonitor loopback"}));
        }
    }
    for child in records(&node["Children"]) {
        walk(child, group, depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_cpu_package_and_preserves_actual_fan_zero() {
        let sample = json!([
          {"Identifier":"/gpu-nvidia/0/temperature/0","Name":"CPU Package","Value":100,"SensorType":"Temperature"},
          {"Identifier":"/amdcpu/0/temperature/0","Name":"Core (Tctl/Tdie)","Value":64,"SensorType":"Temperature"},
          {"Identifier":"/amdcpu/0/power/0","Name":"CPU Package","Value":24,"SensorType":"Power"},
          {"Identifier":"/amdcpu/0/power/1","Name":"CPU PPT Limit","Value":90,"SensorType":"Power"},
          {"Identifier":"/lpc/0/fan/0","Name":"Case fan","Value":0,"SensorType":"Fan"}
        ]);
        assert_eq!(cpu_reading(&sample, "Temperature").unwrap().0, 64.);
        assert_eq!(cpu_reading(&sample, "Power").unwrap().0, 24.);
        assert_eq!(fastest_fan(&sample).unwrap().0, 0.);
    }
    #[test]
    fn parses_loopback_tree_with_sensor_identity() {
        let payload = json!({"Children":[{"Text":"Temperatures","Children":[{"Text":"CPU Package","SensorId":"/intelcpu/0/temperature/1","Value":"51,5 °C","Min":"—","Max":"56 °C"}]}]});
        let mut nodes = Vec::new();
        walk(&payload, None, 0, &mut nodes);
        assert_eq!(nodes[0]["Value"], 51.5);
        assert!(nodes[0]["Min"].is_null());
        assert_eq!(nodes[0]["Parent"], "/intelcpu/0");
    }
}
