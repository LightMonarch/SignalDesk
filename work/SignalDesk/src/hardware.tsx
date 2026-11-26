import {
  ThermalAccess,
  temperatureSource,
  gpuTemperatureText,
  powerState,
} from "./components/thermals";
import { DriveHealth } from "./components/drive-health";
import { useEffect, useState } from "react";
import {
  ArrowDownLeft,
  ArrowUpRight,
  BatteryCharging,
  Cpu,
  Database,
  Fan,
  MemoryStick,
  Monitor,
  Network,
  RefreshCw,
  Wifi,
} from "lucide-react";
import {
  api,
  bytes,
  date,
  duration,
  errorText,
  num,
  object,
  pct,
  qty,
  rows,
  str,
  temp,
  type Config,
  type Process,
  type Raw,
  type Sample,
} from "./api";
import { useLoad } from "./hooks";
import {
  Badge,
  Button,
  Empty,
  Notice,
  PageTitle,
  Pairs,
  Panel,
  Stat,
  Table,
} from "./components/ui";
import { MetricChart } from "./chart";
const descriptions: Record<string, string> = {
  "System Information": "Windows, firmware and installed hardware.",
  CPU: "Processor utilization, clocks and thermal readings.",
  GPU: "Graphics adapters, graphics memory and sensor readings.",
  Memory: "Physical memory, commit and page file usage.",
  Storage: "Volumes, throughput and physical drive condition.",
  Battery: "Charge, available power and capacity over time.",
  Network: "Interface activity and authorized connectivity checks.",
  "Cooling and Sensors":
    "Thermal readings, fan speeds and sensor availability.",
};
export function Hardware({
  page,
  sample: s,
  config: c,
}: {
  page: string;
  sample: Sample | null;
  config: Config;
}) {
  return (
    <>
      <PageTitle title={page} description={descriptions[page]} />
      {s ? (
        <HardwareBody key={page} page={page} s={s} c={c} />
      ) : (
        <Panel>
          <Empty loading text="Waiting for the first Windows sample" />
        </Panel>
      )}
    </>
  );
}
function Inventory({ s, page }: { s: Sample; page: string }) {
  const inv = useLoad<Raw>("inventory", {});
  useEffect(() => {
    void inv.load();
  }, [inv.load]);
  const data = Object.keys(inv.value).length ? inv.value : (s.inventory ?? {});
  const computer = object(data.computer),
    os = object(data.windows),
    bios = object(data.bios),
    board = object(data.board),
    tpm = object(data.tpm);
  return (
    <>
      {inv.error && <Notice bad>{inv.error}</Notice>}
      {page === "System Information" && (
        <div className="detail-grid">
          <Panel
            title="Device and Windows"
            action={
              <Button
                disabled={inv.busy}
                onClick={() => void inv.load({ fresh: true })}
              >
                <RefreshCw size={15} /> Refresh
              </Button>
            }
          >
            <Pairs
              data={[
                ["Computer name", str(s.host)],
                ["Manufacturer", str(computer.Manufacturer)],
                ["Model", str(computer.Model)],
                ["Operating system", str(os.Caption ?? s.os)],
                ["Build", str(os.BuildNumber)],
                ["Architecture", s.cpu.architecture],
                ["Installed", str(os.Installed)],
                ["Last boot", s.boot ? date(s.boot) : str(os.LastBoot)],
                ["Uptime", duration(s.uptime)],
                ["Power plan", str(data.powerPlan ?? s.powerPlan)],
              ]}
            />
          </Panel>
          <Panel title="Firmware and hardware">
            <Pairs
              data={[
                ["BIOS", str(bios.SMBIOSBIOSVersion)],
                ["BIOS supplier", str(bios.Manufacturer)],
                ["Motherboard", str(board.Product)],
                ["Board manufacturer", str(board.Manufacturer)],
                ["Secure Boot", str(data.secureBoot)],
                ["TPM version", str(tpm.SpecVersion)],
                ["TPM enabled", str(tpm.IsEnabled_InitialValue)],
                ["Processor", str(s.cpu.name)],
                ["Installed RAM", bytes(s.memory.total)],
                [
                  "Graphics adapters",
                  str(s.providers?.gpus?.map((g) => g.Name)),
                ],
                [
                  "Storage devices",
                  str(s.providers?.physicalDisks?.map((d) => d.Model)),
                ],
                ["Network interfaces", str(s.adapters.map((a) => a.name))],
              ]}
            />
          </Panel>
          <Panel title="Connected displays">
            <RawTable
              data={rows(data.displays)}
              columns={{
                Name: "Display",
                MonitorManufacturer: "Manufacturer",
                ScreenWidth: "Width",
                ScreenHeight: "Height",
                RefreshRate: "Refresh rate",
              }}
            />
          </Panel>
        </div>
      )}
      {page === "CPU" && (
        <Panel title="Processor specification">
          <RawTable
            data={rows(data.cpu)}
            columns={{
              Name: "Model",
              MaxClockSpeed: "Rated max MHz",
              CurrentClockSpeed: "Reported MHz",
              L2CacheSize: "L2 KiB",
              L3CacheSize: "L3 KiB",
            }}
          />
        </Panel>
      )}
      {page === "Memory" && (
        <Panel
          title="Installed modules"
          subtitle={
            rows(data.memory).length +
            " reported modules · " +
            num(data.slots) +
            " total slots"
          }
        >
          <RawTable
            data={rows(data.memory)}
            columns={{
              DeviceLocator: "Slot",
              Manufacturer: "Manufacturer",
              PartNumber: "Part number",
              Capacity: "Capacity",
              ConfiguredClockSpeed: "Configured MT/s",
              SMBIOSMemoryType: "Type",
            }}
            format={{
              Capacity: (v) => bytes(v),
              SMBIOSMemoryType: (v) =>
                ({
                  20: "DDR",
                  21: "DDR2",
                  24: "DDR3",
                  26: "DDR4",
                  30: "LPDDR4",
                  34: "DDR5",
                  35: "LPDDR5",
                })[Number(v)] ?? str(v),
            }}
          />
        </Panel>
      )}
    </>
  );
}
function HardwareBody({ page, s, c }: { page: string; s: Sample; c: Config }) {
  const m = s.metrics,
    p = s.providers ?? {},
    t = (n: unknown) => temp(n, c.fahrenheit);
  if (page === "System Information") return <Inventory page={page} s={s} />;
  if (page === "CPU")
    return (
      <>
        <div className="stat-row">
          <Stat name="Overall utilization" value={pct(m.cpu)} icon={Cpu} />
          <Stat
            name="Package temperature"
            value={t(m.cpuTemp)}
            detail={readingDetail(
              s.cpu.temperatureReading,
              "Enable the sensor helper or attach LibreHardwareMonitor",
            )}
            icon={Fan}
            tone="blue"
          />
          <Stat
            name="Package power"
            value={qty(s.cpu.power, "W", 1)}
            detail={readingDetail(
              s.cpu.powerReading,
              "A hardware provider is required",
            )}
            tone="purple"
          />
          <Stat name="Logical cores" value={num(s.cpu.logical)} tone="yellow" />
        </div>
        <div className="detail-grid">
          <Panel title={str(s.cpu.name)} icon={Cpu}>
            <Pairs
              data={[
                ["Manufacturer", str(s.cpu.vendor)],
                ["Architecture", s.cpu.architecture],
                ["Physical cores", num(s.cpu.physical)],
                ["Logical cores", num(s.cpu.logical)],
                [
                  "Throttle sensor",
                  str(
                    p.sensors?.find((x) => /throttl/i.test(String(x.Name)))
                      ?.Value,
                  ),
                ],
              ]}
            />
          </Panel>
          <Panel title="Per-core utilization">
            <div className="cores">
              {s.cpu.cores.map((x, i) => (
                <div key={i}>
                  <span>Core {i + 1}</span>
                  <meter min={0} max={100} value={x.usage ?? 0} />
                  <b>{pct(x.usage)}</b>
                  <small>{qty(x.clock, "MHz")}</small>
                </div>
              ))}
            </div>
          </Panel>
        </div>
        <ThermalAccess sample={s} />
        <Panel title="CPU temperature sensors" icon={Fan}>
          <Sensors
            data={(p.sensors ?? []).filter((sensor) =>
              sensor.SensorType === "Temperature" &&
              /cpu/i.test(String(sensor.Identifier).split("/")[1] ?? ""),
            )}
            c={c}
          />
        </Panel>
        <Inventory page={page} s={s} />
        <MetricChart config={c} keys={["cpu"]} title="CPU utilization" />
        <MetricChart config={c} keys={["cpuTemp"]} title="CPU temperature" />
        <MetricChart config={c} keys={["cpuPower"]} title="CPU package power" />
      </>
    );
  if (page === "GPU")
    return (
      <>
        <div className="stat-row">
          <Stat name="Peak GPU engine" value={pct(m.gpu)} icon={Monitor} />
          <Stat
            name="GPU core temperature"
            value={gpuTemperatureText(s, c)}
            detail={temperatureSource(s.gpuReading) ?? undefined}
            icon={Fan}
            tone="blue"
          />
          <Stat
            name="GPU hotspot"
            value={t(m.gpuHotspot)}
            detail={temperatureSource(s.gpuHotspotReading) ?? undefined}
            tone="yellow"
          />
          <Stat
            name="Adapters discovered"
            value={
              (p.dxgi ?? p.gpus)?.length
                ? num((p.dxgi ?? p.gpus)?.length)
                : "Unavailable"
            }
            tone="purple"
          />
        </div>
        <div className="detail-grid">
          {(p.dxgi?.length ? p.dxgi : (p.gpus ?? [])).map((g, i) => {
            const driver = p.gpus?.find((d) => d.Name === g.Name) ?? g;
            const match = (row: Raw) =>
              typeof g.luid === "string" &&
              String(row.Name ?? row.adapter)
                .toLowerCase()
                .includes(g.luid.toLowerCase());
            const mem = p.gpuMemory?.filter(match) ?? [];
            const eng = p.gpuEngines?.filter(match) ?? [];
            const peak = (type?: string) => {
              const v = eng
                .filter((r) => !type || String(r.kind).toLowerCase() === type)
                .map((r) => r.utilization)
                .filter((n): n is number => typeof n === "number");
              return v.length ? Math.max(...v) : null;
            };
            const sum = (key: string) => {
              const v = mem
                .map((r) => r[key])
                .filter((n): n is number => typeof n === "number");
              return v.length ? v.reduce((a, b) => a + b, 0) : null;
            };
            return (
              <Panel key={i} title={str(g.Name)} icon={Monitor}>
                <Pairs
                  data={[
                    ["Vendor", str(g.vendor ?? driver.AdapterCompatibility)],
                    ["Driver version", str(driver.DriverVersion)],
                    ["Utilization (peak engine)", pct(peak())],
                    [
                      "Dedicated capacity",
                      bytes(g.dedicated ?? driver.AdapterRAM),
                    ],
                    ["Dedicated used", bytes(sum("DedicatedUsage"))],
                    ["Shared capacity", bytes(g.shared)],
                    ["Shared used", bytes(sum("SharedUsage"))],
                    ["Video encode", pct(peak("videoencode"))],
                    ["Video decode", pct(peak("videodecode"))],
                  ]}
                />
              </Panel>
            );
          })}
        </div>
        <Panel
          title="Graphics memory by Windows adapter LUID"
          subtitle="The Windows identifiers are listed independently from driver inventory."
        >
          <RawTable
            data={p.gpuMemory ?? []}
            columns={{
              Name: "Adapter identifier",
              DedicatedUsage: "Dedicated usage",
              SharedUsage: "Shared usage",
            }}
            format={{
              DedicatedUsage: (v) => bytes(v),
              SharedUsage: (v) => bytes(v),
            }}
          />
        </Panel>
        <Panel
          title="Graphics power and temperature"
          subtitle="Silicon Desk queries NVIDIA only when Windows reports the adapter active. Passive readings can supply integrated GPU temperatures."
        >
          {(p.graphicsPower ?? []).length ? (
            <Table
              headers={[
                "Adapter",
                "Power state",
                "Core temperature",
                "Provider",
              ]}
            >
              {(p.graphicsPower ?? []).map((gpu, i) => (
                <tr key={i}>
                  <td>{str(gpu.name)}</td>
                  <td>{powerState(gpu)}</td>
                  <td>{t(gpu.temperature)}</td>
                  <td>{str(gpu.source ?? gpu.detail)}</td>
                </tr>
              ))}
            </Table>
          ) : (
            <Empty
              text="Power state unavailable"
              detail="See Cooling and Sensors for the Windows provider status."
            />
          )}
        </Panel>
        <Panel title="Graphics sensors by hardware identifier">
          <Sensors
            data={(p.sensors ?? []).filter((x) =>
              String(x.Identifier).startsWith("/gpu"),
            )}
            c={c}
          />
        </Panel>
        <MetricChart config={c} keys={["gpu"]} title="Graphics utilization" />
        <MetricChart
          config={c}
          keys={["gpuTemp", "gpuHotspot"]}
          title="Graphics temperature"
        />
      </>
    );
  if (page === "Memory")
    return (
      <>
        <div className="stat-row">
          <Stat
            name="Memory utilization"
            value={pct(m.memory)}
            icon={MemoryStick}
          />
          <Stat
            name="Installed memory"
            value={bytes(s.memory.total)}
            tone="blue"
          />
          <Stat
            name="Available memory"
            value={bytes(s.memory.available)}
            tone="purple"
          />
          <Stat
            name="Cached memory"
            value={bytes(s.memory.cache)}
            tone="yellow"
          />
        </div>
        <div className="detail-grid">
          <Panel title="Memory accounting">
            <Pairs
              data={[
                ["Physical used", bytes(s.memory.used)],
                ["Committed", bytes(s.memory.committed)],
                ["Commit limit", bytes(s.memory.commitLimit)],
                ["Page file used", bytes(s.memory.swapUsed)],
                ["Page file total", bytes(s.memory.swapTotal)],
              ]}
            />
          </Panel>
          <MemoryProcesses />
        </div>
        <Inventory s={s} page={page} />
        <MetricChart config={c} keys={["memory"]} title="Memory usage" />
      </>
    );
  if (page === "Storage")
    return (
      <>
        <div className="detail-grid">
          {s.disks.map((d) => (
            <Panel
              key={d.mount}
              title={d.mount + (d.name ? " · " + d.name : "")}
              icon={Database}
              action={
                <Badge tone={d.usedPct >= 100 - c.diskFree ? "warn" : "ok"}>
                  {pct(d.usedPct)} used
                </Badge>
              }
            >
              <meter className="volume" min={0} max={100} value={d.usedPct} />
              <Pairs
                data={[
                  ["Capacity", bytes(d.total)],
                  ["Used", bytes(d.total - d.free)],
                  ["Available", bytes(d.free)],
                  ["File system", d.fileSystem],
                  ["Read rate", bytes(d.read, true)],
                  ["Write rate", bytes(d.write, true)],
                  ["Disk activity", pct(d.active)],
                ]}
              />
            </Panel>
          ))}
        </div>
        <Panel title="Physical drives">
          <RawTable
            data={
              p.physicalDisks?.length
                ? p.physicalDisks
                : (p.driveHealth ?? []).map((d) => ({
                    Model: d.name,
                    Manufacturer: d.manufacturer,
                    InterfaceType: d.bus,
                    Status: d.status,
                  }))
            }
            columns={{
              Model: "Model",
              Manufacturer: "Manufacturer",
              Size: "Capacity",
              InterfaceType: "Interface",
              Partitions: "Partitions",
              Status: "Status",
            }}
            format={{ Size: (v) => bytes(Number(v) || null) }}
          />
        </Panel>
        <DriveHealth drives={p.driveHealth ?? []} fahrenheit={c.fahrenheit} />
        <ProviderNotes
          rows={p.probes}
          names={["Drive health"]}
          includeAvailable={false}
        />
        <MetricChart
          config={c}
          keys={["diskRead", "diskWrite"]}
          title="Storage activity"
        />
        <MetricChart
          config={c}
          keys={["driveTemp"]}
          title="Drive temperatures"
        />
      </>
    );
  if (page === "Battery") {
    const b = s.battery;
    return b?.present === false ? (
      <Panel>
        <Empty
          text="No system battery detected"
          detail="Windows reports this computer has no battery. Battery readings are excluded from the health estimate."
        />
      </Panel>
    ) : (
      <>
        <div className="stat-row">
          <Stat
            name="Battery charge"
            value={pct(m.battery)}
            icon={BatteryCharging}
          />
          <Stat
            name="Estimated capacity health"
            value={pct(m.batteryHealth)}
            detail={
              b?.healthSource ?? "Needs both design and full-charge capacity"
            }
            tone="blue"
          />
          <Stat
            name="Power remaining"
            value={b?.ac === true ? "On AC power" : duration(b?.remaining)}
            detail={
              b?.ac === true
                ? "Runtime is estimated while discharging"
                : (b?.remainingSource ?? "No discharge-time estimate reported")
            }
            tone="purple"
          />
        </div>
        <Panel title="Power state">
          <Pairs
            data={[
              [
                "AC adapter",
                b?.ac == null
                  ? "Unavailable"
                  : b.ac
                    ? "Connected"
                    : "Disconnected",
              ],
              ["Charging", str(b?.charging)],
              [
                "Estimated wear",
                pct(typeof b?.health === "number" ? 100 - b.health : null),
              ],
            ]}
          />
        </Panel>
        <Panel title="Battery packs">
          <RawTable
            data={b?.packs ?? []}
            columns={{
              manufacturer: "Manufacturer",
              model: "Model",
              design: "Design capacity",
              full: "Full-charge capacity",
              remainingCapacity: "Remaining capacity",
              unit: "Capacity unit",
              cycles: "Cycles",
              charge: "Charge mW",
              discharge: "Discharge mW",
              source: "Source",
            }}
          />
          {!b?.packs?.length && (
            <ProviderNotes rows={p.probes} names={["Battery driver"]} />
          )}
        </Panel>
        <MetricChart
          config={c}
          keys={["battery", "batteryHealth"]}
          title="Battery history"
        />
      </>
    );
  }
  if (page === "Network") return <NetworkPage s={s} c={c} />;
  return (
    <>
      <div className="stat-row">
        <Stat
          name="CPU package"
          value={t(m.cpuTemp)}
          detail={readingDetail(
            s.cpu.temperatureReading,
            "No CPU package sensor reported",
          )}
          icon={Cpu}
        />
        <Stat
          name="GPU core"
          value={gpuTemperatureText(s, c)}
          detail={temperatureSource(s.gpuReading) ?? undefined}
          icon={Monitor}
          tone="blue"
        />
        <Stat
          name="Drive temperature"
          value={t(m.driveTemp)}
          detail="Highest reported drive temperature"
          icon={Database}
          tone="purple"
        />
        <Stat
          name="Fastest fan"
          value={qty(m.fan, "RPM")}
          detail={readingDetail(
            s.fanReading,
            "The controller must expose an RPM sensor",
          )}
          icon={Fan}
          tone="yellow"
        />
      </div>
      <ThermalAccess sample={s} />
      <Panel
        title="Hardware sensor inventory"
        subtitle="LibreHardwareMonitor can supply CPU package temperature, package power and fan RPM through WMI or an enabled loopback connection."
      >
        <Sensors data={p.sensors ?? []} c={c} />
        <ProviderNotes rows={p.probes} />
        {s.providerError && <Notice>{s.providerError}</Notice>}
      </Panel>
      <MetricChart
        config={c}
        keys={["cpuTemp", "gpuTemp", "driveTemp"]}
        title="Temperature history"
      />
      <MetricChart config={c} keys={["fan"]} title="Fan speed history" />
    </>
  );
}
function NetworkPage({ s, c }: { s: Sample; c: Config }) {
  const [check, setCheck] = useState<Raw | null>(null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const recent = useLoad<Raw[]>("records", []);
  useEffect(() => {
    void recent.load({ kind: "connection" });
  }, [recent.load]);
  return (
    <>
      <div className="stat-row">
        <Stat
          name="Download"
          value={bytes(s.metrics.rx, true)}
          icon={ArrowDownLeft}
        />
        <Stat
          name="Upload"
          value={bytes(s.metrics.tx, true)}
          icon={ArrowUpRight}
          tone="blue"
        />
        <Stat
          name="Network link"
          value={
            s.connected == null
              ? "Unavailable"
              : s.connected
                ? "Connected"
                : "Disconnected"
          }
          icon={Wifi}
          tone="purple"
        />
      </div>
      <Panel title="Network interfaces" icon={Network}>
        <RawTable
          data={s.adapters.map((a) => ({ ...a }))}
          columns={{
            name: "Interface",
            ips: "Local IPs",
            rx: "Download",
            tx: "Upload",
            rxTotal: "Downloaded",
            txTotal: "Uploaded",
          }}
          format={{
            rx: (v) => bytes(v, true),
            tx: (v) => bytes(v, true),
            rxTotal: (v) => bytes(v),
            txTotal: (v) => bytes(v),
          }}
        />
      </Panel>
      <div className="detail-grid">
        <Panel title="Adapter details">
          <RawTable
            data={s.providers?.adapters ?? []}
            columns={{
              Name: "Adapter",
              NetConnectionID: "Connection",
              Speed: "Link bits/s",
            }}
          />
        </Panel>
        <Panel
          title="Wi-Fi details"
          subtitle="Current adapter and link information from Windows WLAN."
        >
          {s.providers?.wifi?.length ? (
            <>
              <RawTable
                data={s.providers.wifi}
                columns={{
                  name: "Adapter",
                  state: "Status",
                  phy: "Standard",
                  signal: "Signal",
                  rssi: "RSSI",
                  channel: "Channel",
                  frequencyMHz: "Frequency",
                  rxMbps: "Receive link",
                  txMbps: "Transmit link",
                }}
                format={{
                  signal: pct,
                  rssi: (v) => qty(v, "dBm"),
                  frequencyMHz: (v) => qty(v, "MHz"),
                  rxMbps: (v) => qty(v, "Mb/s", 1),
                  txMbps: (v) => qty(v, "Mb/s", 1),
                }}
              />
              {s.providers.wifi
                .filter((row) => row.unavailableReason)
                .map((row, i) => (
                  <Notice key={i}>{str(row.unavailableReason)}</Notice>
                ))}
            </>
          ) : s.providers?.probes?.some((row) => row.name === "Wi-Fi") ? (
            <ProviderNotes rows={s.providers.probes} names={["Wi-Fi"]} />
          ) : (
            <Empty text="Reading Wi-Fi adapters…" loading />
          )}
        </Panel>
      </div>
      <Panel
        title="Connectivity checks"
        subtitle={
          c.offline
            ? "Offline mode allows checks to the local default gateway."
            : c.connectivityTarget
              ? "Checks use the configured IP address."
              : "Checks use the local default gateway."
        }
        action={
          <Button
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              setError("");
              try {
                setCheck(await api<Raw>("connectivity"));
                void recent.load({ kind: "connection" });
              } catch (e) {
                setError(errorText(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            {busy ? "Checking…" : "Run connectivity check"}
          </Button>
        }
      >
        {error && <Notice bad>{error}</Notice>}
        {check && (
          <Pairs
            data={[
              ["Target", str(check.target)],
              ["Latency", qty(check.latency, "ms", 1)],
              ["Packet loss", pct(check.loss)],
            ]}
          />
        )}
        <RawTable
          data={recent.value}
          columns={{
            ts: "Time",
            target: "Target",
            latency: "Latency",
            loss: "Loss",
            status: "Connection",
          }}
          format={{
            ts: (v) => date(Number(v)),
            latency: (v) => qty(v, "ms", 1),
            loss: pct,
          }}
        />
      </Panel>
      <MetricChart config={c} keys={["rx", "tx"]} title="Network activity" />
    </>
  );
}
function MemoryProcesses() {
  const p = useLoad<Process[]>("processes", []);
  useEffect(() => {
    void p.load();
    const id = setInterval(() => {
      if (document.visibilityState === "visible") void p.load();
    }, 5000);
    return () => clearInterval(id);
  }, [p.load]);
  return (
    <Panel title="Highest memory usage">
      {p.error && <Notice bad>{p.error}</Notice>}
      <RawTable
        data={p.value.slice(0, 7)}
        columns={{ name: "Process", pid: "PID", memory: "Memory" }}
        format={{ memory: (v) => bytes(v) }}
      />
    </Panel>
  );
}
export function RawTable({
  data,
  columns,
  format = {},
}: {
  data: Raw[];
  columns: Record<string, string>;
  format?: Record<string, (v: unknown) => string>;
}) {
  return data.length ? (
    <Table headers={Object.values(columns)}>
      {data.map((r, i) => (
        <tr key={i}>
          {Object.keys(columns).map((k) => (
            <td key={k}>{format[k]?.(r[k]) ?? str(r[k])}</td>
          ))}
        </tr>
      ))}
    </Table>
  ) : (
    <Empty detail="The Windows provider has no supported data for this view." />
  );
}
function Sensors({ data, c }: { data: Raw[]; c: Config }) {
  return data.length ? (
    <Table headers={["Sensor", "Type", "Reading", "Min", "Max", "Hardware"]}>
      {data.map((r, i) => {
        const f = (v: unknown) =>
          r.SensorType === "Temperature"
            ? temp(v, c.fahrenheit)
            : qty(
                v,
                String(r.SensorType) === "Fan"
                  ? "RPM"
                  : ((
                      {
                        Power: "W",
                        Clock: "MHz",
                        Load: "%",
                        Voltage: "V",
                        Data: "GiB",
                      } as Record<string, string>
                    )[String(r.SensorType)] ?? ""),
                1,
              );
        return (
          <tr key={i}>
            <td>{str(r.Name)}</td>
            <td>{str(r.SensorType)}</td>
            <td>{f(r.Value)}</td>
            <td>{f(r.Min)}</td>
            <td>{f(r.Max)}</td>
            <td>{str(r.Parent)}</td>
          </tr>
        );
      })}
    </Table>
  ) : (
    <Empty
      text="No sensors exposed"
      detail="Start the LibreHardwareMonitor WMI provider for compatible hardware."
    />
  );
}

function readingDetail(reading: Raw | null | undefined, empty: string) {
  if (!reading) return empty;
  return (
    [reading.name, reading.source]
      .filter((v) => typeof v === "string" && v.length > 0)
      .join(" · ") || empty
  );
}
function ProviderNotes({
  rows,
  names,
  includeAvailable = true,
}: {
  rows?: Raw[];
  names?: string[];
  includeAvailable?: boolean;
}) {
  const selected = (rows ?? []).filter(
    (row) =>
      (!names || names.includes(String(row.name))) &&
      (includeAvailable || row.status !== "Available"),
  );
  if (!selected.length) return null;
  return (
    <div className="telemetry-notes">
      {selected.map((row, i) => (
        <Notice key={i}>
          {str(row.name)} · {str(row.detail)}
        </Notice>
      ))}
    </div>
  );
}
