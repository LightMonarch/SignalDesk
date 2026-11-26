use crate::{battery, sensors, storage, wifi, windows};
use serde_json::{json, Value};

pub fn native(health: &Result<Vec<Value>, String>) -> Value {
    let battery = battery::packs();
    let wlan = wifi::connections();
    let probes = vec![
        state(
            "Drive health",
            health,
            "No physical storage interface returned health data.",
        ),
        state(
            "Battery driver",
            &battery,
            "No system battery interfaces are available to this process.",
        ),
        state("Wi-Fi", &wlan, "Windows WLAN reported no Wi-Fi interfaces."),
    ];
    json!({"nativeDriveHealth":health.as_ref().ok(),"nativeBattery":battery.ok(),
        "batteryAggregate":battery::aggregate(),"wifi":wlan.ok(),"probes":probes})
}

fn state(name: &str, result: &Result<Vec<Value>, String>, empty: &str) -> Value {
    match result {
        Ok(rows) if !rows.is_empty() => {
            json!({"name":name,"status":"Available","detail":format!("{} devices or readings returned.",rows.len())})
        }
        Ok(_) => json!({"name":name,"status":"Unavailable","detail":empty}),
        Err(error) => json!({"name":name,"status":"Unavailable","detail":error}),
    }
}

pub fn combine(mut system: Value, native: &Value) -> Value {
    if !system.is_object() {
        system = json!({});
    }
    let rows = |source: &Value, key: &str| source[key].as_array().cloned().unwrap_or_default();
    system["driveHealth"] = json!(storage::merge(
        rows(native, "nativeDriveHealth"),
        &rows(&system, "driveHealth")
    ));
    for key in ["nativeBattery", "batteryAggregate", "wifi", "probes"] {
        system[key] = native[key].clone();
    }
    system
}

pub fn battery_sample(data: &Value) -> Value {
    let mut status = windows::battery();
    let aggregate = &data["batteryAggregate"];
    let mut packs = data["nativeBattery"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if packs.is_empty() {
        for design in data["batteryDesign"].as_array().into_iter().flatten() {
            let id = &design["InstanceName"];
            if id.is_null() {
                continue;
            }
            let related = |key: &str| {
                data[key]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|row| row["InstanceName"] == *id)
                    .unwrap_or(&Value::Null)
            };
            let full = related("batteryFull");
            let rates = related("batteryStatus");
            packs.push(json!({"manufacturer":design["ManufactureName"],"model":design["DeviceName"],
                "design":design["DesignedCapacity"],"full":full["FullChargedCapacity"],
                "cycles":valid(&related("batteryCycles")["CycleCount"]).filter(|&n|n>0),
                "charge":valid(&rates["ChargeRate"]).filter(|&n|n>0),"discharge":valid(&rates["DischargeRate"]).filter(|&n|n>0),
                "remainingCapacity":valid(&rates["RemainingCapacity"]),"discharging":rates["Discharging"],
                "unit":"mWh","source":"Windows WMI"}));
        }
    }
    if packs.is_empty() && aggregate.is_object() {
        packs.push(aggregate.clone());
    }
    if status["present"].is_null() && !packs.is_empty() {
        status["present"] = json!(true);
    }
    if status["present"] == false {
        return status;
    }
    let measured = battery::system_state().filter(|b| b.BatteryPresent != 0);
    if let Some(b) = measured {
        if status["present"].is_null() {
            status["present"] = json!(true);
        }
        if status["ac"].is_null() {
            status["ac"] = json!(b.AcOnLine != 0);
        }
        if status["charging"].is_null() {
            status["charging"] = json!(b.Charging != 0);
        }
        if status["pct"].is_null()
            && b.MaxCapacity > 0
            && b.MaxCapacity < u32::MAX
            && b.RemainingCapacity < u32::MAX
        {
            status["pct"] = json!((100. * f64::from(b.RemainingCapacity)
                / f64::from(b.MaxCapacity))
            .clamp(0., 100.));
        }
    }
    let aggregate_pair = capacity_pair(aggregate);
    let ratio = aggregate_pair.or_else(|| {
        let same_units = packs.len() == 1 || packs.iter().all(|row| row["unit"] == "mWh");
        if packs.is_empty() || !same_units {
            return None;
        }
        packs.iter().try_fold((0u64, 0u64), |(d, f), pack| {
            let (a, b) = capacity_pair(pack)?;
            Some((d + a, f + b))
        })
    });
    status["health"] = json!(ratio.map(|(d, f)| (100. * f as f64 / d as f64).clamp(0., 100.)));
    status["healthSource"] = json!(if ratio.is_none() {
        None
    } else if aggregate_pair.is_some() {
        Some("Windows battery report")
    } else if packs.len() == 1 {
        packs[0]["source"].as_str()
    } else {
        Some("Combined battery capacities")
    });
    if status["ac"] == true {
        status["remaining"] = Value::Null;
    } else {
        let os = status["remaining"].as_u64().filter(|&v| v > 0);
        let driver = measured
            .filter(|b| b.Discharging != 0)
            .map(|b| b.EstimatedTime)
            .filter(|&v| v > 0 && v < u32::MAX)
            .map(u64::from);
        let estimate = os
            .or(driver)
            .map(|v| (v, "Windows estimate"))
            .or_else(|| battery::remaining_seconds(&packs))
            .or_else(|| battery::remaining_seconds(std::slice::from_ref(aggregate)));
        status["remaining"] = json!(estimate.map(|(v, _)| v));
        status["remainingSource"] = json!(estimate.map(|(_, source)| source));
    }
    status["packs"] = json!(packs);
    status
}

fn valid(v: &Value) -> Option<u64> {
    v.as_u64().filter(|&v| v < u64::from(u32::MAX))
}
fn capacity_pair(pack: &Value) -> Option<(u64, u64)> {
    Some((
        valid(&pack["design"]).filter(|&v| v > 0)?,
        valid(&pack["full"]).filter(|&v| v > 0)?,
    ))
}

pub fn thermals(system: &mut Value, fast: &Value, passive: &Value, loopback: bool) {
    let rows = |source: &Value, key: &str| source[key].as_array().cloned().unwrap_or_default();
    let fresh = passive["sampledAt"]
        .as_i64()
        .is_some_and(|time| crate::now().abs_diff(time) <= 9);
    let mut values = if fresh {
        rows(passive, "rows")
    } else {
        Vec::new()
    };
    if fresh && loopback {
        sensors::merge(&mut values, rows(passive, "loopback"));
    }
    sensors::merge(&mut values, rows(&fast["helper"], "sensors"));
    system["sensors"] = json!(values);
    system["graphicsPower"] = fast["graphics"].clone();
    system["thermalAccess"] = fast["helper"].clone();
    if let Some(helper) = system["thermalAccess"].as_object_mut() {
        helper.remove("sensors");
        helper.remove("timestamp");
    }

    let mut probes = rows(system, "probes");
    if fresh {
        probes.extend(
            rows(passive, "probes")
                .into_iter()
                .filter(|row| loopback || row["name"] != "LibreHardwareMonitor loopback"),
        );
    }
    if let Some(error) = fast["graphicsError"].as_str() {
        probes.push(json!({"name":"Graphics power states","status":"Unavailable","detail":error}));
    }
    system["probes"] = json!(probes);
}
