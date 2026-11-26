import "./thermals.css";
import { useState } from "react";
import {
  ArrowUpRight,
  Cpu,
  Monitor,
  ShieldCheck,
  Thermometer,
  X,
} from "lucide-react";
import {
  api,
  errorText,
  qty,
  str,
  temp,
  type Config,
  type Raw,
  type Sample,
} from "../api";
import { Badge, Button, Notice } from "./ui";

export function temperatureSource(reading?: Raw | null) {
  if (!reading) return null;
  return [reading.name, reading.source]
    .filter((v) => typeof v === "string" && v.trim())
    .join(" · ");
}

export function gpuTemperatureText(sample: Sample | null, config: Config) {
  const value = sample?.metrics.gpuTemp;
  if (typeof value === "number") return temp(value, config.fahrenheit);
  const adapters = sample?.providers?.graphicsPower ?? [];
  if (adapters.length && adapters.every((row) => row.state === "Sleeping"))
    return "Sleeping";
  return "Unavailable";
}

export function ThermalAccess({
  sample,
  startup = false,
}: {
  sample: Sample | null;
  startup?: boolean;
}) {
  const [hidden, setHidden] = useState(
    () =>
      startup && sessionStorage.getItem("thermal-prompt-dismissed") === "yes",
  );
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const access = sample?.thermalAccess;
  if (
    !access ||
    hidden ||
    (startup && typeof sample?.metrics.cpuTemp === "number")
  )
    return null;
  const state = access.state;
  const label =
    state === "needsAdmin"
      ? "Enable CPU hardware sensors"
      : state === "driverMissing"
        ? "PawnIO driver needed"
        : state === "starting" || pending
          ? "Connecting to the sensor helper"
          : state === "active"
            ? "Sensor helper active"
            : state === "paused"
              ? "Monitoring paused"
              : state === "notInstalled"
                ? "Installed sensor helper unavailable"
                : "Check sensor access";
  const canStart = access.installed && state === "needsAdmin";
  async function enable() {
    setPending(true);
    setError("");
    try {
      await api("enable_thermal");
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setPending(false);
    }
  }
  return (
    <section className="thermal-access" aria-label="Thermal sensor access">
      <span className="thermal-access-icon">
        <ShieldCheck size={21} />
      </span>
      <div className="thermal-access-copy">
        <strong>{label}</strong>
        <p>{access.detail}</p>
        {canStart && (
          <p>Windows will ask for permission to start the sensor helper.</p>
        )}
        {error && <Notice bad>{error}</Notice>}
      </div>
      <div className="tools">
        {canStart && (
          <Button
            variant="primary"
            disabled={pending}
            onClick={() => void enable()}
          >
            {pending ? "Waiting for Windows…" : "Enable sensors"}
          </Button>
        )}
        {state === "active" && <Badge tone="ok">Connected</Badge>}
        {startup && (
          <Button
            aria-label="Dismiss sensor prompt"
            variant="ghost"
            onClick={() => {
              sessionStorage.setItem("thermal-prompt-dismissed", "yes");
              setHidden(true);
            }}
          >
            <X size={17} />
          </Button>
        )}
      </div>
    </section>
  );
}

export function ThermalStrip({
  sample,
  config,
  open,
}: {
  sample: Sample | null;
  config: Config;
  open: () => void;
}) {
  const cpu = sample?.metrics.cpuTemp;
  const gpu = sample?.metrics.gpuTemp;
  const hotspot = sample?.metrics.gpuHotspot;
  const temperature = (value: unknown) => temp(value, config.fahrenheit);
  const cards = [
    {
      name: "CPU package",
      icon: Cpu,
      value: temperature(cpu),
      detail:
        temperatureSource(sample?.cpu.temperatureReading) ??
        "Hardware sensor unavailable",
      note:
        typeof sample?.cpu.power === "number"
          ? qty(sample.cpu.power, "W", 1) + " package power"
          : null,
      warning: typeof cpu === "number" && cpu >= config.cpuTemp,
    },
    {
      name: "GPU core",
      icon: Monitor,
      value: gpuTemperatureText(sample, config),
      detail:
        temperatureSource(sample?.gpuReading) ??
        (gpuTemperatureText(sample, config) === "Sleeping"
          ? "Direct polling paused"
          : "Graphics sensor unavailable"),
      note:
        typeof hotspot === "number" ? temperature(hotspot) + " hotspot" : null,
      warning: typeof gpu === "number" && gpu >= config.gpuTemp,
    },
  ];
  return (
    <section className="thermal-strip" aria-label="Live temperatures">
      <div className="thermal-strip-head">
        <h2>
          <Thermometer size={18} /> Live temperatures
        </h2>
        <Button variant="ghost" onClick={open}>
          Sensor details <ArrowUpRight size={15} />
        </Button>
      </div>
      <div className="thermal-readouts">
        {cards.map(({ name, icon: Icon, value, detail, note, warning }) => (
          <article className="thermal-reading" key={name}>
            <span className="thermal-reading-icon">
              <Icon size={20} />
            </span>
            <div>
              <div className="thermal-reading-name">{name}</div>
              <strong className={warning ? "thermal-warning" : undefined}>
                {value}
              </strong>
              <small>{detail}</small>
            </div>
            {note && <small className="thermal-note">{note}</small>}
          </article>
        ))}
      </div>
    </section>
  );
}

export function powerState(row: Raw) {
  return (
    <Badge
      tone={
        row.state === "Active"
          ? "ok"
          : row.state === "Sleeping"
            ? "neutral"
            : "warn"
      }
    >
      {str(row.state)}
    </Badge>
  );
}
