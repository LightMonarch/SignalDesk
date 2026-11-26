# Silicon Desk 1.1.0 verification

The Windows x64 build uses Rust and Tauri 2 with a React and TypeScript interface. Setup bundles the .NET Framework sensor helper and the LibreHardwareMonitor 0.9.6 runtime.

| Check | Result |
| --- | --- |
| Automated tests | All 10 Rust tests and 3 frontend tests passed. The Rust suite covers sensor choice, hotspot handling, and integrated GPU fallback when a discrete adapter is sleeping. |
| Windows desktop | The packaged application opened and all 19 routes rendered. Settings, pause/resume, history, and the CPU benchmark were exercised. |
| Sensor data flow | A local test provider supplied CPU package temperature and power, integrated GPU temperature, hotspot, fan speed, and NVMe temperature. The dashboard received the readings and removed them after the provider was disabled. |
| Live NVIDIA GPU | The packaged application detected an active NVIDIA laptop adapter and read a 45 °C core temperature through NVML. The AMD adapter was also detected and had no live temperature source in this session. |
| Setup and portable EXEs | The setup embedded the expected MSI. The MSI application payload matched the portable EXE and passed its native self-check. The extracted sensor helper matched the staged build and resolved the bundled library. |
| Windows Installer | The test environment returned Windows Installer error 1601. WiX installer-service validation was skipped after that failure. A normal install and the UAC flow remain unverified. |

The test machine lacked PawnIO. The installed-helper flow and live CPU package/core temperature and package power still need validation with the signed driver on a supported laptop. This Windows session also denied native drive health and Wi-Fi details, and no physical discrete GPU sleep/wake transition was verified. The disabled-provider test verifies that the app does not retain the simulated readings.
