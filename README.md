# SignalDesk

SignalDesk is a Windows 10 and 11 system health dashboard built with Tauri 2, Rust, React, TypeScript, Tailwind CSS, shadcn/ui components and SQLite. It samples real system counters and stores performance history on the PC.

## Install

Open the x64 setup executable or MSI and follow the installer prompts. Open SignalDesk from the Start menu. The separately distributed portable executable runs directly with WebView2 installed. Setup can download the Microsoft Edge WebView2 Runtime on a Windows installation where it is missing. Core monitoring works offline once the runtime is available.

The builds are unsigned unless a Windows signing certificate is supplied to the release build. Settings and monitoring history normally live under `%LOCALAPPDATA%\local.signaldesk.monitor`. Use the tray menu to pause, reopen or exit the application. With tray mode enabled, closing the dashboard releases the WebView while the native monitor continues. Settings controls launch at sign-in and close-to-tray behavior.

## Monitoring and compatibility

| Source | Readings |
| --- | --- |
| Native Windows APIs and sysinfo | CPU usage, memory, processes, volumes, network counters, Wi-Fi connection quality, battery status and capacities |
| DXGI and GDI | Physical graphics adapters, memory capacities, active display resolution and refresh rate |
| Windows CIM and storage providers | Hardware inventory, graphics engine and memory counters, battery capacity, storage reliability and adapter details |
| Windows storage driver and NVMe SMART | Read-only drive health, temperature, wear, lifetime counters and combined media errors where exposed |
| LibreHardwareMonitor WMI or loopback | Optional CPU package temperature, package power, clocks and fan RPM exposed by detected hardware |
| NVIDIA driver | Optional GPU telemetry when the driver supplies the NVIDIA System Management Interface utility |
| Windows Event Log | Recent System and Application warnings and errors |

Start LibreHardwareMonitor for CPU package temperature, package watts and fan RPM. SignalDesk looks for its WMI namespace at `root\LibreHardwareMonitor`. If WMI access is restricted, start LibreHardwareMonitor's web server bound to localhost at port 8085 and enable **Read LibreHardwareMonitor over loopback** in SignalDesk Settings. The optional connector reads `127.0.0.1:8085`. Cooling and Sensors shows which providers responded.

Battery health uses the full-charge and design capacity reported by Windows. Remaining runtime is shown on battery power when Windows gives an estimate or the driver exposes both remaining energy and a positive discharge rate. NVMe power-cycle counts and combined media errors have separate fields from start/stop cycles and read/write error counters. Windows and drive firmware determine which of those fields are available. On newer Windows versions SignalDesk uses the location-independent WLAN quality query where available. Older WLAN drivers can require Windows location access for connection details.

Run SignalDesk as a standard user. Provider access, driver availability and device firmware determine which readings appear. Protected processes, some startup locations, per-app network counters, full SMART tables and certain vendor-specific readings can be unavailable. GPU engine utilization is the busiest reported engine, which can differ from a vendor's utilization calculation.

Health is an operating-condition estimate. CPU and GPU utilization appear as performance readings and sustained alerts. Component scoring uses reported thermal or device state where available, memory and free-space headroom, storage health, battery capacity and link state. A component with no usable reading is excluded. Active weights are normalized, and a known missing battery contributes zero weight. The score is a weighted average rounded to an integer. See the score breakdown in Overview.

History keeps raw samples for up to 15 minutes and maintains weighted minute buckets for the configured retention period. Alerts record evidence after configured sustained thresholds, with a per-rule cooldown. History uses the active local database. Changing the database folder opens a database in that folder and preserves the previous file in its original location.

Connectivity checks start only in response to a user action. Offline-only mode uses the local IPv4 gateway. With offline-only mode disabled, a specifically configured IP can be checked. ICMP results describe reachability from this PC at the time of the test. The GPU benchmark uses an accelerated WebGL workload and reports the selected renderer. Disk benchmarks use a disposable temporary file with synchronized writes and buffered reads.

Optional Ollama queries are sent to the local service at `127.0.0.1:11434`. Install an Ollama model locally before enabling the integration. Event text is included only when selected. The assistant provides explanations in text and has no system-changing tools.

## Develop

Install current Node.js LTS and npm, stable Rust for `x86_64-pc-windows-msvc`, Microsoft C++ Build Tools with a Windows SDK, and WebView2. MSI packaging also needs the Windows VBScript optional feature on Windows versions where it is disabled. Tauri downloads the NSIS and WiX build tools into the Cargo target directory on first use.

```powershell
Set-Location work/SignalDesk
npm ci
npm run tauri dev
```

```powershell
npm run build
npm test
cargo test --release --locked --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --bundles nsis,msi
```

Default release artifacts are under `work/SignalDesk/src-tauri/target/release/bundle`, or under the release directory specified by `CARGO_TARGET_DIR`. The application binary supports `signaldesk.exe --self-check <absolute-path-to-json>` for an isolated native telemetry and SQLite smoke check. `SIGNALDESK_DATA_DIR` points desktop smoke tests to a separate app-data directory.

Use `signaldesk.exe --sensor-check <absolute-path-to-json>` to record the availability of the default Windows providers. The report lists available fields and provider messages. It excludes raw drive identifiers, network names and sampled metric values.

The standalone x64 setup bootstrap can wrap an existing MSI when the NSIS compiler cannot run. It stages the MSI and runs Windows Installer, so both files install the same product. From `work/SignalDesk`, run `./packaging/build-bootstrap.ps1 -Msi <path-to-package.msi> -Output <path-to-setup.exe>`. Build and validate the MSI through Tauri on a host with a working Windows Installer service before public distribution.

### Architecture

- `work/SignalDesk/src` contains the application shell, shared controls, feature pages, typed frontend IPC models and report formats.
- `src-tauri/src/monitor.rs`, `native.rs` and `windows.rs` collect native, DXGI, CIM and hardware provider readings.
- `db.rs` persists telemetry, aggregates and records. `config.rs` validates and persists preferences.
- `health.rs`, `diagnostics.rs`, `benchmark.rs` and `assistant.rs` implement the feature engines.
- `main.rs` wires Tauri commands, worker threads and tray integration.

## Release

Update the version in `package.json`, `package-lock.json`, `src-tauri/Cargo.toml` and `tauri.conf.json`. Refresh the Cargo lockfile and changelog. Run both builds and the smoke tests. To enable the provided GitHub Actions workflow, copy `docs/windows-build.workflow.yml` to `.github/workflows/windows.yml`. Upload the EXE, MSI and SHA-256 sums to a versioned release in the private repository.

[Windows packaging](https://v2.tauri.app/distribute/windows-installer/) · [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) · [Ollama chat API](https://docs.ollama.com/api/chat)

[Host verification and resource measurements](docs/verification.md)
