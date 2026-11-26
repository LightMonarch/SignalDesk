import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "./api";
import { exportData } from "./reports";

vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  api: vi.fn().mockResolvedValue(true),
}));

const invoke = vi.mocked(api);
const report = { title: "<Reading>", formula: "=1+1", missing: null };

function savedFile() {
  const [name, options] = invoke.mock.lastCall ?? [];
  expect(name).toBe("save_file");
  expect(options).toBeDefined();
  return {
    filename: options!.filename as string,
    contents: new TextDecoder().decode(new Uint8Array(options!.bytes as number[])),
  };
}

describe("report export", () => {
  beforeEach(() => invoke.mockClear());

  it("preserves typed values in JSON", async () => {
    expect(await exportData("report", report, "JSON")).toBe(true);
    const file = savedFile();
    expect(file.filename).toBe("report.json");
    expect(JSON.parse(file.contents)).toEqual(report);
  });

  it("escapes HTML content and quotes formula-like CSV fields", async () => {
    await exportData("report", report, "HTML");
    expect(savedFile().contents).toContain("&lt;Reading&gt;");
    expect(savedFile().contents).not.toContain("<Reading>");

    await exportData("report", report, "CSV");
    expect(savedFile().contents).toMatch(/"formula","'=1\+1"/);
    expect(savedFile().contents).toContain('"missing","Unavailable"');
  });

  it("emits a complete PDF for the Windows save dialog", async () => {
    await exportData("report", report, "PDF");
    const file = savedFile();
    expect(file.filename).toBe("report.pdf");
    expect(file.contents.startsWith("%PDF-")).toBe(true);
    expect(file.contents.trimEnd().endsWith("%%EOF")).toBe(true);
  });
});
