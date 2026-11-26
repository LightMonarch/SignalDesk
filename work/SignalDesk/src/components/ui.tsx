import { useEffect, useRef, type ReactNode } from "react";
import {
  Activity,
  AlertCircle,
  ChevronDown,
  Inbox,
  LoaderCircle,
  Search,
  X,
  type LucideIcon,
} from "lucide-react";
import { Button } from "./ui/button";
export { Button };
export function Panel({
  title,
  subtitle,
  children,
  action,
  icon: Icon,
  className = "",
}: {
  title?: string;
  subtitle?: string;
  children: ReactNode;
  action?: ReactNode;
  icon?: LucideIcon;
  className?: string;
}) {
  return (
    <section className={"panel " + className}>
      {(title || action) && (
        <div className="panel-head">
          <div>
            <h2>
              {Icon && <Icon size={16} />} {title}
            </h2>
            {subtitle && <p>{subtitle}</p>}
          </div>
          {action}
        </div>
      )}
      {children}
    </section>
  );
}
export function Empty({
  text = "No readings available",
  detail,
  loading = false,
}: {
  text?: string;
  detail?: string;
  loading?: boolean;
}) {
  const I = loading ? LoaderCircle : Inbox;
  return (
    <div className="empty">
      <I size={22} className={loading ? "animate-spin" : ""} />
      <strong>{text}</strong>
      {detail && <p>{detail}</p>}
    </div>
  );
}
export function Notice({
  children,
  bad = false,
}: {
  children: ReactNode;
  bad?: boolean;
}) {
  return (
    <div
      role={bad ? "alert" : "status"}
      className={"notice " + (bad ? "bad" : "")}
    >
      <AlertCircle size={17} />
      <div>{children}</div>
    </div>
  );
}
export function Badge({
  children,
  tone = "neutral",
}: {
  children: ReactNode;
  tone?: string;
}) {
  return (
    <span className={"badge " + tone}>
      <span className="dot" />
      {children}
    </span>
  );
}
export function Pairs({ data }: { data: [string, ReactNode][] }) {
  return (
    <dl className="pairs">
      {data.map(([k, v]) => (
        <div key={k}>
          <dt>{k}</dt>
          <dd>{v ?? "Unavailable"}</dd>
        </div>
      ))}
    </dl>
  );
}
export function Field({
  label,
  children,
  hint,
}: {
  label: string;
  children: ReactNode;
  hint?: string;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
      {hint && <small>{hint}</small>}
    </label>
  );
}
export function Select({
  value,
  onChange,
  options,
  label,
}: {
  value: string;
  onChange: (v: string) => void;
  options: (string | { value: string; label: string })[];
  label: string;
}) {
  return (
    <div className="select">
      <select
        aria-label={label}
        value={value}
        onChange={(e) => onChange(e.currentTarget.value)}
      >
        {options.map((o) =>
          typeof o === "string" ? (
            <option key={o}>{o}</option>
          ) : (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ),
        )}
      </select>
      <ChevronDown size={14} />
    </div>
  );
}
export function Filter({
  value,
  onChange,
  label,
}: {
  value: string;
  onChange: (v: string) => void;
  label: string;
}) {
  return (
    <label className="filter">
      <Search size={16} />
      <input
        type="search"
        placeholder={label}
        aria-label={label}
        value={value}
        onChange={(e) => onChange(e.currentTarget.value)}
      />
    </label>
  );
}
export function Table({
  headers,
  children,
}: {
  headers: ReactNode[];
  children: ReactNode;
}) {
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            {headers.map((v, i) => (
              <th key={i} scope="col">
                {v}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>{children}</tbody>
      </table>
    </div>
  );
}
export function Dialog({
  title,
  close,
  children,
}: {
  title: string;
  close: () => void;
  children: ReactNode;
}) {
  const node = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const n = node.current;
    if (n && !n.open) n.showModal();
    return () => n?.close();
  }, []);
  return (
    <dialog
      ref={node}
      className="dialog"
      onCancel={(e) => {
        e.preventDefault();
        close();
      }}
      aria-labelledby="dialog-title"
    >
      <div className="dialog-head">
        <h2 id="dialog-title">{title}</h2>
        <Button aria-label="Close dialog" variant="ghost" onClick={close}>
          <X size={18} />
        </Button>
      </div>
      {children}
    </dialog>
  );
}
export function PageTitle({
  title,
  description,
  actions,
}: {
  title: string;
  description: string;
  actions?: ReactNode;
}) {
  return (
    <div className="page-title">
      <div>
        <h1>{title}</h1>
        <p>{description}</p>
      </div>
      <div className="tools">{actions}</div>
    </div>
  );
}
export function Stat({
  name,
  value,
  detail,
  icon: Icon = Activity,
  tone = "accent",
}: {
  name: string;
  value: string;
  detail?: string;
  icon?: LucideIcon;
  tone?: string;
}) {
  return (
    <div className={"stat t-" + tone}>
      <div className="stat-icon">
        <Icon size={17} />
      </div>
      <div className="stat-name">{name}</div>
      <strong className={value === "Unavailable" ? "unavailable" : ""}>
        {value}
      </strong>
      {detail && <p>{detail}</p>}
    </div>
  );
}
export function Confirm({
  title,
  description,
  label,
  run,
  close,
}: {
  title: string;
  description: string;
  label: string;
  run: () => void;
  close: () => void;
}) {
  return (
    <Dialog title={title} close={close}>
      <p className="confirm-copy">{description}</p>
      <div className="dialog-actions">
        <Button onClick={close}>Cancel</Button>
        <Button className="destructive" onClick={run}>
          {label}
        </Button>
      </div>
    </Dialog>
  );
}
