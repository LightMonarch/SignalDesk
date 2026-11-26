use serde_json::{json, Value};
use std::{os::windows::process::CommandExt, path::PathBuf, process::Command};
use windows_sys::Win32::System::{
    Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
    ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION},
};
use winreg::{enums::*, RegKey};

pub fn script(code: &str) -> Result<Value, String> {
    let exe = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    use std::{
        io::Read,
        process::Stdio,
        time::{Duration, Instant},
    };
    let mut provider = Command::new(exe)
        .creation_flags(0x08000000)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("[Console]::OutputEncoding=[Text.Encoding]::UTF8; $ProgressPreference='SilentlyContinue'; {code}"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn().map_err(|e|e.to_string())?;
    fn read_output(mut source: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut output = Vec::new();
            let _ = source.read_to_end(&mut output);
            output
        })
    }
    let stdout = read_output(provider.stdout.take().unwrap());
    let stderr = read_output(provider.stderr.take().unwrap());
    let start = Instant::now();
    let status = loop {
        if let Some(status) = provider.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if start.elapsed() >= Duration::from_secs(15) {
            let _ = provider.kill();
            let _ = provider.wait();
            return Err("The Windows provider did not respond within 15 seconds.".into());
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    let bytes = stdout
        .join()
        .map_err(|_| "Windows provider output was interrupted.")?;
    let errors = stderr
        .join()
        .map_err(|_| "Windows provider output was interrupted.")?;
    if !status.success() {
        return Err(String::from_utf8_lossy(&errors)
            .trim()
            .chars()
            .take(1000)
            .collect());
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| format!("Windows provider returned no readable data: {e}"))
}
pub fn battery() -> Value {
    let mut p: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    if unsafe { GetSystemPowerStatus(&mut p) } == 0 {
        return Value::Null;
    }
    json!({"present":if p.BatteryFlag==255{None}else{Some(p.BatteryFlag&128==0)},"pct":(p.BatteryLifePercent<=100).then_some(p.BatteryLifePercent),"charging":(p.BatteryFlag!=255).then_some(p.BatteryFlag&8!=0),"ac":(p.ACLineStatus<=1).then_some(p.ACLineStatus==1),"remaining":(p.BatteryLifeTime!=u32::MAX).then_some(p.BatteryLifeTime)})
}
pub fn memory() -> Value {
    let size = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
    let mut p: PERFORMANCE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetPerformanceInfo(&mut p, size) } == 0 {
        return Value::Null;
    }
    json!({"committed":p.CommitTotal*p.PageSize,"commitLimit":p.CommitLimit*p.PageSize,"cache":p.SystemCache*p.PageSize})
}

pub fn sensors() -> Result<Value, String> {
    script(
        r#"
$ErrorActionPreference='SilentlyContinue'
function cim($ns,$c){Get-CimInstance -Namespace $ns -ClassName $c -OperationTimeoutSec 2 -ErrorAction SilentlyContinue}
$g=@(cim 'root\cimv2' 'Win32_VideoController' | Select-Object Name,DriverVersion,PNPDeviceID,AdapterCompatibility,ConfigManagerErrorCode,AdapterRAM,CurrentRefreshRate)
$media=@(cim 'root\cimv2' 'Win32_DiskDrive' | Select-Object Model,Manufacturer,Size,InterfaceType,MediaType,Partitions,Index,Status)
$health=@()
if(Get-Command Get-PhysicalDisk -ErrorAction SilentlyContinue){$health=@(Get-PhysicalDisk -ErrorAction SilentlyContinue | ForEach-Object {$d=$_; $r=$d | Get-StorageReliabilityCounter -ErrorAction SilentlyContinue; [ordered]@{name=[string]$d.FriendlyName;id=[string]$d.DeviceId;bus=[string]$d.BusType;status=[string]$d.HealthStatus;wear=$r.Wear;temperature=$r.Temperature;hours=$r.PowerOnHours;readErrors=$r.ReadErrorsTotal;writeErrors=$r.WriteErrorsTotal;startStop=$r.StartStopCycleCount;powerCycles=$r.PowerCycleCount;source="Windows storage reliability"}})}
$engineRows=@(cim 'root\cimv2' 'Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine')
$eng=@($engineRows | Group-Object {if($_.Name -match 'luid_(.+?)_phys_(\d+)_eng_(\d+)_engtype_(.+?)(?:#\d+)?$'){$Matches[1]+'~'+$Matches[2]+'~'+$Matches[3]+'~'+$Matches[4]}} | Where-Object Name | ForEach-Object { $v=$_.Name.Split('~'); @{adapter=$v[0];node=$v[1];engine=$v[2];kind=$v[3];utilization=[Math]::Min(100,($_.Group | Measure-Object UtilizationPercentage -Sum).Sum)} })
$gpuProcesses=@($engineRows | Group-Object {if($_.Name -match '^pid_(\d+)_'){$Matches[1]}} | Where-Object Name | ForEach-Object {@{pid=[int]$_.Name;utilization=($_.Group | Measure-Object UtilizationPercentage -Maximum).Maximum}})
$mem=@(cim 'root\cimv2' 'Win32_PerfFormattedData_GPUPerformanceCounters_GPUAdapterMemory' | Select-Object Name,DedicatedUsage,SharedUsage)
$st=@(cim 'root\wmi' 'BatteryStatus' | Where-Object Active | Select-Object InstanceName,ChargeRate,DischargeRate,RemainingCapacity,Discharging)
$sd=@(cim 'root\wmi' 'BatteryStaticData' | Select-Object InstanceName,DesignedCapacity,ManufactureName,DeviceName)
$fc=@(cim 'root\wmi' 'BatteryFullChargedCapacity' | Select-Object InstanceName,FullChargedCapacity)
$cy=@(cim 'root\wmi' 'BatteryCycleCount' | Select-Object InstanceName,CycleCount)
$perf=@(cim 'root\cimv2' 'Win32_PerfFormattedData_PerfDisk_LogicalDisk' | Where-Object {$_.Name -ne '_Total'} | Select-Object Name,DiskReadBytesPersec,DiskWriteBytesPersec,PercentIdleTime)
$adapters=@(cim 'root\cimv2' 'Win32_NetworkAdapter' | Where-Object {$_.NetConnectionID} | Select-Object Name,NetConnectionID,NetConnectionStatus,Speed,MACAddress,PhysicalAdapter)
 $pow=Join-Path $env:SystemRoot 'System32\powercfg.exe'
$power=$null;if(Test-Path $pow){$raw=& $pow /getactivescheme 2>$null;if($raw -match '\(([^)]+)\)\s*$'){$power=$Matches[1]}}
[ordered]@{powerPlan=$power;gpus=$g;physicalDisks=$media;driveHealth=$health;gpuEngines=$eng;gpuProcesses=$gpuProcesses;gpuMemory=$mem;batteryStatus=$st;batteryDesign=$sd;batteryFull=$fc;batteryCycles=$cy;diskPerf=$perf;adapters=$adapters} | ConvertTo-Json -Depth 7 -Compress
"#,
    )
}
pub fn inventory() -> Result<Value, String> {
    script(
        r#"
$ErrorActionPreference='SilentlyContinue'
function w($c){Get-CimInstance -ClassName $c -OperationTimeoutSec 3 -ErrorAction SilentlyContinue}
$secure=$null;try{$secure=Confirm-SecureBootUEFI -ErrorAction Stop}catch{}
$power=Get-CimInstance -Namespace 'root\cimv2\power' -ClassName Win32_PowerPlan -OperationTimeoutSec 3 -ErrorAction SilentlyContinue | Where-Object IsActive | Select-Object -ExpandProperty ElementName -First 1
[ordered]@{
 computer=w Win32_ComputerSystem | Select-Object Manufacturer,Model,SystemType;
 windows=w Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber,OSArchitecture,@{n='Installed';e={if($_.InstallDate){$_.InstallDate.ToString('o')}}},@{n='LastBoot';e={if($_.LastBootUpTime){$_.LastBootUpTime.ToString('o')}}};
 bios=w Win32_BIOS | Select-Object Manufacturer,SMBIOSBIOSVersion;
 board=w Win32_BaseBoard | Select-Object Manufacturer,Product,Version;
 cpu=@(w Win32_Processor | Select-Object Name,Manufacturer,MaxClockSpeed,CurrentClockSpeed,L2CacheSize,L3CacheSize,NumberOfCores,NumberOfLogicalProcessors);
 memory=@(w Win32_PhysicalMemory | Select-Object Manufacturer,PartNumber,DeviceLocator,Capacity,ConfiguredClockSpeed,Speed,SMBIOSMemoryType);
 slots=(w Win32_PhysicalMemoryArray | Where-Object Use -eq 3 | Measure-Object MemoryDevices -Sum).Sum;
 displays=@(w Win32_DesktopMonitor | Select-Object Name,MonitorManufacturer,ScreenWidth,ScreenHeight);
 devices=@(w Win32_PnPEntity | Where-Object {$_.Present -eq $true -and $null -ne $_.ConfigManagerErrorCode -and $_.ConfigManagerErrorCode -ne 0 -and $_.ConfigManagerErrorCode -ne 22} | Select-Object -First 50 Name,ConfigManagerErrorCode);
 secureBoot=$secure; tpm=Get-CimInstance -Namespace 'root\cimv2\Security\MicrosoftTpm' -ClassName Win32_Tpm -OperationTimeoutSec 3 -ErrorAction SilentlyContinue | Select-Object SpecVersion,IsEnabled_InitialValue; powerPlan=$power
} | ConvertTo-Json -Depth 7 -Compress
"#,
    )
}
pub fn events() -> Result<Value, String> {
    events_page(None, None, 0)
}
pub fn events_page(start: Option<i64>, end: Option<i64>, page: u32) -> Result<Value, String> {
    let now = crate::now();
    let start = start.unwrap_or(now - 7 * 86400);
    let end = end.unwrap_or(now + 1);
    if start < 0 || end <= start || end > now + 86400 || page > 99 {
        return Err("Select a valid event-log date range and page.".into());
    }
    let input=format!("$first=[DateTimeOffset]::FromUnixTimeSeconds({start}).LocalDateTime; $last=[DateTimeOffset]::FromUnixTimeSeconds({end}).LocalDateTime; $skip={}; $limit={}; ",page*60,(page+1)*60);
    script(
        &(input
            + r#"
$ErrorActionPreference='Stop'
$all=@()
foreach($log in @('System','Application')){
 try {$all+=@(Get-WinEvent -FilterHashtable @{LogName=$log;Level=1,2,3;StartTime=$first;EndTime=$last} -MaxEvents $limit -ErrorAction Stop)}catch{if($_.FullyQualifiedErrorId -notmatch 'NoMatchingEventsFound'){throw}}
}
$x=@($all | Sort-Object TimeCreated -Descending | Select-Object -Skip $skip -First 60 | Select-Object @{n='ts';e={([DateTimeOffset]$_.TimeCreated).ToUnixTimeSeconds()}},Id,RecordId,ProviderName,LogName,LevelDisplayName,Message)
ConvertTo-Json -InputObject $x -Depth 4 -Compress
"#),
    )
}
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
pub fn startup() -> Value {
    let mut list = vec![];
    for (scope, root) in [
        ("Current user", HKEY_CURRENT_USER),
        ("All users", HKEY_LOCAL_MACHINE),
    ] {
        let parent = RegKey::predef(root);
        for (view, flags) in [("Native", 0), ("32-bit", KEY_WOW64_32KEY)] {
            if scope == "Current user" && view == "32-bit" {
                continue;
            }
            if let Ok(key) = parent.open_subkey_with_flags(RUN, KEY_READ | flags) {
                let approved_path = if view == "32-bit" {
                    format!("{APPROVED}32")
                } else {
                    APPROVED.into()
                };
                let approvals = parent.open_subkey(approved_path).ok();
                for (name, value) in key.enum_values().flatten() {
                    if scope == "All users"
                        && view == "32-bit"
                        && list
                            .iter()
                            .any(|item: &Value| item["scope"] == scope && item["name"] == name)
                    {
                        continue;
                    }
                    let status = approvals
                        .as_ref()
                        .and_then(|a| a.get_raw_value(&name).ok())
                        .and_then(|v| v.bytes.first().copied())
                        .map(|v| v & 1 == 0)
                        .unwrap_or(true);
                    list.push(json!({"name":name,"path":value.to_string(),"scope":scope,"kind":"Run registry","view":view,"enabled":status,"editable":scope=="Current user"}));
                }
            }
        }
    }
    for (scope, base) in [
        ("Current user", std::env::var_os("APPDATA")),
        ("All users", std::env::var_os("ProgramData")),
    ] {
        if let Some(base) = base {
            let folder = PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs\Startup");
            if let Ok(entries) = std::fs::read_dir(folder) {
                for e in entries.flatten() {
                    if e.file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case("desktop.ini")
                        || !e.path().is_file()
                    {
                        continue;
                    }
                    list.push(json!({"name":e.file_name().to_string_lossy(),"path":e.path(),"scope":scope,"kind":"Startup folder","view":"Native","enabled":null,"editable":false}));
                }
            }
        }
    }
    json!(list)
}
pub fn toggle_startup(name: &str, enabled: bool) -> Result<(), String> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let key = user.open_subkey(RUN).map_err(|e| e.to_string())?;
    if name.eq_ignore_ascii_case("SignalDesk") {
        return Err("Change SignalDesk startup in Settings.".into());
    }
    key.get_raw_value(name).map_err(|e| e.to_string())?;
    let (a, _) = user.create_subkey(APPROVED).map_err(|e| e.to_string())?;
    let mut bytes = vec![0; 12];
    bytes[0] = if enabled { 2 } else { 3 };
    if !enabled {
        let ft = (crate::now() as u64 + 11_644_473_600) * 10_000_000;
        bytes[4..].copy_from_slice(&ft.to_le_bytes());
    }
    a.set_raw_value(
        name,
        &winreg::RegValue {
            bytes,
            vtype: REG_BINARY,
        },
    )
    .map_err(|e| e.to_string())
}

pub fn publisher(path: &str) -> Result<Value, String> {
    let path = path.replace('\u{27}', "''");
    let code=format!("$ErrorActionPreference='Stop'; $file='{path}'; $v=[Diagnostics.FileVersionInfo]::GetVersionInfo($file); $sig=Get-AuthenticodeSignature -LiteralPath $file; @{{company=$v.CompanyName; status=$sig.Status.ToString(); signer=$sig.SignerCertificate.Subject}} | ConvertTo-Json -Compress");
    script(&code)
}
