# Changelog

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
