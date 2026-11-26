# Development and verification roadmap

| Phase | Implemented work | Verification |
| --- | --- | --- |
| 1 | Git, Tauri, Rust and frontend dependencies | Inspect toolchain and lockfiles |
| 2 | Dashboard, navigation, themes and persisted widget layout | TypeScript build and desktop smoke test |
| 3–4 | Windows counters, DXGI, hardware inventory and optional sensor providers | Native sample check and hardware availability review |
| 5 | Process telemetry and SQLite history | Aggregate tests and live history checks |
| 6 | Health scoring, diagnostics and notifications | Score tests and rule-state checks |
| 7 | Event logs, startup and benchmarks | Provider smoke tests and focused Windows checks |
| 8 | Reports, tray and settings | Runtime UI smoke test and export review |
| 9 | Local Ollama integration | Connection and unavailable-state review |
| 10–11 | Release builds, host resource measurements and Windows setup | Installer and portable binary checks |

See the verification notes distributed with the release for the results from this development machine.
