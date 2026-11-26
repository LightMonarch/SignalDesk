use serde_json::{json, Value};
use std::{
    hint::black_box,
    io::{Read, Seek, Write},
    path::Path,
    time::{Duration, Instant},
};
fn stop(flag: &dyn Fn() -> bool) -> Result<(), String> {
    if flag() {
        Err("Benchmark cancelled.".into())
    } else {
        Ok(())
    }
}
pub fn run(
    kind: &str,
    path: &Path,
    seconds: u64,
    target: Option<&str>,
    cancel: &dyn Fn() -> bool,
    progress: impl Fn(u8),
) -> Result<Value, String> {
    let at = Instant::now();
    let mut result = match kind {
        "CPU" | "Memory" => {
            let mut total = 0u64;
            let mut source = vec![0x39u8; 8 * 1024 * 1024];
            let mut target = source.clone();
            let mut seed = 0x91cdabd12345u64;
            while at.elapsed() < Duration::from_secs(seconds) {
                stop(cancel)?;
                if kind == "CPU" {
                    for _ in 0..65536 {
                        seed = seed.rotate_left(17).wrapping_mul(0x9e3779b97f4a7c15) ^ 0xace114a;
                        black_box(seed);
                    }
                    total += 65536;
                } else {
                    target.copy_from_slice(black_box(&source));
                    black_box(&target);
                    std::mem::swap(&mut source, &mut target);
                    total += source.len() as u64;
                }
                progress((at.elapsed().as_secs_f64() / seconds as f64 * 100.).min(99.) as u8);
            }
            json!({"value":total as f64/at.elapsed().as_secs_f64()/if kind=="CPU"{1e6}else{1048576.},"unit":if kind=="CPU"{"M iterations/s (single thread)"}else{"MiB/s (buffer copy)"}})
        }
        "Storage" => {
            let mut file = tempfile::NamedTempFile::new_in(path).map_err(|e| e.to_string())?;
            let mut data = vec![0u8; 1024 * 1024];
            let mut seed = 1u32;
            for chunk in data.chunks_exact_mut(4) {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                chunk.copy_from_slice(&seed.to_le_bytes());
            }
            let init = Instant::now();
            for i in 0..128u16 {
                stop(cancel)?;
                file.write_all(&data).map_err(|e| e.to_string())?;
                progress((i * 50 / 128) as u8);
            }
            file.as_file().sync_data().map_err(|e| e.to_string())?;
            let write = 128. / init.elapsed().as_secs_f64();
            file.rewind().map_err(|e| e.to_string())?;
            let init = Instant::now();
            for i in 0..128u16 {
                stop(cancel)?;
                file.read_exact(&mut data).map_err(|e| e.to_string())?;
                black_box(&data);
                progress((50 + i * 49 / 128) as u8);
            }
            json!({"value":write,"unit":"MiB/s sequential write","read":128./init.elapsed().as_secs_f64(),"note":"Read is buffered and may benefit from the Windows cache."})
        }
        "Network" => {
            let x = connectivity(target, cancel, &progress)?;
            if x["latency"].is_null() {
                return Err(format!(
                    "Target returned no ICMP replies ({}% packet loss).",
                    x["loss"]
                ));
            }
            json!({"value":x["latency"],"unit":"ms ICMP round trip","loss":x["loss"],"target":x["target"]})
        }
        _ => return Err("Unknown benchmark.".into()),
    };
    result["kind"] = json!(kind);
    result["duration"] = json!(at.elapsed().as_secs_f64());
    result["ts"] = json!(crate::now());
    progress(100);
    Ok(result)
}
pub fn connectivity(
    address: Option<&str>,
    cancel: &dyn Fn() -> bool,
    progress: impl Fn(u8),
) -> Result<Value, String> {
    let mut sum = 0.;
    let mut received = 0;
    let mut target = Value::Null;
    for i in 0..4 {
        stop(cancel)?;
        let target_script = match address.and_then(|s| s.parse::<std::net::IpAddr>().ok()) {
            Some(ip) => format!("$target='{ip}';"),
            None => String::new(),
        };
        let row = crate::windows::script(
            &(target_script
                + r#"
 if(!$target){ $default=Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue | Where-Object {$_.NextHop -ne '0.0.0.0'} | Sort-Object RouteMetric | Select-Object -First 1
 if(!$default){throw 'No IPv4 default gateway was found.'};$target=[string]$default.NextHop }
 $p=New-Object System.Net.NetworkInformation.Ping
 $ms=$null
 try{$r=$p.Send([string]$target,900);if($r.Status -eq 'Success'){$ms=$r.RoundtripTime}}catch{}
 @{target=[string]$target;ms=$ms} | ConvertTo-Json -Compress
 "#),
        )?;
        target = row["target"].clone();
        if let Some(v) = row["ms"].as_f64() {
            sum += v;
            received += 1;
        }
        progress((i + 1) * 24);
    }
    Ok(
        json!({"target":target,"loss":(4-received)*25,"latency":(received>0).then(||sum/received as f64),"ts":crate::now()}),
    )
}
