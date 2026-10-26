import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { AlertTriangle, Check, Info, Loader2, X } from "lucide-react";

/* --------------------------------- card --------------------------------- */

export function Card({
  children,
  className = "",
  ...rest
}: { children: ReactNode; className?: string } & React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={`card ${className}`} {...rest}>
      {children}
    </div>
  );
}

export function SectionTitle({
  title,
  hint,
  action,
}: {
  title: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <div className="section-title">
      <div>
        <h2>{title}</h2>
        {hint && <p>{hint}</p>}
      </div>
      {action}
    </div>
  );
}

/* -------------------------------- buttons ------------------------------- */

type ButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "ghost" | "quiet" | "danger";
  size?: "md" | "sm";
  icon?: ReactNode;
  loading?: boolean;
};

export function Button({
  variant = "ghost",
  size = "md",
  icon,
  loading,
  children,
  className = "",
  disabled,
  ...rest
}: ButtonProps) {
  return (
    <button
      className={`btn btn-${variant} btn-${size} ${className}`}
      disabled={disabled || loading}
      {...rest}
    >
      {loading ? <Loader2 size={16} className="spin" /> : icon}
      {children && <span>{children}</span>}
    </button>
  );
}

/* ------------------------------ indicators ------------------------------ */

export function ProgressBar({
  value,
  tone = "accent",
}: {
  value: number;
  tone?: "accent" | "warn" | "danger" | "cool";
}) {
  return (
    <div className="progress">
      <div
        className={`progress-fill tone-${tone}`}
        style={{ width: `${Math.min(100, Math.max(0, value))}%` }}
      />
    </div>
  );
}

export function Ring({
  value,
  size = 132,
  stroke = 10,
  label,
  sub,
}: {
  value: number;
  size?: number;
  stroke?: number;
  label?: string;
  sub?: string;
}) {
  const radius = (size - stroke) / 2;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference * (1 - Math.min(100, Math.max(0, value)) / 100);
  const tone = value >= 80 ? "var(--accent)" : value >= 55 ? "var(--warn)" : "var(--danger)";
  return (
    <div className="ring" style={{ width: size, height: size }}>
      <svg width={size} height={size}>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke="var(--border)"
          strokeWidth={stroke}
        />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke={tone}
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={offset}
          transform={`rotate(-90 ${size / 2} ${size / 2})`}
          style={{ transition: "stroke-dashoffset .6s ease" }}
        />
      </svg>
      <div className="ring-label">
        <strong>{label ?? Math.round(value)}</strong>
        {sub && <span>{sub}</span>}
      </div>
    </div>
  );
}

export function Sparkline({
  data,
  color = "var(--accent)",
  height = 42,
  fill = true,
}: {
  data: number[];
  color?: string;
  height?: number;
  fill?: boolean;
}) {
  const width = 220;
  const points = data.length > 1 ? data : [0, 0];
  const max = Math.max(...points, 1);
  const step = width / (points.length - 1);
  const coords = points.map((value, index) => {
    const x = index * step;
    const y = height - (value / max) * (height - 6) - 3;
    return `${x.toFixed(2)},${y.toFixed(2)}`;
  });
  const line = `M${coords.join(" L")}`;
  const area = `${line} L${width},${height} L0,${height} Z`;
  return (
    <svg
      className="sparkline"
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      height={height}
    >
      {fill && <path d={area} fill={color} opacity="0.12" />}
      <path d={line} fill="none" stroke={color} strokeWidth="1.8" strokeLinejoin="round" />
    </svg>
  );
}

export function RiskBadge({ risk, admin }: { risk: string; admin?: boolean }) {
  if (admin) return <span className="badge badge-warn">需要管理员</span>;
  if (risk === "caution") return <span className="badge badge-warn">谨慎</span>;
  return <span className="badge badge-ok">安全</span>;
}

export function EmptyState({
  icon,
  title,
  hint,
  action,
}: {
  icon: ReactNode;
  title: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty">
      <div className="empty-icon">{icon}</div>
      <h3>{title}</h3>
      {hint && <p>{hint}</p>}
      {action}
    </div>
  );
}

export function Spinner({ label }: { label?: string }) {
  return (
    <div className="loading-block">
      <Loader2 size={22} className="spin" />
      {label && <span>{label}</span>}
    </div>
  );
}

/* ------------------------------- checkbox ------------------------------- */

export function Checkbox({
  checked,
  onChange,
  label,
  hint,
  disabled,
  className = "",
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: ReactNode;
  hint?: ReactNode;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <label className={`check ${checked ? "is-checked" : ""} ${disabled ? "is-disabled" : ""} ${className}`}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span className="check-box">{checked && <Check size={13} strokeWidth={3} />}</span>
      <span className="check-body">
        <span className="check-label">{label}</span>
        {hint && <span className="check-hint">{hint}</span>}
      </span>
    </label>
  );
}

/* --------------------------------- modal -------------------------------- */

export function Modal({
  open,
  title,
  description,
  children,
  footer,
  onClose,
  tone = "default",
  wide = false,
}: {
  open: boolean;
  title: string;
  description?: string;
  children?: ReactNode;
  footer?: ReactNode;
  onClose: () => void;
  tone?: "default" | "danger";
  wide?: boolean;
}) {
  useEffect(() => {
    if (!open) return;
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;
  // Portal to body: page-level transform animations would otherwise become the
  // containing block for position: fixed, pushing the dialog off-center.
  return createPortal(
    <div className="modal-backdrop" onMouseDown={onClose}>
      <div
        className={`modal ${tone === "danger" ? "modal-danger" : ""} ${wide ? "modal-wide" : ""}`}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="modal-head">
          <div className="modal-head-icon">
            {tone === "danger" ? <AlertTriangle size={18} /> : <Info size={18} />}
          </div>
          <div>
            <h3>{title}</h3>
            {description && <p>{description}</p>}
          </div>
          <button className="icon-btn" onClick={onClose} aria-label="关闭">
            <X size={18} />
          </button>
        </div>
        {children && <div className="modal-body">{children}</div>}
        {footer && <div className="modal-footer">{footer}</div>}
      </div>
    </div>,
    document.body,
  );
}

/* --------------------------------- toast -------------------------------- */

type ToastTone = "success" | "error" | "info";
type Toast = { id: number; text: string; tone: ToastTone };

const ToastContext = createContext<(text: string, tone?: ToastTone) => void>(() => {});

export function useToast() {
  return useContext(ToastContext);
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const idRef = useRef(0);

  const push = (text: string, tone: ToastTone = "info") => {
    const id = ++idRef.current;
    setToasts((current) => [...current, { id, text, tone }]);
    setTimeout(() => {
      setToasts((current) => current.filter((toast) => toast.id !== id));
    }, 3800);
  };

  return (
    <ToastContext.Provider value={push}>
      {children}
      <div className="toasts">
        {toasts.map((toast) => (
          <div key={toast.id} className={`toast toast-${toast.tone}`}>
            {toast.tone === "success" ? (
              <Check size={16} />
            ) : toast.tone === "error" ? (
              <AlertTriangle size={16} />
            ) : (
              <Info size={16} />
            )}
            <span>{toast.text}</span>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}
