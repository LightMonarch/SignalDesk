import { useId, useMemo, useState } from "react";
import {
  Area,
  CartesianGrid,
  ComposedChart,
  Line,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { Pause, Play, ZoomIn, ZoomOut } from "lucide-react";
import {
  bytes,
  date,
  num,
  pct,
  qty,
  temp,
  tsNow,
  type Config,
  type History,
  type Metric,
} from "./api";
import { useHistory } from "./hooks";
import { Button, Empty, Notice, Panel, Select } from "./components/ui";
export const metricNames: Record<Metric, string> = {
  cpu: "CPU load",
  gpu: "GPU load",
  memory: "Memory",
  disk: "Storage used",
  cpuTemp: "CPU temperature",
  cpuPower: "CPU package power",
  gpuTemp: "GPU core temperature",
  gpuHotspot: "GPU hotspot",
  battery: "Battery charge",
  batteryHealth: "Battery health",
  rx: "Download",
  tx: "Upload",
  score: "Health score",
  processes: "Processes",
  fan: "Fan speed",
  driveTemp: "Drive temperature",
  diskRead: "Disk read",
  diskWrite: "Disk write",
};
const colors = [
  "var(--accent)",
  "var(--blue)",
  "var(--purple)",
  "var(--yellow)",
];
const tempKeys = new Set<Metric>([
  "cpuTemp",
  "gpuTemp",
  "gpuHotspot",
  "driveTemp",
]);
const rateKeys = new Set<Metric>(["rx", "tx", "diskRead", "diskWrite"]);
const percentKeys = new Set<Metric>([
  "cpu",
  "gpu",
  "memory",
  "disk",
  "battery",
  "batteryHealth",
]);
const convert = (v: number, k: Metric, c: Config) =>
  tempKeys.has(k) && c.fahrenheit ? (v * 9) / 5 + 32 : v;
const fmt = (v: unknown, k: Metric, c: Config) =>
  tempKeys.has(k)
    ? temp(v, c.fahrenheit)
    : rateKeys.has(k)
      ? bytes(v, true)
      : percentKeys.has(k)
        ? pct(v)
        : k === "fan"
          ? qty(v, "RPM")
          : k === "cpuPower"
            ? qty(v, "W", 1)
            : num(v, 1);
const axis = (v: number, k: Metric, c: Config) =>
  rateKeys.has(k)
    ? bytes(v, true)
    : tempKeys.has(k)
      ? num(v) + (c.fahrenheit ? "°F" : "°C")
      : percentKeys.has(k)
        ? v + "%"
        : k === "cpuPower"
          ? qty(v, "W")
          : k === "fan"
            ? qty(v, "RPM")
            : num(v);
const ranges = [
  [-1, "Live"],
  [900, "15 min"],
  [3600, "1 hour"],
  [86400, "24 hours"],
  [604800, "7 days"],
  [2592000, "30 days"],
  [0, "Custom"],
] as const;
const localInput = (ts: number) =>
  new Date(ts * 1000 - new Date(ts * 1000).getTimezoneOffset() * 60000)
    .toISOString()
    .slice(0, 16);
export function MetricChart({
  config,
  keys,
  title = "Performance timeline",
  subtitle = "Local telemetry",
  compact = false,
  initial = -1,
}: {
  config: Config;
  keys: Metric[];
  title?: string;
  subtitle?: string;
  compact?: boolean;
  initial?: number;
}) {
  const [range, setRange] = useState(initial);
  const [paused, setPaused] = useState(false);
  const [zoom, setZoom] = useState(1);
  const [start, setStart] = useState(() => localInput(tsNow() - 86400));
  const [end, setEnd] = useState(() => localInput(tsNow()));
  const hrs = useHistory(
    range === -1 ? 300 : range,
    range === -1 ? config.chartRefresh : Math.max(10, config.chartRefresh),
    paused,
    Date.parse(start) / 1000,
    Date.parse(end) / 1000,
  );
  const data = useMemo(
    () =>
      chartData(
        hrs.value,
        keys,
        config,
        zoom,
        [300, 900].includes(range === -1 ? 300 : range)
          ? config.interval * 3
          : undefined,
      ),
    [hrs.value, keys, config, zoom, range],
  );
  const id = useId().replace(/:/g, "");
  const metrics = keys.filter((k) =>
    data.some((p) => p[k] !== null && p[k] !== undefined),
  );
  const stats = keys.map((k) => {
    const v = hrs.value.flatMap((p) => (p.values[k] ? [p.values[k]!] : []));
    return {
      k,
      min: v.length ? Math.min(...v.map((m) => m.min)) : null,
      max: v.length ? Math.max(...v.map((m) => m.max)) : null,
      avg: v.length
        ? v.reduce((a, m) => a + m.avg * m.n, 0) /
          v.reduce((a, m) => a + m.n, 0)
        : null,
      trend: v.length > 1 ? v[v.length - 1].avg - v[0].avg : null,
    };
  });
  const main = keys[0];
  const homogeneous =
    keys.every((k) => percentKeys.has(k) || k === "score") ||
    keys.every((k) => tempKeys.has(k)) ||
    keys.every((k) => rateKeys.has(k));
  return (
    <Panel
      title={title}
      subtitle={subtitle}
      className="chart-panel"
      action={
        <div className="tools">
          <Select
            label="Chart time range"
            value={String(range)}
            onChange={(v) => {
              setRange(Number(v));
              setZoom(1);
            }}
            options={ranges.map(([value, label]) => ({
              value: String(value),
              label,
            }))}
          />
          <Button
            aria-label={paused ? "Resume chart" : "Pause chart"}
            onClick={() => setPaused(!paused)}
          >
            {paused ? <Play size={15} /> : <Pause size={15} />}
          </Button>
          {!compact && (
            <>
              <Button
                aria-label="Zoom in"
                disabled={zoom >= 8}
                onClick={() => setZoom(zoom * 2)}
              >
                <ZoomIn size={15} />
              </Button>
              <Button
                aria-label="Zoom out"
                disabled={zoom <= 1}
                onClick={() => setZoom(zoom / 2)}
              >
                <ZoomOut size={15} />
              </Button>
            </>
          )}
        </div>
      }
    >
      {range === 0 && (
        <div className="tools range-inputs">
          <label>
            From{" "}
            <input
              type="datetime-local"
              value={start}
              onChange={(e) => setStart(e.target.value)}
            />
          </label>
          <label>
            To{" "}
            <input
              type="datetime-local"
              value={end}
              onChange={(e) => setEnd(e.target.value)}
            />
          </label>
        </div>
      )}
      {hrs.error && <Notice bad>{hrs.error}</Notice>}
      <div className={"chart " + (compact ? "compact" : "")} aria-label={title}>
        {metrics.length ? (
          <ResponsiveContainer width="100%" height="100%" minWidth={0}>
            <ComposedChart
              data={data}
              margin={{ left: 0, right: 14, top: 8, bottom: 1 }}
            >
              <defs>
                {keys.map((k, i) => (
                  <linearGradient
                    key={k}
                    id={id + k}
                    x1="0"
                    y1="0"
                    x2="0"
                    y2="1"
                  >
                    <stop
                      offset="0%"
                      stopColor={colors[i % 4]}
                      stopOpacity={0.25}
                    />
                    <stop
                      offset="100%"
                      stopColor={colors[i % 4]}
                      stopOpacity={0.015}
                    />
                  </linearGradient>
                ))}
              </defs>
              <CartesianGrid
                vertical={false}
                stroke="var(--line)"
                strokeDasharray="4 4"
              />
              <XAxis
                dataKey="ts"
                type="number"
                domain={["dataMin", "dataMax"]}
                tickFormatter={(n) =>
                  new Date(n * 1000).toLocaleString(
                    undefined,
                    range >= 86400
                      ? { month: "short", day: "numeric" }
                      : {
                          hour: "2-digit",
                          minute: "2-digit",
                          ...(data.length &&
                          Number(data.at(-1)?.ts) - Number(data[0]?.ts) < 180
                            ? { second: "2-digit" }
                            : {}),
                        },
                  )
                }
                tick={{ fill: "var(--muted)", fontSize: 10 }}
                tickMargin={9}
                minTickGap={35}
                axisLine={false}
                tickLine={false}
              />
              <YAxis
                width={58}
                domain={
                  keys.every((k) => percentKeys.has(k) || k === "score")
                    ? [0, 100]
                    : ["auto", "auto"]
                }
                tickFormatter={(n) => axis(n, main, config)}
                tick={{ fill: "var(--muted)", fontSize: 10 }}
                tickLine={false}
                axisLine={false}
              />
              <Tooltip
                content={<ChartTooltip metricKeys={keys} config={config} />}
              />
              {!compact &&
                homogeneous &&
                [stats[0].min, stats[0].max].map((n, i) =>
                  n === null ? null : (
                    <ReferenceLine
                      key={i}
                      y={convert(n, main, config)}
                      stroke="var(--muted)"
                      strokeDasharray="3 5"
                      strokeOpacity={0.55}
                    />
                  ),
                )}
              {keys.map((k, i) =>
                i === 0 ? (
                  <Area
                    key={k}
                    type="monotone"
                    dataKey={k}
                    stroke={colors[i % 4]}
                    fill={"url(#" + id + k + ")"}
                    strokeWidth={2}
                    isAnimationActive={false}
                    dot={data.length === 1}
                    connectNulls={false}
                  />
                ) : (
                  <Line
                    key={k}
                    type="monotone"
                    dataKey={k}
                    stroke={colors[i % 4]}
                    strokeWidth={2}
                    isAnimationActive={false}
                    dot={data.length === 1}
                    connectNulls={false}
                  />
                ),
              )}
            </ComposedChart>
          </ResponsiveContainer>
        ) : (
          <Empty
            text="Waiting for readings"
            detail="Available samples appear as they are recorded."
          />
        )}
      </div>
      <div className="legend">
        {keys.map((k, i) => (
          <span key={k}>
            <i style={{ background: colors[i % 4] }} />
            {metricNames[k]}
          </span>
        ))}
      </div>
      {!compact && (
        <div className="chart-stats">
          {stats.map((x) => (
            <div key={x.k}>
              <strong>{metricNames[x.k]}</strong>
              <span>Min {fmt(x.min, x.k, config)}</span>
              <span>Avg {fmt(x.avg, x.k, config)}</span>
              <span>Max {fmt(x.max, x.k, config)}</span>
              <span>
                {x.trend == null
                  ? "Trend unavailable"
                  : x.trend === 0
                    ? "Flat over range"
                    : x.trend > 0
                      ? "Ends higher"
                      : "Ends lower"}
              </span>
            </div>
          ))}
        </div>
      )}
    </Panel>
  );
}
function chartData(
  history: History,
  keys: Metric[],
  c: Config,
  zoom: number,
  gap?: number,
) {
  const points = history.slice(-Math.max(2, Math.ceil(history.length / zoom)));
  const data: Record<string, number | null>[] = [];
  for (const p of points) {
    const prev = data.at(-1);
    if (gap && prev && p.ts - Number(prev.ts) > gap)
      data.push({ ts: Number(prev.ts) + 1 });
    const row: Record<string, number | null> = { ts: p.ts };
    for (const k of keys) {
      const n = p.values[k]?.avg;
      row[k] = n == null ? null : convert(n, k, c);
      if (n != null) row[k + "Raw"] = n;
    }
    data.push(row);
  }
  return data;
}
function ChartTooltip({
  active,
  payload,
  metricKeys,
  config,
}: {
  active?: boolean;
  payload?: readonly { payload?: Record<string, number | null> }[];
  metricKeys: Metric[];
  config: Config;
}) {
  const p = payload?.[0]?.payload;
  if (!active || !p) return null;
  return (
    <div className="chart-tooltip">
      <strong>{date(Number(p.ts))}</strong>
      {metricKeys.map((k) => (
        <div key={k}>
          <span>{metricNames[k]}</span>
          <b>{fmt(p[k + "Raw"], k, config)}</b>
        </div>
      ))}
    </div>
  );
}
