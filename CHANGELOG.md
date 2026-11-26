# Changelog

## 1.1.0

- Renamed the desktop application to Silicon Desk while retaining the existing data location and installer upgrade ID.
- Added an installed, separately elevated LibreHardwareMonitor CPU sensor helper and an in-app permission prompt.
- Added persistent LibreHardwareMonitor WMI and direct NVIDIA NVML temperature reads guarded by Windows device power state.
- Added live CPU and GPU temperature cards, GPU hotspot display, and power-state diagnostics.
- Reworked Drive Health around a drive selector and added NVMe lifetime fields and raw ATA SMART attributes where available.

## 1.0.1

- Added read-only NVMe and ATA health queries and Windows drive-temperature fallback.
- Added Windows battery-driver and aggregate battery reporting for capacity, wear and discharge runtime estimates.
- Replaced text parsing with native Windows WLAN queries for adapter state, signal, channel and link rates.
- Improved CPU package power and temperature sensor matching and added an optional local LibreHardwareMonitor connector.
- Added provider explanations for unavailable drive, battery, Wi-Fi and cooling readings.

## 1.0.0

- Introduced the SignalDesk desktop application, with a customizable overview and 19 pages for system telemetry, troubleshooting and settings.
- Added SQLite performance history, local reporting, rule-based diagnostics, Windows event browsing, alerts, tray monitoring and benchmarks.
- Added native Windows, DXGI, WMI, LibreHardwareMonitor and NVIDIA provider support, with unavailable readings displayed explicitly.
- Added optional Ollama integration and x64 Windows installation packages.
