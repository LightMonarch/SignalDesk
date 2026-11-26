# SignalDesk 1.0.1 sensor verification

Tested on Windows 11 x64 build 26300. SignalDesk reports each value only when a Windows or hardware provider returns it.

| Reading | Collection path | Result on this test host |
| --- | --- | --- |
| CPU package temperature and package power | LibreHardwareMonitor WMI or the optional loopback connector | WMI access was denied. Intel and AMD sensor selection and loopback formatting were tested with fixtures. |
| Drive health, wear, temperature, power-on hours and power cycles | Read-only NVMe SMART and Windows storage queries | Physical-drive access was denied in the test session. NVMe result decoding and counter semantics passed unit tests. |
| Start/stop cycles and individual read/write errors | ATA SMART where supported and Windows storage reliability counters | Not returned in this session. NVMe combined media errors and power cycles are labeled separately. |
| Battery capacity health and estimated wear | Windows battery driver, WinRT battery report and WMI fallback | Windows returned capacity health on the actual laptop. Estimated wear is the complement of capacity health. |
| Battery runtime remaining | Windows time estimate or a calculation from remaining energy and discharge rate | The laptop was on AC. Discharging, multiple-pack and relative-capacity handling passed unit tests. |
| Wi-Fi link details | Native WLAN interface and connection quality queries | Windows returned access denied. The app shows the provider message and Windows location guidance. |
| Fastest fan | Highest observed RPM exposed by LibreHardwareMonitor | The live provider was unavailable. A fixture exercised RPM and a valid zero-RPM reading. |

The installer bundle was extracted and the embedded application matched the portable executable byte for byte. The executable passed its native telemetry and SQLite smoke check. Windows Installer returned error 1601 in the test environment, so full install and uninstall remain unverified.

All eight Rust tests and three frontend tests passed. The desktop smoke test opened all 19 pages and checked live Windows samples, SQLite history, monitoring controls, diagnostics and a CPU benchmark. A loopback fixture verified that temperature, power, fan and drive sensor values reach the monitoring API with provider attribution. Disabling the connector removed its readings from subsequent samples.

The optional local LibreHardwareMonitor connector is disabled by default. See the [setup instructions](../README.md#monitoring-and-compatibility) to configure it. For a local availability report, run the portable executable with `--sensor-check` and an absolute destination for a JSON report.
