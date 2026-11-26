import { useEffect, useState } from "react";
import { Bot, CornerDownLeft } from "lucide-react";
import { api, errorText, type Config } from "./api";
import {
  Badge,
  Button,
  Empty,
  Notice,
  PageTitle,
  Panel,
} from "./components/ui";
export function Assistant({
  config: c,
  settings,
}: {
  config: Config;
  settings: () => void;
}) {
  const [q, setQ] = useState(""),
    [events, setEvents] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [availability, setAvailability] = useState("");
  const [thread, setThread] = useState<{ question: string; answer: string }[]>(
    [],
  );
  useEffect(() => {
    if (c.ai) void check();
  }, [c.ai, c.model]);
  async function check() {
    setAvailability("Checking…");
    try {
      await api("ai_status");
      setAvailability("Connected");
    } catch (e) {
      setAvailability(errorText(e));
    }
  }
  async function send() {
    const question = q.trim();
    if (!question) return;
    setBusy(true);
    setError("");
    try {
      const answer = await api<string>("ask", {
        prompt: question,
        includeEvents: events,
      });
      setThread((t) => [...t, { question, answer }]);
      setQ("");
      setAvailability("Connected");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <PageTitle
        title="AI Assistant"
        description="Ask an installed Ollama model to interpret local system readings."
        actions={
          <Badge tone={availability === "Connected" ? "ok" : "neutral"}>
            {c.ai
              ? availability === "Connected"
                ? "Model connected"
                : "Ollama enabled"
              : "Disabled"}
          </Badge>
        }
      />
      <Panel
        icon={Bot}
        title="Local system assistant"
        subtitle={c.ai ? c.model + " · 127.0.0.1:11434" : undefined}
        action={
          c.ai ? (
            <Button onClick={() => void check()}>Test connection</Button>
          ) : (
            <Button onClick={settings}>Open Settings</Button>
          )
        }
      >
        {!c.ai ? (
          <Empty
            text="Local AI is disabled"
            detail="Enable Ollama in Settings and choose a model installed on this PC."
          />
        ) : (
          <>
            <div className="conversation">
              {availability && availability !== "Connected" && (
                <Notice>{availability}</Notice>
              )}
              {thread.length ? (
                thread.map((t, i) => (
                  <article key={i}>
                    <strong>{t.question}</strong>
                    <p>{t.answer}</p>
                  </article>
                ))
              ) : (
                <Empty
                  text="Ask about this computer"
                  detail="Try asking which resources are under pressure or what a recent Windows error means."
                />
              )}
            </div>
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void send();
              }}
            >
              <textarea
                aria-label="Question for Ollama"
                placeholder="Ask about system health…"
                maxLength={5000}
                value={q}
                onChange={(e) => setQ(e.target.value)}
              />
              <div className="tools form-bar">
                <label className="check">
                  <input
                    type="checkbox"
                    checked={events}
                    onChange={(e) => setEvents(e.target.checked)}
                  />{" "}
                  Include recent event messages
                </label>
                <Button
                  variant="primary"
                  disabled={busy || !q.trim()}
                  type="submit"
                >
                  <CornerDownLeft size={16} /> {busy ? "Thinking…" : "Send"}
                </Button>
              </div>
            </form>
            {error && <Notice bad>{error}</Notice>}
          </>
        )}
      </Panel>
    </>
  );
}
