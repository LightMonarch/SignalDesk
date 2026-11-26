# SignalDesk 1.0.0 verification

Tested on Windows 11 x64, build 26300, on 26 November 2026.

## Checks passed

- Built the production React and Tauri application from the lockfiles.
- Passed three Rust tests for score boundaries, weight redistribution and SQLite aggregation.
- Passed three frontend report tests covering JSON, CSV, HTML and PDF output at the file-save boundary.
- Launched the native desktop app, loaded all 19 pages and retrieved live CPU, memory, volume, battery API and process readings.
- Exercised desktop IPC for settings, widget order, monitoring pause and resume, SQLite history, diagnostics and the CPU benchmark.
- Closed the dashboard to the tray and confirmed the native process continued to write SQLite samples.
- Verified that the setup EXE contains the released MSI byte for byte, and that the MSI's extracted executable matches the portable release. Ran the native telemetry and SQLite self-check on the extracted executable.

## Resource measurements

CPU is normalized by the logical processor count. Working set is the sum of the measured native process and WebView2 working sets, which can count shared pages more than once. Each row is a short sample of about nine seconds. Transient Windows provider processes are outside the working-set total.

| Mode | CPU | Mean working set | Native process |
| --- | ---: | ---: | ---: |
| Active dashboard | 0.70% | 436.1 MiB | 31.8 MiB |
| Paused dashboard | 1.04% | 443.3 MiB | 31.8 MiB |
| Background launch | 0.06% | 19.1 MiB | 19.1 MiB |

Background launch met the 100 MB working-set target. The active CPU result was below 2%. The open dashboard exceeded the 200 MB working-set target, and the short paused run exceeded the 0.5% idle CPU target. The fresh background SQLite database and WAL occupied about 119 KB after the measurement. The setup EXE and MSI are about 5.7 MB and 5.4 MB, respectively.

## Verification limits

- Windows Installer returned error 1601 in this environment. WiX linked the MSI with service-based validation skipped and successfully extracted the application payload. A full install and uninstall remain unverified. The EXE is an unsigned bootstrap for that MSI. The unsigned portable app was exercised directly.
- This session denied access to the Windows event log and some CIM resources. Windows 10, each GPU and storage configuration, notifications, Ollama responses and native Save dialog interaction were not tested on the available host.
- The app exposes storage reliability counters when Windows provides them. Full vendor SMART tables, per-application network counters and startup impact are outside this build's current coverage. Changes to startup entries are supported for current-user Run entries. System-wide and startup-folder entries are shown for inspection.
- Use the [readme](../README.md) for installation, development, build and release instructions, including optional sensor providers.
