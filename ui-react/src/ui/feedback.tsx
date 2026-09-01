import { type HTMLAttributes } from "react";
import { Icon } from "../components/layout/icons";

export enum SpinnerSize { Sm = "sm", Md = "md", Lg = "lg" }
const SPINNER_SIZE_CLASSES: Record<SpinnerSize, string> = {
  [SpinnerSize.Sm]: "w-3 h-3", [SpinnerSize.Md]: "w-4 h-4", [SpinnerSize.Lg]: "w-5 h-5",
};
export interface SpinnerProps extends HTMLAttributes<HTMLSpanElement> { size?: SpinnerSize; label?: string; }
export function Spinner({ size = SpinnerSize.Md, label, className = "", ...rest }: SpinnerProps) {
  const classes = ["animate-spin rounded-full border-2 border-current border-t-transparent", SPINNER_SIZE_CLASSES[size], className].filter(Boolean).join(" ");
  return <span className={classes} role="status" aria-label={label} {...rest} />;
}

export interface SkeletonProps extends HTMLAttributes<HTMLDivElement> {}
export function Skeleton({ className = "", ...rest }: SkeletonProps) {
  return <div className={["animate-pulse bg-gray-800 rounded", className].filter(Boolean).join(" ")} {...rest} />;
}
export interface SkeletonListProps extends HTMLAttributes<HTMLDivElement> { rows?: number; }
export function SkeletonList({ rows = 5, className = "", ...rest }: SkeletonListProps) {
  return (
    <div className={["space-y-1 p-2", className].filter(Boolean).join(" ")} {...rest}>
      {Array.from({ length: rows }).map((_, i) => (
        <Skeleton key={i} className="h-4 bg-gray-800 rounded" style={{ width: `${40 + i * 10}%` }} />
      ))}
    </div>
  );
}

export interface ErrorPanelProps extends HTMLAttributes<HTMLDivElement> { title: string; message: string; }
export function ErrorPanel({ title, message, className = "", ...rest }: ErrorPanelProps) {
  return (
    <div className={["bg-red-900/20 border border-red-800 rounded-lg p-3 mt-2", className].filter(Boolean).join(" ")} role="alert" {...rest}>
      <p className="text-sm font-semibold text-red-300">{title}</p>
      <p className="text-xs text-red-400 mt-1">{message}</p>
    </div>
  );
}

export interface LoadingBlockProps extends HTMLAttributes<HTMLDivElement> { size?: SpinnerSize; label?: string; }
export function LoadingBlock({ size = SpinnerSize.Md, label = "Loading…", className = "", ...rest }: LoadingBlockProps) {
  return (
    <div className={["flex items-center justify-center gap-2 py-4", className].filter(Boolean).join(" ")} {...rest}>
      <Spinner size={size} />
      <span className="text-sm text-gray-400">{label}</span>
    </div>
  );
}

export enum StatusKind { Info = "info", Success = "success", Warning = "warning", Error = "error" }
const STATUS_COLOR_CLASSES: Record<StatusKind, string> = {
  [StatusKind.Info]: "bg-blue-500", [StatusKind.Success]: "bg-green-500",
  [StatusKind.Warning]: "bg-amber-500", [StatusKind.Error]: "bg-red-500",
};
export interface StatusDotProps extends HTMLAttributes<HTMLSpanElement> { kind: StatusKind; label?: string; pulse?: boolean; }
export function StatusDot({ kind, label, pulse = false, className = "", ...rest }: StatusDotProps) {
  const classes = ["inline-block w-2 h-2 rounded-full shrink-0", STATUS_COLOR_CLASSES[kind], pulse && "animate-pulse", className].filter(Boolean).join(" ");
  return (
    <span className="inline-flex items-center gap-1">
      <span className={classes} role="status" aria-label={label} {...rest} />
      {label && <span className="text-xs text-gray-400">{label}</span>}
    </span>
  );
}

export interface EmptyStateProps extends HTMLAttributes<HTMLDivElement> { icon?: string; title: string; description?: string; }
export function EmptyState({ icon, title, description, className = "", ...rest }: EmptyStateProps) {
  return (
    <div className={["flex flex-col items-center justify-center text-center py-8", className].filter(Boolean).join(" ")} {...rest}>
      {icon && <Icon name={icon} className="w-10 h-10 text-gray-600 mb-3" />}
      <p className="text-gray-400 font-medium">{title}</p>
      {description && <p className="text-sm text-gray-600 mt-1">{description}</p>}
    </div>
  );
}
