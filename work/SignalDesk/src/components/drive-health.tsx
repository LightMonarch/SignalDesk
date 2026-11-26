import { useState } from "react";
import { HardDrive } from "lucide-react";
import { bytes, pct, str, temp, type Raw } from "../api";
import { Badge, Empty, Notice, Panel, Pairs, Select, Table } from "./ui";
const counters: [string, string][] = [
  ["hours", "Power-on hours"],
  ["powerCycles", "Power cycles"],
  ["startStop", "Start/stop cycles"],
  ["readErrors", "Read errors"],
  ["writeErrors", "Write errors"],
  ["mediaErrors", "Combined media errors"],
  ["errorLogEntries", "Error log entries"],
  ["unsafeShutdowns", "Unsafe shutdowns"],
  ["ataHoursRaw", "ATA SMART 09 raw"],
];

function amount(value: unknown) {
  return typeof value === "number"
    ? bytes(value)
    : typeof value === "string"
      ? str(value) + " B"
      : "Unavailable";
}

export function DriveHealth({
  drives,
  fahrenheit,
}: {
  drives: Raw[];
  fahrenheit: boolean;
}) {
  const [selected, setSelected] = useState("0");
  const index = Math.min(Number(selected), Math.max(0, drives.length - 1));
  const drive = drives[index];
  const attributes = Array.isArray(drive?.attributes)
    ? (drive.attributes as Raw[])
    : [];
  const tone =
    drive?.status === "Healthy"
      ? "ok"
      : drive?.status === "Warning" || drive?.status === "Unhealthy"
        ? "warn"
        : "neutral";
  return (
    <Panel
      title="Drive health"
      icon={HardDrive}
      action={
        drive && (
          <Select
            label="Physical drive"
            value={String(index)}
            onChange={setSelected}
            options={drives.map((row, i) => ({
              value: String(i),
              label: str(row.name) + " · " + (i + 1),
            }))}
          />
        )
      }
    >
      {!drive ? (
        <Empty
          text="Drive telemetry unavailable"
          detail="Windows did not return physical drive health counters."
        />
      ) : (
        <>
          <div className="drive-summary">
            <div>
              <small>Health</small>
              <Badge tone={tone}>{str(drive.status)}</Badge>
            </div>
            <div>
              <small>Temperature</small>
              <strong>{temp(drive.temperature, fahrenheit)}</strong>
            </div>
            <div>
              <small>Wear used</small>
              <strong>{pct(drive.wear)}</strong>
            </div>
            <div>
              <small>Source</small>
              <strong>{str(drive.source)}</strong>
            </div>
          </div>
          {typeof drive.unavailableReason === "string" && (
            <Notice>{drive.unavailableReason}</Notice>
          )}
          <details className="drive-counters" open>
            <summary>Lifetime and error counters</summary>
            <Pairs
              data={[
                ...counters.map(([key, label]): [string, string] => [
                  label,
                  str(drive[key]),
                ]),
                ...(drive.bus === "NVMe"
                  ? ([
                      ["Data read", amount(drive.dataReadBytes)],
                      ["Data written", amount(drive.dataWrittenBytes)],
                      ["Available spare", pct(drive.spare)],
                      ["Spare threshold", pct(drive.spareThreshold)],
                      ["Critical warning flags", str(drive.criticalWarning)],
                    ] as [string, string][])
                  : []),
              ]}
            />
          </details>
          {attributes.length > 0 && (
            <details className="drive-counters">
              <summary>Raw ATA SMART attributes</summary>
              <Table
                headers={[
                  "ID",
                  "Attribute",
                  "Current",
                  "Worst",
                  "Raw hexadecimal",
                ]}
              >
                {attributes.map((a, i) => (
                  <tr key={i}>
                    <td>{str(a.id)}</td>
                    <td>{str(a.label)}</td>
                    <td>{str(a.current)}</td>
                    <td>{str(a.worst)}</td>
                    <td className="drive-raw">{str(a.raw)}</td>
                  </tr>
                ))}
              </Table>
            </details>
          )}
        </>
      )}
    </Panel>
  );
}
