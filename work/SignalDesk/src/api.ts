import { invoke, isTauri } from "@tauri-apps/api/core";
export type Metric =
  | "cpu"
  | "gpu"
  | "memory"
  | "disk"
  | "cpuTemp"
  | "gpuTemp"
  | "battery"
  | "batteryHealth"
  | "rx"
  | "tx"
  | "score"
  | "processes"
  | "fan"
  | "driveTemp"
  | "diskRead"
  | "diskWrite";
export type Raw = Record<string, unknown>;
export type Metrics = Partial<Record<Metric, number | null>>;
export type Issue = {
  id: string;
  component: string;
  severity: string;
  title: string;
  evidence: string;
  cause: string;
  action: string;
  ts: number;
  status?: string;
};
export type Process = {
  pid: number;
  name: string;
  started: number;
  cpu: number | null;
  gpu?: number | null;
  memory: number;
  disk: number | null;
  path: string | null;
  status: string;
};
export type Disk = {
  name: string;
  mount: string;
  fileSystem: string;
  kind: string;
  total: number;
  free: number;
  usedPct: number;
  read: number | null;
  write: number | null;
  active: number | null;
};
export type Adapter = {
  name: string;
  rx: number | null;
  tx: number | null;
  rxTotal: number;
  txTotal: number;
  ips: string[];
};
export type Sample = {
  ts: number;
  host: string | null;
  os: string | null;
  boot: number;
  uptime: number;
  metrics: Metrics;
  health: {
    score: number | null;
    label: string;
    parts: { name: string; score: number | null; weight: number }[];
  };
  cpu: {
    name: string | null;
    vendor: string;
    architecture: string;
    physical: number | null;
    logical: number;
    power: number | null;
    cores: { name: string; usage: number | null; clock: number | null }[];
  };
  memory: {
    total: number;
    used: number;
    available: number;
    cache?: number;
    committed?: number;
    commitLimit?: number;
    swapUsed: number;
    swapTotal: number;
  };
  battery: {
    present: boolean | null;
    pct: number | null;
    ac: boolean | null;
    charging: boolean | null;
    remaining: number | null;
    health?: number;
    packs?: Raw[];
  } | null;
  disks: Disk[];
  adapters: Adapter[];
  connected: boolean | null;
  providers: Record<string, Raw[] | undefined> | null;
  providerError: string | null;
  issues: Issue[];
  powerPlan?: string | null;
  inventory?: Raw;
};
export type State = {
  sample: Sample | null;
  paused: boolean;
  error: string | null;
  settingsError: string | null;
};
export type Config = {
  interval: number;
  retention: number;
  cooldown: number;
  duration: number;
  notifications: boolean;
  tray: boolean;
  startup: boolean;
  lowResource: boolean;
  theme: "dark" | "light" | "system";
  accent: "mint" | "blue" | "violet";
  fahrenheit: boolean;
  weights: number[];
  cpuLimit: number;
  memoryLimit: number;
  cpuTemp: number;
  gpuTemp: number;
  diskFree: number;
  batteryHealth: number;
  ai: boolean;
  model: string;
  databaseDir: string;
  benchmarkLimit: number;
  benchSeconds: number;
  connectivityTarget: string;
  offline: boolean;
  widgetOrder: string[];
  hiddenWidgets: string[];
  wideWidgets: string[];
  chartRefresh: number;
};
export type History = {
  ts: number;
  values: Partial<
    Record<Metric, { avg: number; min: number; max: number; n: number }>
  >;
}[];
export type Bench = {
  kind: string;
  value: number;
  duration: number;
  unit: string;
  ts: number;
  read?: number;
  note?: string;
  loss?: number;
};
export type Startup = {
  name: string;
  path: string;
  scope: string;
  kind: string;
  enabled: boolean | null;
  editable: boolean;
};
export type Event = {
  ts: number;
  Id: number;
  RecordId: number;
  ProviderName: string;
  LogName: string;
  LevelDisplayName: string;
  Message: string;
};
export const defaults: Config = {
  interval: 2,
  retention: 30,
  cooldown: 30,
  duration: 12,
  notifications: false,
  tray: true,
  startup: false,
  lowResource: false,
  theme: "dark",
  accent: "mint",
  fahrenheit: false,
  weights: [20, 15, 15, 20, 15, 10, 5],
  cpuLimit: 90,
  memoryLimit: 90,
  cpuTemp: 85,
  gpuTemp: 85,
  diskFree: 10,
  batteryHealth: 70,
  ai: false,
  model: "llama3.2",
  databaseDir: "",
  benchmarkLimit: 90,
  benchSeconds: 3,
  connectivityTarget: "",
  offline: true,
  widgetOrder: [],
  hiddenWidgets: [],
  wideWidgets: [],
  chartRefresh: 2,
};
export const api = <T>(cmd: string, args?: Record<string, unknown>) =>
  isTauri()
    ? invoke<T>(cmd, args)
    : Promise.reject(
        new Error("Open the SignalDesk desktop application to access Windows."),
      );
export const errorText = (e: unknown) =>
  e instanceof Error ? e.message : String(e);
export const tsNow = () => Math.floor(Date.now() / 1000);
export const date = (ts: number) => new Date(ts * 1000).toLocaleString();
export const num = (v: unknown, d = 0) =>
  typeof v === "number" && Number.isFinite(v)
    ? v.toLocaleString(undefined, { maximumFractionDigits: d })
    : "Unavailable";
export const qty = (v: unknown, unit: string, d = 0) =>
  typeof v === "number" && Number.isFinite(v)
    ? num(v, d) + (unit === "%" ? "" : " ") + unit
    : "Unavailable";
export const pct = (v: unknown) => qty(v, "%");
export function bytes(v: unknown, rate = false) {
  if (typeof v !== "number" || !Number.isFinite(v) || v < 0)
    return "Unavailable";
  const units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
  const i =
    v > 0
      ? Math.max(
          0,
          Math.min(units.length - 1, Math.floor(Math.log(v) / Math.log(1024))),
        )
      : 0;
  return qty(v / 1024 ** i, units[i] + (rate ? "/s" : ""), i ? 1 : 0);
}
export function temp(v: unknown, f: boolean) {
  return qty(
    typeof v === "number" && f ? (v * 9) / 5 + 32 : v,
    f ? "°F" : "°C",
    1,
  );
}
export const duration = (s: unknown) =>
  typeof s === "number" && s >= 0
    ? [
        s >= 86400 ? Math.floor(s / 86400) + "d" : "",
        s >= 3600 ? (Math.floor(s / 3600) % 24) + "h" : "",
        (Math.floor(s / 60) % 60) + "m",
      ]
        .filter(Boolean)
        .join(" ")
    : "Unavailable";
export const str = (v: unknown): string =>
  v === null || v === undefined || v === ""
    ? "Unavailable"
    : typeof v === "boolean"
      ? v
        ? "Yes"
        : "No"
      : Array.isArray(v)
        ? v.length
          ? v.map(str).join(", ")
          : "Unavailable"
        : typeof v === "object"
          ? JSON.stringify(v)
          : String(v).trim() || "Unavailable";
export const object = (v: unknown): Raw =>
  v && typeof v === "object" && !Array.isArray(v) ? (v as Raw) : {};
export const rows = (v: unknown): Raw[] =>
  Array.isArray(v) ? v.map(object) : [];
export const command = (cmd: string) => (text: string) =>
  api<boolean>("save_file", {
    filename: cmd,
    bytes: Array.from(new TextEncoder().encode(text)),
  });
