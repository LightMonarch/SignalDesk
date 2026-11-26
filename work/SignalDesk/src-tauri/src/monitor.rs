use crate::{native, sensors, telemetry, windows};
use serde_json::{json, Value};
use std::time::Instant;
use sysinfo::{
    DiskRefreshKind, Disks, Networks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind,
};

pub struct Monitor {
    sys: System,
    disks: Disks,
    nets: Networks,
    last: Instant,
    ready: bool,
    freq_tick: u8,
    cpu_times: Option<native::CpuTimes>,
}
fn list(data: &Value) -> impl Iterator<Item = &Value> {
    data.as_array().into_iter().flatten()
}
fn peak(data: impl Iterator<Item = f64>) -> Option<f64> {
    data.filter(|x| x.is_finite()).reduce(f64::max)
}
fn sensor(s: &Value, prefix: &str, kind: &str, match_name: impl Fn(&str) -> bool) -> Option<f64> {
    peak(
        list(&s["sensors"])
            .filter(|r| {
                r["Identifier"]
                    .as_str()
                    .is_some_and(|id| id.starts_with(prefix))
                    && r["SensorType"] == kind
                    && match_name(r["Name"].as_str().unwrap_or("").to_lowercase().as_str())
            })
            .filter_map(|r| r["Value"].as_f64()),
    )
}
impl Monitor {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_all();
        Self {
            sys,
            disks: Disks::new(),
            nets: Networks::new_with_refreshed_list(),
            last: Instant::now(),
            ready: false,
            freq_tick: 0,
            cpu_times: native::CpuTimes::get(),
        }
    }
    pub fn sample(&mut self, s: &Value) -> Value {
        let elapsed = self.last.elapsed().as_secs_f64().max(0.001);
        self.last = Instant::now();
        let ready = self.ready && elapsed <= 75.;
        self.ready = true;
        self.sys.refresh_cpu_usage();
        let times = native::CpuTimes::get();
        let cpu_usage = if ready {
            times
                .zip(self.cpu_times)
                .and_then(|(current, previous)| current.usage_since(previous))
                .or_else(|| {
                    (!self.sys.cpus().is_empty()).then(|| f64::from(self.sys.global_cpu_usage()))
                })
        } else {
            None
        };
        self.cpu_times = times;
        if self.freq_tick == 0 {
            self.sys.refresh_cpu_frequency();
        }
        self.freq_tick = (self.freq_tick + 1) % 5;
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_disk_usage()
                .with_exe(UpdateKind::OnlyIfNotSet),
        );
        self.disks
            .refresh_specifics(true, DiskRefreshKind::everything());
        self.nets.refresh(true);
        let cpu = self.sys.cpus().first();
        let cores:Vec<_>=self.sys.cpus().iter().map(|c|json!({"name":c.name(),"usage":ready.then(||c.cpu_usage()),"clock":(c.frequency()>0).then(||c.frequency())})).collect();
        let total = self.sys.total_memory();
        let used = self.sys.used_memory();
        let mut memory = windows::memory();
        if memory.is_null() {
            memory = json!({})
        }
        memory["total"] = json!(total);
        memory["used"] = json!(used);
        memory["available"] = json!(self.sys.available_memory());
        memory["swapUsed"] = json!(self.sys.used_swap());
        memory["swapTotal"] = json!(self.sys.total_swap());
        let disks:Vec<_>=self.disks.list().iter().filter(|d|d.total_space()>0).map(|d|{
 let mount=d.mount_point().to_string_lossy();let drive=mount.trim_end_matches('\\');
 let perf=list(&s["diskPerf"]).find(|p|p["Name"]==drive);
 json!({"name":d.name().to_string_lossy(),"mount":mount,"fileSystem":d.file_system().to_string_lossy(),"kind":d.kind().to_string(),"total":d.total_space(),"free":d.available_space(),"usedPct":100.*(1.-d.available_space() as f64/d.total_space() as f64),"read":perf.and_then(|p|p["DiskReadBytesPersec"].as_f64()),"write":perf.and_then(|p|p["DiskWriteBytesPersec"].as_f64()),"active":perf.and_then(|p|p["PercentIdleTime"].as_f64()).map(|v|(100.-v).clamp(0.,100.))})
 }).collect();
        let process_gpu: std::collections::HashMap<_, _> = list(&s["gpuProcesses"])
            .filter_map(|p| Some((p["pid"].as_u64()? as u32, p["utilization"].as_f64()?)))
            .collect();
        let mut procs:Vec<_>=self.sys.processes().iter().map(|(id,p)|{
 let io=p.disk_usage();json!({"pid":id.as_u32(),"gpu":process_gpu.get(&id.as_u32()),"name":p.name().to_string_lossy(),"started":p.start_time(),"cpu":ready.then(||p.cpu_usage()/cores.len().max(1) as f32),"memory":p.memory(),"disk":ready.then(||(io.read_bytes+io.written_bytes) as f64/elapsed),"path":p.exe(),"status":format!("{:?}",p.status())})
 }).collect();
        procs.sort_by(|a, b| b["memory"].as_u64().cmp(&a["memory"].as_u64()));
        let adapters:Vec<_>=self.nets.iter().filter(|(name,_)|!name.to_lowercase().contains("loopback")).map(|(name,n)|json!({"name":name,"rx":ready.then(||n.received() as f64/elapsed),"tx":ready.then(||n.transmitted() as f64/elapsed),"rxTotal":n.total_received(),"txTotal":n.total_transmitted(),"ips":n.ip_networks().iter().filter(|p|!p.addr.is_loopback()).map(|ip|ip.addr.to_string()).collect::<Vec<_>>() })).collect();
        let connected = native::network_connected().or_else(|| {
            s["adapters"].as_array().filter(|a| !a.is_empty()).map(|a| {
                a.iter()
                    .any(|v| v["NetConnectionStatus"] == 2 && v["PhysicalAdapter"] == true)
            })
        });
        let battery = telemetry::battery_sample(s);
        let gpu_temp = sensors::gpu_reading(s, false);
        let gpu_hotspot = sensors::gpu_reading(s, true);
        let cpu_temp = sensors::cpu_reading(&s["sensors"], "Temperature");
        let cpu_power = sensors::cpu_reading(&s["sensors"], "Power");
        let gpu_load = sensor(s, "/gpu", "Load", |name| name == "gpu core")
            .or_else(|| peak(list(&s["gpuEngines"]).filter_map(|e| e["utilization"].as_f64())))
            .or_else(|| peak(list(&s["nvidia"]).filter_map(|d| d["utilization"].as_f64())))
            .map(|n| n.clamp(0., 100.));
        let fan = sensors::fastest_fan(&s["sensors"]);
        let drive_temp = peak(
            list(&s["driveHealth"])
                .filter_map(|row| row["temperature"].as_f64())
                .chain(
                    list(&s["sensors"])
                        .filter(|row| sensors::is_drive_sensor(row))
                        .filter_map(|row| row["Value"].as_f64()),
                )
                .filter(|v| (-60. ..=190.).contains(v)),
        );
        let rate = |k: &str| {
            disks
                .iter()
                .filter_map(|d| d[k].as_f64())
                .reduce(|a, b| a + b)
        };
        let disk_pct = peak(disks.iter().filter_map(|d| d["usedPct"].as_f64()));
        let adapter_sum = |k: &str| {
            (!adapters.is_empty() && ready)
                .then(|| adapters.iter().filter_map(|a| a[k].as_f64()).sum::<f64>())
        };
        json!({"ts":crate::now(),"host":System::host_name(),"os":System::long_os_version(),"boot":System::boot_time(),"uptime":System::uptime(),
        "cpu":{"name":cpu.map(|c|c.brand().trim()),"vendor":cpu.map(|c|c.vendor_id()),"architecture":System::cpu_arch(),"physical":self.sys.physical_core_count(),"logical":cores.len(),"cores":cores,"power":cpu_power.as_ref().map(|(v,_)|*v),"powerReading":cpu_power.as_ref().map(|(_,row)|row),"temperatureReading":cpu_temp.as_ref().map(|(_,row)|row)},
        "memory":memory,"disks":disks,"battery":battery,"adapters":adapters,"connected":connected,"providers":s,"fanReading":fan.as_ref().map(|(_,row)|row),"gpuReading":gpu_temp.as_ref().map(|(_,row)|row),"gpuHotspotReading":gpu_hotspot.as_ref().map(|(_,row)|row),"thermalAccess":s["thermalAccess"],"processes":procs,
        "metrics":{"cpu":cpu_usage,"gpu":gpu_load,"memory":(total>0).then(||100.*used as f64/total as f64),"disk":disk_pct,"cpuTemp":cpu_temp.as_ref().map(|(v,_)|*v),"cpuPower":cpu_power.as_ref().map(|(v,_)|*v),"gpuTemp":gpu_temp.as_ref().map(|(v,_)|*v),"gpuHotspot":gpu_hotspot.as_ref().map(|(v,_)|*v),"fan":fan.as_ref().map(|(v,_)|*v),"driveTemp":drive_temp,"diskRead":rate("read"),"diskWrite":rate("write"),"battery":battery["pct"],"batteryHealth":battery["health"],"rx":adapter_sum("rx"),"tx":adapter_sum("tx"),"processes":procs.len()}
        })
    }
}
pub fn end(pid: u32, started: u64) -> Result<(), String> {
    if pid <= 4 || pid == std::process::id() || started == 0 {
        return Err("This system process cannot be ended here.".into());
    }
    let mut sys = System::new();
    let id = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[id]),
        true,
        ProcessRefreshKind::nothing(),
    );
    let p = sys
        .process(id)
        .filter(|p| p.start_time() == started)
        .ok_or("The process has exited or changed.")?;
    if [
        "csrss.exe",
        "wininit.exe",
        "winlogon.exe",
        "services.exe",
        "lsass.exe",
        "smss.exe",
        "system",
        "registry",
    ]
    .iter()
    .any(|n| p.name().to_string_lossy().eq_ignore_ascii_case(n))
    {
        return Err("This system process is protected.".into());
    }
    if p.kill() {
        Ok(())
    } else {
        Err("Windows refused to end this process. Check your permissions.".into())
    }
}
