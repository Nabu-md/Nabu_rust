import { type ButtonHTMLAttributes } from "react";
import { Icon } from "../components/layout/icons";

export enum ButtonVariant {
  Primary = "primary",
  Secondary = "secondary",
  Ghost = "ghost",
  Outline = "outline",
  Destructive = "destructive",
  Icon = "icon",
}

export enum ButtonSize {
  Sm = "sm",
  Md = "md",
  Lg = "lg",
}

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
  icon?: string;
  ariaLabel?: string;
}

const BASE_CLASSES = "inline-flex items-center justify-center rounded font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-offset-1 focus:ring-offset-gray-900";
const VARIANT_CLASSES: Record<ButtonVariant, string> = {
  [ButtonVariant.Primary]: "bg-blue-600 text-white hover:bg-blue-700 focus:ring-blue-500",
  [ButtonVariant.Secondary]: "bg-gray-800 text-gray-200 hover:bg-gray-700 focus:ring-gray-500",
  [ButtonVariant.Ghost]: "bg-transparent text-gray-400 hover:bg-gray-800 hover:text-gray-200 focus:ring-gray-500",
  [ButtonVariant.Outline]: "border border-gray-700 text-gray-300 hover:bg-gray-800 focus:ring-gray-500",
  [ButtonVariant.Destructive]: "bg-red-600 text-white hover:bg-red-700 focus:ring-red-500",
  [ButtonVariant.Icon]: "bg-transparent text-gray-400 hover:bg-gray-800 hover:text-gray-200 focus:ring-gray-500 p-1",
};
const SIZE_CLASSES: Record<ButtonSize, string> = {
  [ButtonSize.Sm]: "px-2 py-1 text-xs",
  [ButtonSize.Md]: "px-3 py-1.5 text-sm",
  [ButtonSize.Lg]: "px-4 py-2 text-base",
};

export function Button({ variant = ButtonVariant.Secondary, size = ButtonSize.Md, loading = false, icon, ariaLabel, children, className = "", disabled, ...rest }: ButtonProps) {
  const disabledAttr = disabled || loading;
  const classes = [BASE_CLASSES, VARIANT_CLASSES[variant], SIZE_CLASSES[size], "btn", className].filter(Boolean).join(" ");
  return (
    <button className={classes} disabled={disabledAttr} aria-busy={loading || undefined}
      {...(ariaLabel ? { "aria-label": ariaLabel } : {})} {...rest}>
      {loading ? (
        <span className="animate-spin rounded-full border-2 border-current border-t-transparent w-4 h-4" />
      ) : icon ? (
        <Icon name={icon} className="w-4 h-4 mr-1.5" />
      ) : null}
      {children}
    </button>
  );
}
