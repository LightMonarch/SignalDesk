import {
  ThermalAccess,
  ThermalStrip,
  gpuTemperatureText,
} from "./components/thermals";
import { useEffect, useState } from "react";
import {
  Activity,
  ArrowDown,
  ArrowDownLeft,
  ArrowUp,
  ArrowUpRight,
  BatteryCharging,
  Bell,
  Check,
  ChevronRight,
  Cpu,
  Database,
  Fan,
  Gauge,
  HeartPulse,
  LayoutGrid,
  MemoryStick,
  Monitor,
  ShieldCheck,
  Wifi,
} from "lucide-react";
import {
  api,
  bytes,
  date,
  duration,
  errorText,
  num,
  pct,
  str,
  temp,
  type Config,
  type Issue,
  type Sample,
} from "./api";
import {
  Badge,
  Button,
  Dialog,
  Empty,
  Notice,
  PageTitle,
  Panel,
  Stat,
} from "./components/ui";
import { MetricChart } from "./chart";
const widgets = {
  health: "System health",
  cpu: "Processor",
  gpu: "Graphics",
  memory: "Memory",
  storage: "Storage",
  battery: "Battery",
  network: "Network",
  chart: "Performance timeline",
  sensors: "Hardware status",
  alerts: "Recent alerts",
};
const order = [
  "health",
  "cpu",
  "gpu",
  "memory",
  "storage",
  "battery",
  "network",
  "chart",
  "sensors",
  "alerts",
];
const full = ["health", "chart"];
export function Overview({
  sample: s,
  config: c,
  save,
  go,
  paused,
}: {
  sample: Sample | null;
  config: Config;
  save: (c: Config) => Promise<unknown>;
  go: (p: string) => void;
  paused: boolean;
}) {
  const [custom, setCustom] = useState(false);
  const [score, setScore] = useState(false);
  const [draft, setDraft] = useState(c);
  const [error, setError] = useState("");
  const active = [
    ...new Set([...c.widgetOrder.filter((k) => k in widgets), ...order]),
  ];
  const m = s?.metrics ?? {};
  const severity = s?.health.label ?? "Unavailable";
  const healthTone =
    severity === "Poor" || severity === "Critical"
      ? "bad"
      : severity === "Fair"
        ? "warn"
        : "ok";
  const fan = s?.metrics.fan;
  const cards: Record<string, React.ReactNode> = {
    health: (
      <section className="health-hero">
        <div className="hero-copy">
          <div className="hero-label">
            <HeartPulse size={16} /> SYSTEM HEALTH
          </div>
          <h2>
            {severity === "Unavailable"
              ? "Health estimate pending"
              : severity + " condition"}
          </h2>
          <p>
            {severity === "Unavailable"
              ? "SignalDesk is waiting for supported Windows readings."
              : "An estimate based on your available hardware and system readings."}
          </p>
          <div className="tools">
            <Button variant="primary" onClick={() => setScore(true)}>
              <ShieldCheck size={15} /> Score breakdown{" "}
              <ChevronRight size={14} />
            </Button>
            <Button onClick={() => go("Diagnostics")}>
              Diagnostics <ArrowUpRight size={14} />
            </Button>
          </div>
        </div>
        <div
          className={"health-gauge " + healthTone}
          style={
            { "--score": (s?.health.score ?? 0) + "%" } as React.CSSProperties
          }
        >
          <div>
            <strong>{s?.health.score ?? "—"}</strong>
            <span>OUT OF 100</span>
          </div>
        </div>
        <div className="hero-meta">
          <div>
            <span>UPTIME</span>
            <b>{duration(s?.uptime)}</b>
          </div>
          <div>
            <span>PROCESSES</span>
            <b>{num(m.processes)}</b>
          </div>
          <div>
            <span>POWER PLAN</span>
            <b>{str(s?.powerPlan)}</b>
          </div>
        </div>
      </section>
    ),
    cpu: (
      <Stat
        name="CPU load"
        value={pct(m.cpu)}
        detail={
          temp(m.cpuTemp, c.fahrenheit) +
          " · " +
          (s?.cpu.logical
            ? s.cpu.logical + " logical cores"
            : "Core count unavailable")
        }
        icon={Cpu}
      />
    ),
    gpu: (
      <Stat
        name="GPU load"
        value={pct(m.gpu)}
        detail={gpuTemperatureText(s, c) + " GPU temperature"}
        icon={Monitor}
        tone="blue"
      />
    ),
    memory: (
      <Stat
        name="Memory used"
        value={pct(m.memory)}
        detail={bytes(s?.memory.used) + " / " + bytes(s?.memory.total)}
        icon={MemoryStick}
        tone="purple"
      />
    ),
    storage: (
      <Stat
        name="Storage used"
        value={pct(m.disk)}
        detail={
          (s?.disks.length ?? 0) > 0
            ? s!.disks.length +
              (s!.disks.length === 1 ? " volume" : " volumes") +
              " · peak allocation"
            : "Volume details unavailable"
        }
        icon={Database}
        tone="yellow"
      />
    ),
    battery: (
      <Stat
        name="Battery"
        value={s?.battery?.present === false ? "On AC" : pct(m.battery)}
        detail={
          s?.battery?.present === false
            ? "Desktop power"
            : pct(m.batteryHealth) + " estimated health"
        }
        icon={BatteryCharging}
      />
    ),
    network: (
      <Stat
        name="Download"
        value={bytes(m.rx, true)}
        detail={bytes(m.tx, true) + " upload"}
        icon={Wifi}
        tone="blue"
      />
    ),
    chart: (
      <MetricChart
        config={c}
        keys={["cpu", "memory", "gpu"]}
        subtitle="CPU, memory and graphics utilization"
        compact
      />
    ),
    sensors: (
      <Panel
        title="Hardware status"
        icon={Fan}
        action={
          <Button
            variant="ghost"
            onClick={() => go("Cooling and Sensors")}
            aria-label="Open cooling and sensors"
          >
            <ChevronRight size={17} />
          </Button>
        }
      >
        <div className="status-list">
          {[
            ["CPU temperature", temp(m.cpuTemp, c.fahrenheit)],
            ["GPU temperature", gpuTemperatureText(s, c)],
            [
              "Fastest fan",
              typeof fan === "number" ? num(fan) + " RPM" : "Unavailable",
            ],
            [
              "Network link",
              s?.connected == null
                ? "Unavailable"
                : s.connected
                  ? "Connected"
                  : "Disconnected",
            ],
          ].map(([k, v]) => (
            <div key={k}>
              <span>{k}</span>
              <b>{v}</b>
            </div>
          ))}
        </div>
      </Panel>
    ),
    alerts: <Recent go={go} ts={s?.ts} />,
  };
  return (
    <>
      <PageTitle
        title="System overview"
        description="Live hardware telemetry and the condition of your PC."
        actions={
          <>
            <Badge tone={paused ? "warn" : "ok"}>
              {paused
                ? "Monitoring paused"
                : s
                  ? "Monitoring live"
                  : "Connecting"}
            </Badge>
            <Button
              onClick={() => {
                setDraft({ ...c, widgetOrder: active });
                setCustom(true);
                setError("");
              }}
            >
              <LayoutGrid size={16} /> Customize
            </Button>
          </>
        }
      />
      <ThermalStrip
        sample={s}
        config={c}
        open={() => go("Cooling and Sensors")}
      />
      <ThermalAccess sample={s} startup />
      <div className="overview-grid">
        {active
          .filter((k) => !c.hiddenWidgets.includes(k))
          .map((k) => (
            <div
              key={k}
              className={
                "widget " +
                (full.includes(k)
                  ? "full"
                  : c.wideWidgets.includes(k)
                    ? "wide"
                    : k === "sensors" || k === "alerts"
                      ? "half"
                      : "")
              }
            >
              {cards[k]}
            </div>
          ))}
        {active.every((k) => c.hiddenWidgets.includes(k)) && (
          <Empty
            text="All widgets are hidden"
            detail="Choose Customize to show widgets again."
          />
        )}
      </div>
      {score && (
        <Dialog title="Estimated health score" close={() => setScore(false)}>
          <p className="confirm-copy">
            Each available component contributes its score multiplied by its
            effective weight. Weights are redistributed across components with
            readings. CPU and GPU components use supported thermal readings.
            Memory and storage reflect headroom and reported drive health.
            Battery and network use reported capacity and link state.
          </p>
          <div className="status-list">
            {s?.health.parts.map((p) => (
              <div key={p.name}>
                <span>{p.name}</span>
                <b>
                  {p.score == null ? "Unavailable" : p.score + " / 100"} ·{" "}
                  {p.weight.toFixed(1)}% weight
                </b>
              </div>
            ))}
          </div>
        </Dialog>
      )}
      {custom && (
        <Dialog title="Customize overview" close={() => setCustom(false)}>
          <div className="widget-edit">
            {draft.widgetOrder.map((k, i) => (
              <div key={k}>
                <label>
                  <input
                    type="checkbox"
                    checked={!draft.hiddenWidgets.includes(k)}
                    onChange={(e) =>
                      setDraft({
                        ...draft,
                        hiddenWidgets: e.target.checked
                          ? draft.hiddenWidgets.filter((v) => v !== k)
                          : [...draft.hiddenWidgets, k],
                      })
                    }
                  />
                  {widgets[k as keyof typeof widgets]}
                </label>
                {!full.includes(k) && (
                  <label className="muted">
                    <input
                      type="checkbox"
                      checked={draft.wideWidgets.includes(k)}
                      onChange={(e) =>
                        setDraft({
                          ...draft,
                          wideWidgets: e.target.checked
                            ? [...draft.wideWidgets, k]
                            : draft.wideWidgets.filter((v) => v !== k),
                        })
                      }
                    />
                    Wide
                  </label>
                )}
                <Button
                  aria-label={"Move " + k + " up"}
                  disabled={i === 0}
                  onClick={() => move(i, -1)}
                >
                  <ArrowUp size={15} />
                </Button>
                <Button
                  aria-label={"Move " + k + " down"}
                  disabled={i === draft.widgetOrder.length - 1}
                  onClick={() => move(i, 1)}
                >
                  <ArrowDown size={15} />
                </Button>
              </div>
            ))}
          </div>
          {error && <Notice bad>{error}</Notice>}
          <div className="dialog-actions">
            <Button onClick={() => setCustom(false)}>Cancel</Button>
            <Button
              variant="primary"
              onClick={() =>
                void save(draft)
                  .then(() => setCustom(false))
                  .catch((e) => setError(errorText(e)))
              }
            >
              <Check size={16} /> Save layout
            </Button>
          </div>
        </Dialog>
      )}
    </>
  );
  function move(i: number, offset: number) {
    const next = [...draft.widgetOrder];
    [next[i], next[i + offset]] = [next[i + offset], next[i]];
    setDraft({ ...draft, widgetOrder: next });
  }
}
function Recent({ go, ts }: { go: (p: string) => void; ts?: number }) {
  const [items, setItems] = useState<Issue[]>([]);
  useEffect(() => {
    let live = true;
    if (ts)
      void api<Issue[]>("records", { kind: "alert" })
        .then((r) => {
          if (live) setItems(r.slice(0, 3));
        })
        .catch(() => {});
    return () => {
      live = false;
    };
  }, [Math.floor((ts ?? 0) / 10)]);
  return (
    <Panel
      title="Recent alerts"
      icon={Bell}
      action={
        <Button variant="ghost" onClick={() => go("Alerts")}>
          View all <ChevronRight size={15} />
        </Button>
      }
    >
      {items.length ? (
        <div className="status-list">
          {items.map((i) => (
            <div key={i.id + i.ts}>
              <span>
                {i.title}
                <small>{date(i.ts)}</small>
              </span>
              <Badge tone={i.severity === "Critical" ? "bad" : "warn"}>
                {i.severity}
              </Badge>
            </div>
          ))}
        </div>
      ) : (
        <div className="quiet">
          <Activity size={17} /> No alerts recorded.
        </div>
      )}
    </Panel>
  );
}
