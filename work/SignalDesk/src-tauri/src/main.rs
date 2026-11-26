#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod assistant;
mod battery;
mod benchmark;
mod config;
mod db;
mod device;
mod diagnostics;
mod gpu;
mod health;
mod monitor;
mod native;
mod sensors;
mod storage;
mod telemetry;
mod thermal;
mod wifi;
mod windows;

use config::Config;
use db::Store;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering::Relaxed},
        Arc, Mutex, RwLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, RunEvent, State, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;

type Reply<T> = Result<T, String>;
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
struct Service {
    dir: PathBuf,
    inventory_cache: Mutex<Option<(Instant, Value)>>,
    config: RwLock<Config>,
    thermal: Mutex<thermal::Bridge>,
    db: Mutex<Store>,
    latest: RwLock<Value>,
    metrics_error: RwLock<Option<String>>,
    settings_error: RwLock<Option<String>>,
    settings_gate: Mutex<()>,
    window_gate: Mutex<()>,
    paused: AtomicBool,
    exit: AtomicBool,
    benchmark_busy: AtomicBool,
    cancel: AtomicBool,
    progress: AtomicU8,
}
fn database(dir: &Path, c: &Config) -> PathBuf {
    if c.database_dir.is_empty() {
        dir.join("monitor.sqlite3")
    } else {
        Path::new(&c.database_dir).join("monitor.sqlite3")
    }
}
impl Service {
    fn open(dir: PathBuf) -> Reply<Self> {
        std::fs::create_dir_all(&dir).map_err(err)?;
        let (mut c, mut error) = match config::read(&dir.join("settings.json")) {
            Ok(c) => (c, None),
            Err(e) => (Config::default(), Some(e)),
        };
        let db = match Store::open(&database(&dir, &c)) {
            Ok(db) => db,
            Err(e) if !c.database_dir.is_empty() => {
                error = Some(format!(
                    "Configured database is unavailable: {e}. The default database is in use."
                ));
                c.database_dir.clear();
                Store::open(&database(&dir, &c)).map_err(err)?
            }
            Err(e) => return Err(err(e)),
        };
        Ok(Self {
            dir,
            inventory_cache: Mutex::new(None),
            config: RwLock::new(c),
            thermal: Mutex::new(thermal::Bridge::new()),
            db: Mutex::new(db),
            latest: RwLock::new(Value::Null),
            metrics_error: RwLock::new(None),
            settings_error: RwLock::new(error),
            settings_gate: Mutex::new(()),
            window_gate: Mutex::new(()),
            paused: AtomicBool::new(false),
            exit: AtomicBool::new(false),
            benchmark_busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            progress: AtomicU8::new(0),
        })
    }
    fn inventory(&self, fresh: bool) -> Reply<Value> {
        let mut cache = self.inventory_cache.lock().map_err(err)?;
        if !fresh {
            if let Some((t, v)) = cache.as_ref() {
                if t.elapsed() < Duration::from_secs(300) {
                    return Ok(v.clone());
                }
            }
        }
        let mut v = windows::inventory()?;
        let (system, bios) = native::firmware();
        if v["computer"].is_null() {
            v["computer"] = system;
        }
        if v["bios"].is_null() {
            v["bios"] = bios;
        }
        let displays = native::graphics().1;
        if !displays.is_empty() {
            v["displays"] = json!(displays)
        }
        *cache = Some((Instant::now(), v.clone()));
        Ok(v)
    }
    fn persist(&self, c: Config) -> Reply<Config> {
        c.validate()?;
        let _gate = self.settings_gate.lock().map_err(err)?;
        let mut active = self.config.write().map_err(err)?;
        let path = database(&self.dir, &c);
        let same_db = path == database(&self.dir, &active);
        let replacement = if same_db {
            None
        } else {
            let dir = Path::new(&c.database_dir);
            if !c.database_dir.is_empty()
                && (!dir.is_absolute() || !dir.is_dir() || c.database_dir.starts_with(r"\\"))
            {
                return Err("Choose an existing local directory.".into());
            }
            Some(Store::open(&path).map_err(err)?)
        };
        let mut store = self.db.lock().map_err(err)?;
        if c.startup != active.startup {
            config::autorun(c.startup)?;
        }
        if let Err(e) = config::save(&self.dir.join("settings.json"), &c) {
            if c.startup != active.startup {
                let _ = config::autorun(active.startup);
            }
            return Err(e);
        }
        if let Some(db) = replacement {
            *store = db;
        }
        *active = c.clone();
        *self.settings_error.write().map_err(err)? = None;
        Ok(c)
    }
}

fn wait_provider(service: &Service, seconds: u64) {
    for _ in 0..seconds {
        if service.exit.load(Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn poll(app: AppHandle, s: Arc<Service>) {
    let provider = Arc::new(RwLock::new((Value::Null, None::<String>)));
    let provider_thread = provider.clone();
    let provider_state = s.clone();
    std::thread::spawn(move || {
        while !provider_state.exit.load(Relaxed) {
            if !provider_state.paused.load(Relaxed) {
                let mut next = match windows::sensors() {
                    Ok(v) => (v, None),
                    Err(e) => (Value::Null, Some(e)),
                };
                let (graphics, displays) = native::graphics();
                if next.0.is_null() {
                    next.0 = json!({})
                }
                next.0["dxgi"] = json!(graphics);
                next.0["displays"] = json!(displays);
                *provider_thread.write().unwrap() = next;
            }
            let secs = if provider_state.config.read().unwrap().low_resource {
                30
            } else {
                10
            };
            wait_provider(&provider_state, secs);
        }
    });
    let storage_data = Arc::new(RwLock::new(Ok(Vec::new())));
    let drive_thread = storage_data.clone();
    let drive_state = s.clone();
    std::thread::spawn(move || {
        while !drive_state.exit.load(Relaxed) {
            if !drive_state.paused.load(Relaxed) {
                let fresh = storage::drives();
                *drive_thread.write().unwrap() = fresh;
            }
            let seconds = if drive_state.config.read().unwrap().low_resource {
                90
            } else {
                40
            };
            wait_provider(&drive_state, seconds);
        }
    });
    let native_data = Arc::new(RwLock::new(Value::Null));
    let native_thread = native_data.clone();
    let native_state = s.clone();
    std::thread::spawn(move || {
        while !native_state.exit.load(Relaxed) {
            let config = native_state.config.read().unwrap().clone();
            if !native_state.paused.load(Relaxed) {
                let drives = storage_data.read().unwrap().clone();
                let fresh = telemetry::native(&drives);
                *native_thread.write().unwrap() = fresh;
            }
            wait_provider(&native_state, if config.low_resource { 12 } else { 3 });
        }
    });
    // Keep COM sensor reads separate from the companion and NVIDIA polling.
    let passive = Arc::new(RwLock::new(Value::Null));
    let passive_thread = passive.clone();
    let passive_state = s.clone();
    std::thread::spawn(move || {
        let mut reader = sensors::Wmi::new();
        while !passive_state.exit.load(Relaxed) {
            let config = passive_state.config.read().unwrap().clone();
            if !passive_state.paused.load(Relaxed) {
                let next = reader.poll(config.sensor_loopback);
                *passive_thread.write().unwrap() = next;
            }
            std::thread::sleep(Duration::from_millis(if config.low_resource {
                4000
            } else {
                1500
            }));
        }
    });
    let fast = Arc::new(RwLock::new(Value::Null));
    let fast_thread = fast.clone();
    let fast_state = s.clone();
    std::thread::spawn(move || {
        let mut gpu = gpu::Monitor::new();
        while !fast_state.exit.load(Relaxed) {
            let paused = fast_state.paused.load(Relaxed);
            let helper = fast_state
                .thermal
                .try_lock()
                .ok()
                .map(|mut bridge| bridge.read(paused));
            if paused {
                gpu.pause();
            } else {
                let (graphics, error) = gpu.poll();
                *fast_thread.write().unwrap() =
                    json!({"helper":helper,"graphics":graphics,"graphicsError":error});
            }
            std::thread::sleep(Duration::from_millis(1500));
        }
    });
    let mut monitor = monitor::Monitor::new();
    let mut next = Instant::now();
    let mut last_prune = 0;
    let mut previous_link = None;
    let mut previous_health = None;
    let mut drop_baseline: Option<f64> = None;
    let mut prior_sample = 0;
    let mut sustained = std::collections::HashMap::<String, i64>::new();
    while !s.exit.load(Relaxed) {
        if s.paused.load(Relaxed) {
            sustained.clear();
            std::thread::sleep(Duration::from_millis(300));
            next = Instant::now();
            continue;
        }
        if Instant::now() < next {
            std::thread::sleep(Duration::from_millis(220));
            continue;
        }
        let c = s.config.read().unwrap().clone();
        let sensors = provider.read().unwrap().clone();
        let mut providers = telemetry::combine(sensors.0, &native_data.read().unwrap());
        telemetry::thermals(
            &mut providers,
            &fast.read().unwrap(),
            &passive.read().unwrap(),
            c.sensor_loopback,
        );
        let mut sample = monitor.sample(&providers);
        sample["powerPlan"] = providers["powerPlan"].clone();
        let score = health::assess(&sample, &c);
        sample["metrics"]["score"] = score["score"].clone();
        sample["health"] = score;
        sample["providerError"] = json!(sensors.1);
        let ts = sample["ts"].as_i64().unwrap_or_else(now);
        if ts < prior_sample || ts - prior_sample > (c.interval.max(8) * 3) as i64 {
            sustained.clear();
            previous_health = None;
            drop_baseline = None;
        }
        prior_sample = ts;
        let mut issues = health::detect(&sample, &c);
        let link = sample["connected"].as_bool();
        let connection = if previous_link.is_some() && link.is_some() && link != previous_link {
            Some(json!({"ts":ts,"status":if link==Some(true){"Connected"}else{"Disconnected"}}))
        } else {
            None
        };
        if previous_link == Some(true) && link == Some(false) {
            issues.retain(|i| i["id"] != "network");
            issues.push(health::issue(
                "network",
                "Network",
                "Warning",
                "Network disconnected",
                "Operational physical adapters changed from connected to disconnected.".into(),
                "The adapter, cable or access point may be unavailable.",
                "Inspect the current network interface.",
                ts,
            ));
        }
        previous_link = link;
        let current = sample.pointer("/health/score").and_then(Value::as_f64);
        if let (Some(p), Some(n)) = (previous_health, current) {
            if p - n >= 25. {
                drop_baseline = Some(drop_baseline.unwrap_or(p).max(p));
            }
        }
        if let (Some(p), Some(n)) = (drop_baseline, current) {
            if p - n >= 20. {
                issues.push(health::issue(
                    "score-drop",
                    "System",
                    "Warning",
                    "Sustained health score decline",
                    format!("The estimate fell from {p:.0} to {n:.0}."),
                    "A monitored condition has worsened.",
                    "Compare recent metrics and inspect active diagnostics.",
                    ts,
                ));
            } else {
                drop_baseline = None;
            }
        }
        previous_health = current;
        if let Some(sen) = sample["providers"]["sensors"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|sen| {
                sen["Name"]
                    .as_str()
                    .is_some_and(|n| n.to_lowercase().contains("throttl"))
                    && sen["Value"].as_f64().is_some_and(|n| n > 0.)
            })
        {
            issues.push(health::issue(
                "throttle",
                "Cooling",
                "Warning",
                "Thermal throttling detected",
                format!("{} is active.", sen["Name"]),
                "Processor firmware is reducing performance to control temperature.",
                "Inspect temperatures and cooling under load.",
                ts,
            ));
        }
        for d in sample["providers"]["driveHealth"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if d["status"]
                .as_str()
                .is_some_and(|v| ["Warning", "Unhealthy"].contains(&v))
            {
                issues.push(health::issue(
                    &format!("storage-{}", d["id"]),
                    "Storage",
                    "Critical",
                    "Storage health warning",
                    format!("{} reports {}.", d["name"], d["status"]),
                    "Windows reports degraded hardware health.",
                    "Back up important data and run vendor diagnostics.",
                    ts,
                ));
            }
        }
        sustained.retain(|id, last| {
            ts >= *last && issues.iter().any(|v| v["id"] == id.as_str()) && ts - *last <= 86_400
        });
        issues.retain(|v| {
            let id = v["id"].as_str().unwrap_or("system");
            if !["cpu", "memory", "cpuTemp", "gpuTemp", "score-drop"].contains(&id) {
                return true;
            }
            let first = sustained.entry(id.into()).or_insert(ts);
            ts - *first >= c.duration as i64
        });
        if s.benchmark_busy.load(Relaxed) && over_limit(&sample, c.benchmark_limit) {
            s.cancel.store(true, Relaxed);
        }
        sample["issues"] = json!(issues);
        let mut notify = vec![];
        let mut write = || -> rusqlite::Result<()> {
            let mut db = s.db.lock().unwrap();
            db.sample(ts, &sample["metrics"])?;
            if let Some(row) = &connection {
                db.record("connection", "link", ts, row)?;
            }
            if ts - last_prune >= 600 || ts < last_prune {
                db.prune(ts, c.retention)?;
                last_prune = ts;
            }
            for issue in &issues {
                let id = issue["id"].as_str().unwrap_or("system");
                if db
                    .last("alert", id)?
                    .is_none_or(|last| ts < last || ts - last >= c.cooldown as i64 * 60)
                {
                    db.record("alert", id, ts, issue)?;
                    notify.push(issue);
                }
            }
            Ok(())
        };
        *s.metrics_error.write().unwrap() = write().err().map(err);
        for issue in notify {
            if c.notifications {
                let _ = app
                    .notification()
                    .builder()
                    .title(issue["title"].as_str().unwrap_or("System alert"))
                    .body(issue["evidence"].as_str().unwrap_or(""))
                    .show();
            }
        }
        if let Some(tray) = app.tray_by_id("monitor") {
            let pct = |k: &str| {
                sample["metrics"][k]
                    .as_f64()
                    .map(|n| format!("{n:.0}%"))
                    .unwrap_or_else(|| "—".into())
            };
            let _ = tray.set_tooltip(Some(format!(
                "Silicon Desk · CPU {} · RAM {} · GPU {}",
                pct("cpu"),
                pct("memory"),
                pct("gpu")
            )));
        }
        let counts = sample["processes"].as_array().map(Vec::len).unwrap_or(0);
        *s.latest.write().unwrap() = sample;
        let _ = app.emit("sample-ready", counts);
        next = Instant::now()
            + Duration::from_secs(if c.low_resource {
                c.interval.max(8)
            } else {
                c.interval
            });
    }
}
fn make_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?.build()
}
fn reveal(app: &AppHandle) {
    let app = app.clone();
    // WebView2 window creation must run off the Windows event callback.
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<Arc<Service>>();
        let _gate = service.window_gate.lock().unwrap();
        let window = match app.get_webview_window("main") {
            Some(w) => Ok(w),
            None => make_window(&app),
        };
        match window {
            Ok(w) => {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            Err(error) => eprintln!("Unable to open Silicon Desk dashboard: {error}"),
        }
    });
}
fn toggle_pause(app: &AppHandle, s: &Service) -> bool {
    let paused = !s.paused.fetch_xor(true, Relaxed);
    let _ = app.emit("paused-changed", paused);
    if paused {
        if let Some(t) = app.tray_by_id("monitor") {
            let _ = t.set_tooltip(Some("Silicon Desk · Monitoring paused"));
        }
    }
    paused
}

#[tauri::command]
async fn enable_thermal(s: State<'_, Arc<Service>>) -> Reply<()> {
    let service = s.inner().clone();
    task(move || {
        service
            .thermal
            .lock()
            .map_err(err)?
            .start(service.paused.load(Relaxed))
    })
    .await
}

#[tauri::command]
fn snapshot(s: State<'_, Arc<Service>>) -> Reply<Value> {
    let data = s.latest.read().map_err(err)?;
    let compact = data.as_object().map(|m| {
        m.iter()
            .filter(|(k, _)| k.as_str() != "processes")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<serde_json::Map<_, _>>()
    });
    let paused = s.paused.load(Relaxed);
    Ok(
        json!({"sample":compact,"paused":paused,"error":*s.metrics_error.read().map_err(err)?,"settingsError":*s.settings_error.read().map_err(err)?}),
    )
}
#[tauri::command]
fn processes(s: State<'_, Arc<Service>>) -> Reply<Value> {
    Ok(s.latest.read().map_err(err)?["processes"].clone())
}
#[tauri::command]
async fn process_publisher(s: State<'_, Arc<Service>>, pid: u32, started: u64) -> Reply<Value> {
    let path = {
        let data = s.latest.read().map_err(err)?;
        data["processes"]
            .as_array()
            .and_then(|a| {
                a.iter()
                    .find(|p| p["pid"] == pid && p["started"] == started)
            })
            .and_then(|p| p["path"].as_str())
            .map(str::to_owned)
    };
    let Some(path) = path else {
        return Ok(Value::Null);
    };
    task(move || windows::publisher(&path)).await
}
#[tauri::command]
fn settings(s: State<'_, Arc<Service>>) -> Reply<Value> {
    let c = s.config.read().map_err(err)?;
    Ok(json!({"config":*c,"database":database(&s.dir,&c)}))
}
#[tauri::command]
async fn save_settings(s: State<'_, Arc<Service>>, config: Config) -> Reply<Config> {
    let service = s.inner().clone();
    task(move || service.persist(config)).await
}
#[tauri::command]
fn pause(app: AppHandle, s: State<'_, Arc<Service>>) -> bool {
    toggle_pause(&app, &s)
}
#[tauri::command]
fn history(s: State<'_, Arc<Service>>, start: i64, end: i64) -> Reply<Value> {
    if start < 0 || end <= start || end > now() + 3600 || end - start > 366 * 86400 {
        return Err("Select a valid range up to 366 days.".into());
    };
    s.db.lock()
        .map_err(err)?
        .history(start, end, now())
        .map_err(err)
}
#[tauri::command]
fn records(s: State<'_, Arc<Service>>, kind: String) -> Reply<Vec<Value>> {
    if !["alert", "benchmark", "diagnostic", "connection"].contains(&kind.as_str()) {
        return Err("Unknown history type.".into());
    }
    s.db.lock().map_err(err)?.records(&kind).map_err(err)
}
#[tauri::command]
fn mark_issue(
    s: State<'_, Arc<Service>>,
    kind: String,
    id: String,
    ts: i64,
    status: String,
) -> Reply<()> {
    if !["alert", "diagnostic"].contains(&kind.as_str())
        || !["Acknowledged", "Dismissed", "Resolved", "Open"].contains(&status.as_str())
    {
        return Err("Invalid record state.".into());
    }
    s.db.lock()
        .map_err(err)?
        .mark(&kind, &id, ts, &status)
        .map_err(err)
}
#[tauri::command]
async fn clear_history(s: State<'_, Arc<Service>>) -> Reply<()> {
    let service = s.inner().clone();
    task(move || service.db.lock().map_err(err)?.clear().map_err(err)).await
}
#[tauri::command]
async fn inventory(s: State<'_, Arc<Service>>, fresh: Option<bool>) -> Reply<Value> {
    let service = s.inner().clone();
    task(move || service.inventory(fresh.unwrap_or(false))).await
}
#[tauri::command]
async fn event_logs(start: Option<i64>, end: Option<i64>, page: Option<u32>) -> Reply<Value> {
    task(move || windows::events_page(start, end, page.unwrap_or(0))).await
}
#[tauri::command]
fn startup_entries() -> Value {
    windows::startup()
}
#[tauri::command]
fn startup_toggle(name: String, enabled: bool) -> Reply<()> {
    windows::toggle_startup(&name, enabled)
}
#[tauri::command]
fn end_process(pid: u32, started: u64) -> Reply<()> {
    monitor::end(pid, started)
}

async fn task<T: Send + 'static>(f: impl FnOnce() -> Reply<T> + Send + 'static) -> Reply<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}
#[tauri::command]
async fn run_diagnostics(s: State<'_, Arc<Service>>) -> Reply<Vec<Value>> {
    let service = s.inner().clone();
    task(move || {
        let sample = service.latest.read().map_err(err)?.clone();
        if sample.is_null() {
            return Err("Wait for the first hardware sample.".into());
        }
        let c = service.config.read().map_err(err)?.clone();
        let mut rows = diagnostics::scan(&sample, &c);
        rows.retain(|r| {
            !["cpu", "memory", "cpuTemp", "gpuTemp"].contains(&r["id"].as_str().unwrap_or(""))
        });
        rows.extend(
            sample["issues"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| {
                    [
                        "cpu",
                        "memory",
                        "cpuTemp",
                        "gpuTemp",
                        "throttle",
                        "score-drop",
                    ]
                    .contains(&r["id"].as_str().unwrap_or(""))
                })
                .cloned(),
        );
        let db = service.db.lock().map_err(err)?;
        for r in &rows {
            let id = r["id"].as_str().unwrap_or("scan");
            db.record("diagnostic", id, r["ts"].as_i64().unwrap_or_else(now), r)
                .map_err(err)?;
            if r["severity"] == "Warning" || r["severity"] == "Critical" {
                let ts = now();
                if db
                    .last("alert", id)
                    .map_err(err)?
                    .is_none_or(|last| ts - last >= c.cooldown as i64 * 60)
                {
                    let mut alert = r.clone();
                    alert["ts"] = json!(ts);
                    db.record("alert", id, ts, &alert).map_err(err)?;
                }
            }
        }
        Ok(rows)
    })
    .await
}
#[tauri::command]
async fn start_benchmark(s: State<'_, Arc<Service>>, kind: String) -> Reply<Value> {
    let service = s.inner().clone();
    task(move || {
        if service
            .benchmark_busy
            .compare_exchange(false, true, Relaxed, Relaxed)
            .is_err()
        {
            return Err("A benchmark is already running.".into());
        }
        service.cancel.store(false, Relaxed);
        service.progress.store(0, Relaxed);
        let config = service.config.read().map_err(err)?.clone();
        let limit = config.benchmark_limit;
        let cut =
            || service.cancel.load(Relaxed) || over_limit(&service.latest.read().unwrap(), limit);
        let target = (!config.offline && !config.connectivity_target.is_empty())
            .then_some(config.connectivity_target.as_str());
        let result = benchmark::run(
            &kind,
            &service.dir,
            config.bench_seconds,
            target,
            &cut,
            |n| service.progress.store(n, Relaxed),
        );
        service.benchmark_busy.store(false, Relaxed);
        let row = result?;
        service
            .db
            .lock()
            .map_err(err)?
            .record("benchmark", &kind, now(), &row)
            .map_err(err)?;
        Ok(row)
    })
    .await
}
fn over_limit(s: &Value, limit: f64) -> bool {
    ["cpuTemp", "gpuTemp", "driveTemp"]
        .iter()
        .any(|k| s["metrics"][k].as_f64().is_some_and(|v| v >= limit))
}
#[tauri::command]
fn gpu_result(
    s: State<'_, Arc<Service>>,
    fps: f64,
    duration: f64,
    renderer: String,
) -> Reply<Value> {
    if !fps.is_finite() || fps <= 0. || !duration.is_finite() || !(0.5..=60.).contains(&duration) {
        return Err("Invalid WebGL benchmark result.".into());
    }
    let row = json!({"kind":"GPU","value":fps,"duration":duration,"unit":"frames/s (WebGL 2 workload)","ts":now(),"note":renderer.chars().take(250).collect::<String>()});
    s.db.lock()
        .map_err(err)?
        .record("benchmark", "GPU", now(), &row)
        .map_err(err)?;
    Ok(row)
}
#[tauri::command]
fn benchmark_status(s: State<'_, Arc<Service>>) -> Value {
    json!({"busy":s.benchmark_busy.load(Relaxed),"progress":s.progress.load(Relaxed)})
}
#[tauri::command]
fn cancel_benchmark(s: State<'_, Arc<Service>>) {
    s.cancel.store(true, Relaxed)
}
#[tauri::command]
async fn connectivity(s: State<'_, Arc<Service>>) -> Reply<Value> {
    let service = s.inner().clone();
    task(move || {
        let c = service.config.read().map_err(err)?.clone();
        let target = (!c.offline && !c.connectivity_target.is_empty())
            .then_some(c.connectivity_target.as_str());
        let row = benchmark::connectivity(target, &|| false, |_| {})?;
        service
            .db
            .lock()
            .map_err(err)?
            .record("connection", "gateway", now(), &row)
            .map_err(err)?;
        Ok(row)
    })
    .await
}
#[tauri::command]
async fn ai_status(s: State<'_, Arc<Service>>) -> Reply<()> {
    let c = s.config.read().map_err(err)?.clone();
    if !c.ai {
        return Err("Enable local AI in Settings.".into());
    }
    task(move || assistant::status(&c.model).map(|_| ())).await
}
#[tauri::command]
async fn ask(s: State<'_, Arc<Service>>, prompt: String, include_events: bool) -> Reply<String> {
    let service = s.inner().clone();
    task(move||{
 let c=service.config.read().map_err(err)?.clone();if !c.ai{return Err("Enable local AI in Settings.".into())}
 let (sample,summary)={let latest=service.latest.read().map_err(err)?;let sample=json!({"metrics":latest["metrics"],"cpu":latest["cpu"]["name"],"os":latest["os"],"health":latest["health"],"issues":latest["issues"]});let data=service.db.lock().map_err(err)?.history(now()-3600,now(),now()).map_err(err)?;let mut summary=serde_json::Map::new();for key in ["cpu","gpu","memory","cpuTemp","gpuTemp","disk","score"]{let mut total=0.;let mut n=0.;let mut lo=f64::INFINITY;let mut hi=f64::NEG_INFINITY;for pt in data.as_array().into_iter().flatten(){let v=&pt["values"][key];if let (Some(avg),Some(min),Some(max),Some(count))=(v["avg"].as_f64(),v["min"].as_f64(),v["max"].as_f64(),v["n"].as_f64()){total+=avg*count;n+=count;lo=lo.min(min);hi=hi.max(max);}}if n>0.{summary.insert(key.into(),json!({"average":total/n,"min":lo,"max":hi,"samples":n}));}}(sample,summary)};
 let events=if include_events{windows::events().ok().and_then(|v|v.as_array().map(|items|items.iter().take(8).map(|row|json!({"time":row["ts"],"id":row["Id"],"source":row["ProviderName"],"message":row["Message"].as_str().unwrap_or("").chars().take(400).collect::<String>()})).collect::<Vec<_>>()))}else{None};
 assistant::ask(&c.model,&prompt,json!({"current":sample,"lastHour":summary,"events":events}))
 }).await
}
#[tauri::command]
async fn save_file(filename: String, bytes: Vec<u8>) -> Reply<bool> {
    if bytes.len() > 40_000_000 || filename.len() > 140 || filename.contains(['\\', '/', ':']) {
        return Err("Invalid export file.".into());
    }
    task(move || {
        let ext = match Path::new(&filename).extension().and_then(|e| e.to_str()) {
            Some(e @ ("json" | "html" | "pdf" | "csv")) => e,
            _ => return Err("Unsupported export format.".into()),
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(&filename)
            .add_filter(ext, &[ext])
            .save_file()
        else {
            return Ok(false);
        };
        use std::io::Write;
        let mut tmp =
            tempfile::NamedTempFile::new_in(path.parent().ok_or("Invalid output folder.")?)
                .map_err(err)?;
        tmp.write_all(&bytes).map_err(err)?;
        tmp.persist(path).map_err(err)?;
        Ok(true)
    })
    .await
}
#[tauri::command]
async fn choose_database() -> Reply<Option<String>> {
    task(|| {
        Ok(rfd::FileDialog::new()
            .pick_folder()
            .map(|p| p.to_string_lossy().into_owned()))
    })
    .await
}
#[tauri::command]
async fn import_settings(s: State<'_, Arc<Service>>) -> Reply<Option<Config>> {
    let service = s.inner().clone();
    task(move || {
        let Some(p) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
        else {
            return Ok(None);
        };
        if p.metadata().map_err(err)?.len() > 131072 {
            return Err("The settings file is too large.".into());
        }
        let mut c = config::read(&p)?;
        let active = service.config.read().map_err(err)?.clone();
        c.database_dir = active.database_dir;
        c.startup = active.startup;
        service.persist(c).map(Some)
    })
    .await
}

fn self_check(path: &Path) -> Reply<()> {
    let dir =
        tempfile::tempdir_in(path.parent().ok_or("Choose an output folder.")?).map_err(err)?;
    let service = Service::open(dir.path().to_path_buf())?;
    let mut monitor = monitor::Monitor::new();
    monitor.sample(&Value::Null);
    std::thread::sleep(Duration::from_millis(350));
    let mut snap = monitor.sample(&Value::Null);
    let score = health::assess(&snap, &Config::default());
    snap["metrics"]["score"] = score["score"].clone();
    let ts = now();
    let mut db = service.db.lock().map_err(err)?;
    db.sample(ts, &snap["metrics"]).map_err(err)?;
    let h = db.history(ts - 2, ts + 1, ts).map_err(err)?;
    if h.as_array().is_none_or(Vec::is_empty)
        || snap["metrics"]["cpu"].is_null()
        || snap["metrics"]["memory"].is_null()
    {
        return Err("Live telemetry or SQLite self-check failed.".into());
    }
    let summary = json!({"status":"passed","liveCpu":snap["metrics"]["cpu"],"liveMemory":snap["metrics"]["memory"],"volumes":snap["disks"].as_array().map(Vec::len),"processCount":snap["metrics"]["processes"],"batteryApi":!snap["battery"].is_null(),"sqliteSamples":h.as_array().map(Vec::len),"score":score["score"],"startupRegistryReadable":windows::startup().is_array()});
    std::fs::write(path, serde_json::to_vec_pretty(&summary).map_err(err)?).map_err(err)
}
fn sensor_check(path: &Path) -> Reply<()> {
    let drives = storage::drives();
    let native = telemetry::native(&drives);
    let windows = windows::sensors().unwrap_or(Value::Null);
    let mut providers = telemetry::combine(windows, &native);
    let passive = sensors::Wmi::new().poll(false);
    let (graphics, graphics_error) = gpu::Monitor::new().poll();
    let fast = json!({"graphics":graphics,"graphicsError":graphics_error});
    telemetry::thermals(&mut providers, &fast, &passive, false);
    let sample = monitor::Monitor::new().sample(&providers);
    let device_fields = |key: &str, fields: &[&str]| {
        providers[key]
            .as_array()
            .into_iter()
            .flatten()
            .map(|row| {
                fields
                    .iter()
                    .map(|&field| (field.to_owned(), json!(!row[field].is_null())))
                    .collect::<serde_json::Map<_, _>>()
            })
            .collect::<Vec<_>>()
    };
    let summary = json!({
        "cpuPackageTemperature": sample["metrics"]["cpuTemp"].is_number(),
        "cpuPackagePower": sample["metrics"]["cpuPower"].is_number(),
        "fastestFan": sample["metrics"]["fan"].is_number(),
        "driveTemperature": sample["metrics"]["driveTemp"].is_number(),
        "driveHealthFields": device_fields("driveHealth", &["status","temperature","wear","hours","startStop","powerCycles","readErrors","writeErrors","mediaErrors"]),
        "batteryPresent": sample["battery"]["present"],
        "batteryOnAc": sample["battery"]["ac"],
        "batteryCapacityHealth": sample["battery"]["health"].is_number(),
        "batteryRemainingTime": sample["battery"]["remaining"].is_number(),
        "wifiFields": device_fields("wifi", &["state","signal","rssi","channel","frequencyMHz","rxMbps","txMbps"]),
        "providers": providers["probes"],
    });
    std::fs::write(path, serde_json::to_vec_pretty(&summary).map_err(err)?).map_err(err)
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    if let Some(flag) = args.next() {
        if flag == "--self-check" || flag == "--sensor-check" {
            if let Some(path) = args.next() {
                let check = if flag == "--sensor-check" {
                    sensor_check
                } else {
                    self_check
                };
                match check(Path::new(&path)) {
                    Ok(()) => return,
                    Err(e) => {
                        let _ = std::fs::write(path, e);
                        std::process::exit(1);
                    }
                }
            }
        }
    }
    let mut context = tauri::generate_context!();
    if std::env::args().any(|arg| arg == "--background") {
        context.config_mut().app.windows[0].create = false;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            snapshot,
            enable_thermal,
            processes,
            process_publisher,
            settings,
            save_settings,
            pause,
            history,
            records,
            mark_issue,
            clear_history,
            inventory,
            event_logs,
            startup_entries,
            startup_toggle,
            end_process,
            run_diagnostics,
            start_benchmark,
            benchmark_status,
            cancel_benchmark,
            gpu_result,
            connectivity,
            ai_status,
            ask,
            save_file,
            choose_database,
            import_settings
        ])
        .setup(|app| {
            let dir = match std::env::var_os("SIGNALDESK_DATA_DIR") {
                Some(dir) => PathBuf::from(dir),
                None => app.path().app_local_data_dir()?,
            };
            let service = Arc::new(Service::open(dir).map_err(std::io::Error::other)?);
            service.thermal.lock().unwrap().configure();
            app.manage(service.clone());
            let open = MenuItem::with_id(app, "open", "Open Silicon Desk", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause", "Pause / resume", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Exit Silicon Desk", true, None::<&str>)?;
            let metrics =
                MenuItem::with_id(app, "status", "Show current metrics", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&metrics, &open, &pause, &quit])?;
            let mut tray = TrayIconBuilder::with_id("monitor")
                .menu(&menu)
                .tooltip("Silicon Desk · Monitoring")
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => reveal(app),
                    "status" => {
                        let state = app.state::<Arc<Service>>();
                        let sample = state.latest.read().unwrap();
                        let pct = |name: &str| {
                            sample["metrics"][name]
                                .as_f64()
                                .map(|n| format!("{n:.0}%"))
                                .unwrap_or_else(|| "Unavailable".into())
                        };
                        let status = if state.paused.load(Relaxed) {
                            " · paused"
                        } else {
                            ""
                        };
                        let _ = app
                            .notification()
                            .builder()
                            .title(format!("Silicon Desk{status}"))
                            .body(format!(
                                "CPU {} · RAM {} · GPU {}",
                                pct("cpu"),
                                pct("memory"),
                                pct("gpu")
                            ))
                            .show();
                    }
                    "pause" => {
                        toggle_pause(app, &app.state::<Arc<Service>>());
                    }
                    "quit" => {
                        app.state::<Arc<Service>>().exit.store(true, Relaxed);
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        reveal(tray.app_handle())
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            let handle = app.handle().clone();
            std::thread::spawn(move || poll(handle, service));
            Ok(())
        })
        .on_window_event(|w, e| {
            let s = w.state::<Arc<Service>>();
            match e {
                WindowEvent::CloseRequested { api, .. } => {
                    if s.config.read().unwrap().tray && !s.exit.load(Relaxed) {
                        api.prevent_close();
                        let _ = w.destroy();
                    } else {
                        s.exit.store(true, Relaxed);
                        w.app_handle().exit(0);
                    }
                }
                WindowEvent::Resized(_) => {
                    if s.config.read().unwrap().tray && w.is_minimized().unwrap_or(false) {
                        let _ = w.destroy();
                    }
                }
                _ => {}
            }
        })
        .build(context)
        .expect("desktop runtime")
        .run(|app, event| {
            if let RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                if !app.state::<Arc<Service>>().exit.load(Relaxed) {
                    api.prevent_exit();
                }
            }
        });
}
