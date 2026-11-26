import { useState } from "react";
import {
  Download,
  FileCheck,
  FileJson,
  FileText,
  LockKeyhole,
} from "lucide-react";
import {
  api,
  date,
  errorText,
  rows,
  str,
  tsNow,
  type Bench,
  type History,
  type Issue,
  type Raw,
  type Sample,
} from "./api";
import {
  Button,
  Empty,
  Field,
  Notice,
  PageTitle,
  Panel,
  Select,
  Table,
} from "./components/ui";
export type Format = "PDF" | "CSV" | "JSON" | "HTML";
const types = [
  "System health",
  "Hardware specifications",
  "Performance",
  "Diagnostics",
  "Battery health",
  "Storage health",
  "Benchmarks",
  "Alerts",
];
const chosen = ["Performance", "Diagnostics", "Benchmarks", "Alerts"];
const sensitive =
  /(^host$|serial|uuid|^pid$|machine|address|^target$|^ips$|^path$|^message$|^pnpdeviceid$|instanceName|^adapter$|^computername$|^mac$)/i;
function redact(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(redact);
  if (v && typeof v === "object")
    return Object.fromEntries(
      Object.entries(v)
        .filter(([k]) => !sensitive.test(k))
        .map(([k, value]) => [k, redact(value)]),
    );
  return v;
}
function flatten(value: unknown, prefix = "", out: [string, string][] = []) {
  if (value && typeof value === "object") {
    const entries = Object.entries(value);
    if (!entries.length) out.push([prefix, "No records"]);
    for (const [name, v] of entries)
      flatten(
        v,
        prefix
          ? [
              prefix,
              Array.isArray(value) ? String(Number(name) + 1) : name,
            ].join(" / ")
          : name,
        out,
      );
  } else out.push([prefix, str(value)]);
  return out;
}
function html(v: string) {
  return v.replace(
    /[&<>"']/g,
    (x) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        x
      ]!,
  );
}
const cell = (s: string) =>
  '"' + (/^[\t\r\n ]*[=+@-]/.test(s) ? "'" : "") + s.replace(/"/g, '""') + '"';
export async function exportData(name: string, data: unknown, format: Format) {
  let encoded: Uint8Array;
  if (format === "JSON") {
    encoded = new TextEncoder().encode(JSON.stringify(data, null, 2));
  } else {
    const items = flatten(data);
    if (format === "CSV") {
      encoded = new TextEncoder().encode(
        "\ufeffField,Value\r\n" +
          items.map((r) => r.map(cell).join(",")).join("\r\n"),
      );
    } else if (format === "HTML") {
      encoded = new TextEncoder().encode(
        '<!doctype html><html lang="en"><meta charset="utf-8"><title>' +
          html(name) +
          "</title><style>body{font:15px system-ui;max-width:1000px;margin:3rem auto;padding:1rem}td,th{border-bottom:1px solid #d8dee4;text-align:left;padding:.6rem;vertical-align:top}table{border-collapse:collapse;width:100%}td{white-space:pre-wrap;overflow-wrap:anywhere}</style><h1>" +
          html(name) +
          "</h1><table><tr><th>Field</th><th>Value</th></tr>" +
          items
            .map(
              ([a, b]) =>
                "<tr><td>" + html(a) + "</td><td>" + html(b) + "</td></tr>",
            )
            .join("") +
          "</table></html>",
      );
    } else {
      const { jsPDF } = await import("jspdf");
      const pdf = new jsPDF();
      let y = 20;
      const clean = (v: string) =>
        v.normalize("NFKD").replace(/[^\x20-\x7e\n]/g, "?");
      const head = () => {
        pdf.setFontSize(16);
        pdf.text(clean(name), 15, 20);
        pdf.setFontSize(9);
        y = 33;
      };
      head();
      for (const [a, b] of items) {
        const lines = pdf.splitTextToSize(
          clean(a + "  ·  " + b),
          180,
        ) as string[];
        for (const line of lines) {
          if (y > 279) {
            pdf.addPage();
            head();
          }
          pdf.text(line, 15, y);
          y += 5;
        }
        y += 1;
      }
      encoded = new Uint8Array(pdf.output("arraybuffer"));
    }
  }
  return api<boolean>("save_file", {
    filename: name + "." + format.toLowerCase(),
    bytes: Array.from(encoded),
  });
}
export function Reports({ sample: s }: { sample: Sample | null }) {
  const [type, setType] = useState("System health"),
    [period, setPeriod] = useState("24 hours"),
    [format, setFormat] = useState<Format>("PDF"),
    [hide, setHide] = useState(true),
    [segments, setSegments] = useState([
      "CPU",
      "GPU",
      "Memory",
      "Storage",
      "Battery",
      "Cooling",
      "Network",
      "Windows",
    ]),
    [data, setData] = useState<Raw | null>(null),
    [busy, setBusy] = useState(false),
    [message, setMessage] = useState(""),
    [bad, setBad] = useState(false);
  const periodSeconds: Record<string, number> = {
    "1 hour": 3600,
    "24 hours": 86400,
    "7 days": 604800,
    "30 days": 2592000,
  };
  const invalidate = () => {
    setData(null);
    setMessage("");
  };
  async function preview() {
    setBusy(true);
    setMessage("");
    try {
      const now = tsNow(),
        start = now - periodSeconds[period];
      let main: unknown;
      if (type === "Performance")
        main = await api<History>("history", { start, end: now + 1 });
      else if (chosen.includes(type)) {
        const kind =
          type === "Diagnostics"
            ? "diagnostic"
            : type === "Benchmarks"
              ? "benchmark"
              : "alert";
        main = (await api<(Issue | Bench)[]>("records", { kind })).filter(
          (x) => x.ts >= start,
        );
        if (hide)
          main = rows(main).map((x) => ({
            ts: x.ts,
            id: x.id,
            component: x.component,
            title: x.title,
            severity: x.severity,
            status: x.status,
            kind: x.kind,
            value: x.value,
            unit: x.unit,
            duration: x.duration,
            read: x.read,
            loss: x.loss,
          }));
      } else if (type === "Hardware specifications") {
        const inv = await api<Raw>("inventory");
        main = Object.fromEntries(
          Object.entries(inv).filter(
            ([k]) =>
              (
                ({
                  cpu: "CPU",
                  memory: "Memory",
                  slots: "Memory",
                  windows: "Windows",
                  bios: "Windows",
                  board: "Windows",
                  computer: "Windows",
                  secureBoot: "Windows",
                  tpm: "Windows",
                  powerPlan: "Windows",
                  displays: "GPU",
                }) as Record<string, string>
              )[k] &&
              segments.includes(
                (
                  {
                    cpu: "CPU",
                    memory: "Memory",
                    slots: "Memory",
                    windows: "Windows",
                    bios: "Windows",
                    board: "Windows",
                    computer: "Windows",
                    secureBoot: "Windows",
                    tpm: "Windows",
                    powerPlan: "Windows",
                    displays: "GPU",
                  } as Record<string, string>
                )[k],
              ),
          ),
        );
      } else if (type === "Battery health") main = s?.battery;
      else if (type === "Storage health")
        main = {
          volumes: hide ? s?.disks.map(({ name, mount, ...d }) => d) : s?.disks,
          drives: s?.providers?.driveHealth,
        };
      else {
        if (!s) throw Error("Wait for the first hardware sample.");
        main = {
          score: s.health.score,
          classification: s.health.label,
          components: s.health.parts.filter((x) => segments.includes(x.name)),
          host: s.host,
          system: segments.includes("Windows")
            ? { os: s.os, uptime: s.uptime, powerPlan: s.powerPlan }
            : undefined,
        };
      }
      const doc = {
        report: type,
        generated: date(now),
        ...(chosen.includes(type) ? { from: date(start), to: date(now) } : {}),
        data: main,
      };
      setData((hide ? redact(doc) : doc) as Raw);
    } catch (e) {
      setBad(true);
      setMessage(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  async function download() {
    if (!data) return;
    setBusy(true);
    setMessage("");
    try {
      const saved = await exportData(
        "signaldesk-" + type.toLowerCase().replaceAll(" ", "-"),
        data,
        format,
      );
      if (saved) {
        setBad(false);
        setMessage("Report saved.");
      }
    } catch (e) {
      setBad(true);
      setMessage(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  const items = data ? flatten(data) : [];
  return (
    <>
      <PageTitle
        title="Reports"
        description="Preview and export local monitoring, hardware and diagnostic data."
      />
      <Panel title="Report options" icon={FileCheck}>
        <div className="fields">
          <Field label="Report type">
            <Select
              label="Report type"
              options={types}
              value={type}
              onChange={(v) => {
                setType(v);
                invalidate();
              }}
            />
          </Field>
          {chosen.includes(type) && (
            <Field label="Reporting period">
              <Select
                label="Reporting period"
                options={Object.keys(periodSeconds)}
                value={period}
                onChange={(v) => {
                  setPeriod(v);
                  invalidate();
                }}
              />
            </Field>
          )}
          <Field label="Export format">
            <Select
              label="Export format"
              options={["PDF", "CSV", "JSON", "HTML"]}
              value={format}
              onChange={(v) => setFormat(v as Format)}
            />
          </Field>
        </div>
        {(type === "System health" || type === "Hardware specifications") && (
          <div className="tools option-row">
            {[
              "CPU",
              "GPU",
              "Memory",
              "Storage",
              "Battery",
              "Cooling",
              "Network",
              "Windows",
            ].map((k) => (
              <label key={k}>
                <input
                  type="checkbox"
                  checked={segments.includes(k)}
                  onChange={(e) => {
                    setSegments(
                      e.target.checked
                        ? [...segments, k]
                        : segments.filter((x) => x !== k),
                    );
                    invalidate();
                  }}
                />
                {k}
              </label>
            ))}
          </div>
        )}
        <div className="tools form-bar">
          <label className="check">
            <input
              type="checkbox"
              checked={hide}
              onChange={(e) => {
                setHide(e.target.checked);
                invalidate();
              }}
            />
            <LockKeyhole size={15} /> Exclude sensitive identifiers
          </label>
          <Button disabled={busy} onClick={() => void preview()}>
            <FileText size={16} /> Generate preview
          </Button>
          <Button
            disabled={busy || !data}
            variant="primary"
            onClick={() => void download()}
          >
            <Download size={16} /> Save {format}
          </Button>
        </div>
        {message && <Notice bad={bad}>{message}</Notice>}
      </Panel>
      <Panel
        title="Report preview"
        icon={FileJson}
        subtitle={
          items.length > 80
            ? "Showing 80 of " +
              items.length +
              " fields. Export includes the full report."
            : undefined
        }
      >
        {data ? (
          <Table headers={["Field", "Value"]}>
            {items.slice(0, 80).map(([a, b], i) => (
              <tr key={i}>
                <td>{a}</td>
                <td>{b}</td>
              </tr>
            ))}
          </Table>
        ) : (
          <Empty
            text={busy ? "Generating preview" : "No preview generated"}
            loading={busy}
          />
        )}
      </Panel>
    </>
  );
}
