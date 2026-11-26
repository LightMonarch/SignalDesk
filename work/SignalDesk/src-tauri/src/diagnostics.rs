use crate::{
    config::Config,
    health::{detect, issue},
    windows,
};
use serde_json::Value;
pub fn scan(snapshot: &Value, c: &Config) -> Vec<Value> {
    let mut rows = detect(snapshot, c);
    let ts = crate::now();
    if let Some(disks) = snapshot
        .pointer("/providers/driveHealth")
        .and_then(Value::as_array)
    {
        for d in disks {
            if d["status"]
                .as_str()
                .is_some_and(|s| ["Warning", "Unhealthy"].contains(&s))
            {
                rows.push(issue(
                    &format!("storage-{}", d["id"]),
                    "Storage",
                    "Critical",
                    "Storage reports a health problem",
                    format!("{} reports {}.", d["name"], d["status"]),
                    "The drive or its controller reported degraded health.",
                    "Back up important data and run the drive vendor's diagnostic utility.",
                    ts,
                ));
            }
        }
    }
    if let Ok(v) = windows::inventory() {
        for d in v["devices"].as_array().into_iter().flatten() {
            rows.push(issue(&format!("device-{}",d["Name"]),"Drivers","Warning","Windows device has a problem",format!("{} reports Configuration Manager code {}.",d["Name"],d["ConfigManagerErrorCode"]),"A driver may be missing or the device may have failed to start.","Review this device in Device Manager and check the hardware manufacturer's support page.",ts));
        }
    }
    let list = windows::startup();
    let count = list
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["enabled"] == true)
        .count();
    if count >= 12 {
        rows.push(issue(
            "startup",
            "Startup",
            "Advisory",
            "Many startup entries are enabled",
            format!("{count} enabled Run entries were found."),
            "Several installed applications are configured to launch at sign-in.",
            "Review startup items and disable optional entries you recognize.",
            ts,
        ));
    }
    if let Ok(events) = windows::events() {
        let events = events.as_array().into_iter().flatten();
        let mut crashes = 0;
        let mut system = 0;
        for event in events {
            if event["LogName"] == "Application" && event["Id"] == 1000 {
                crashes += 1;
            }
            if event["LogName"] == "System"
                && ["Critical", "Error"].contains(&event["LevelDisplayName"].as_str().unwrap_or(""))
            {
                system += 1;
            }
        }
        if crashes >= 3 {
            rows.push(issue("crashes","Applications","Warning","Repeated application crashes",format!("{crashes} Application Error event 1000 records in the returned seven-day window."),"An application or shared component may be unstable.","Inspect matching events and check whether the same executable repeats.",ts));
        }
        if system > 0 {
            rows.push(issue(
                "events",
                "Windows",
                "Advisory",
                "Windows logged system errors",
                format!("{system} error or critical records in the returned seven-day window."),
                "The event sources may include unrelated devices or services.",
                "Inspect the latest System entries for recurring sources and event IDs.",
                ts,
            ));
        }
    }
    rows
}
