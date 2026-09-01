import { type HTMLAttributes, type MouseEvent } from "react";
import { Icon } from "../components/layout/icons";

export interface MenuItemProps extends HTMLAttributes<HTMLDivElement> {
  label: string; icon?: string; hint?: string; danger?: boolean;
  disabled?: boolean; onSelect?: (e: MouseEvent<HTMLDivElement>) => void;
}
export function MenuItem({ label, icon, hint, danger = false, disabled = false, onSelect, className = "", ...rest }: MenuItemProps) {
  const classes = [
    "menu-item flex items-center gap-2 px-2 py-1 text-sm rounded cursor-pointer",
    "hover:bg-gray-800 text-gray-300",
    danger && "hover:bg-red-900/30 hover:text-red-300",
    disabled && "opacity-50 cursor-not-allowed",
    className,
  ].filter(Boolean).join(" ");
  return (
    <div className={classes} onClick={disabled ? undefined : onSelect} aria-disabled={disabled || undefined} {...rest}>
      {icon && <Icon name={icon} className="w-3.5 h-3.5 text-gray-500" />}
      <span className="flex-1">{label}</span>
      {hint && <span className="text-xs text-gray-600 ml-auto">{hint}</span>}
    </div>
  );
}

export interface MenuSeparatorProps extends HTMLAttributes<HTMLDivElement> {}
export function MenuSeparator({ className = "", ...rest }: MenuSeparatorProps) {
  return <div className={["my-1 border-t border-gray-700", className].filter(Boolean).join(" ")} {...rest} />;
}

export interface MenuProps extends HTMLAttributes<HTMLDivElement> { children: React.ReactNode; }
export function Menu({ children, className = "", ...rest }: MenuProps) {
  return (
    <div className={["menu fixed z-50 min-w-48 bg-gray-900 border border-gray-700 rounded shadow-xl py-1", className].filter(Boolean).join(" ")} role="menu" {...rest}>
      {children}
    </div>
  );
}
