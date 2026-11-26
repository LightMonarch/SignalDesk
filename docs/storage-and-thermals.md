# Storage tools and thermal sensors

## Which storage tool to use

| Tool | Best fit | What it reads |
| --- | --- | --- |
| CrystalDiskInfo | A quick visual check of a Windows drive | SMART attributes and vendor-specific health rules for HDDs and SSDs. The app supports NVMe and some USB bridges and RAID controllers. |
| smartmontools / smartctl | Detailed drive diagnostics, logs, and scripted checks | ATA, SCSI and NVMe SMART data, controller-specific access, and drive self-tests where supported. |
| HWiNFO | A second opinion on laptop temperatures and whole-system telemetry | CPU, graphics, motherboard and storage sensors with logging. It is useful when comparing support for the same laptop. |

CrystalDiskInfo is the strongest starting point here for a drive-only Windows GUI. For deeper storage diagnostics, use smartctl. For hardware temperatures across the laptop, use HWiNFO or LibreHardwareMonitor. Silicon Desk adopts the per-drive summary and raw-attribute view while retaining local monitoring and historical charts.

[CrystalDiskInfo supported drives](https://crystalmark.info/en/software/CrystalDiskInfo/) · [CrystalDiskInfo health rules](https://crystalmark.info/en/software/crystaldiskinfo/crystaldiskinfo-health-status/) · [smartmontools upstream](https://github.com/smartmontools/smartmontools) · [HWiNFO capabilities](https://www.hwinfo.com/about-software/)

## CPU package temperature

Windows system counters readily expose CPU utilization. Package and core temperatures come from hardware registers that need a supported driver and processor-specific decoding. Before Silicon Desk 1.1, the application depended on a separately running LibreHardwareMonitor provider. A missing or inaccessible provider returned no package reading.

The Setup EXE installs the LibreHardwareMonitor CPU reader under the standard Silicon Desk Program Files directory. Select **Enable sensors** in the app, then approve the Windows prompt for the helper. The helper uses an existing PawnIO installation. When the driver is missing, install the signed edition from its official source and restart Silicon Desk. The helper runs for the duration of the dashboard/tray process and pauses its hardware reads when monitoring is paused.

There are separate messages for declined Administrator access, a missing driver, driver access denied, and a provider returning no readings. A generic provider error may leave the exact Windows policy or application responsible unknown.

[LibreHardwareMonitor source and releases](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) · [Official PawnIO driver](https://pawnio.eu/)

## Laptop GPU temperatures

The default fast polling interval is about 1.5 seconds. Silicon Desk uses Windows' last reported device power state and starts direct NVML polling only when all detected NVIDIA adapters report an active D0 state. When an adapter reports a low-power or unknown state, the app stops direct polling and releases NVML. A power transition can occur between the Windows check and driver query.

Integrated and other GPU readings can arrive through LibreHardwareMonitor WMI or the opt-in localhost endpoint. The app uses a genuine available GPU core reading when a discrete device is sleeping. Hotspot is displayed independently when the provider supplies it. An external monitoring tool controls its own hardware polling behavior.

[Windows device power-data API](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/devpkey-device-powerdata) · [NVIDIA NVML documentation](https://docs.nvidia.com/deploy/nvml-api/latest/index.html)

## Drive counters

Silicon Desk shows firmware health, temperature and wear per drive. Supported NVMe devices also expose data read and written, spare capacity, power cycles, unsafe shutdowns and combined media errors. ATA SMART attributes have normalized current and worst fields alongside their raw hexadecimal values. ATA attribute meanings and thresholds can vary by manufacturer.
