import { type HTMLAttributes } from "react";

export enum CardVariant {
  Default = "default",
  Interactive = "interactive",
  Elevated = "elevated",
}

const VARIANT_CLASSES: Record<CardVariant, string> = {
  [CardVariant.Default]: "bg-gray-900 border border-gray-800 rounded-lg shadow-sm",
  [CardVariant.Interactive]: "bg-gray-900 border border-gray-800 rounded-lg shadow-sm hover:border-gray-600 transition-colors",
  [CardVariant.Elevated]: "bg-gray-900 border border-gray-700 rounded-lg shadow-xl",
};

export function Card({ variant = CardVariant.Default, className = "", children, ...rest }: { variant?: CardVariant; className?: string } & HTMLAttributes<HTMLDivElement>) {
  const classes = [VARIANT_CLASSES[variant], className].filter(Boolean).join(" ");
  return <div className={classes} {...rest}>{children}</div>;
}

export function CardHeader({ className = "", children, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return <div className={["px-4 py-3 border-b border-gray-800", className].filter(Boolean).join(" ")} {...rest}>{children}</div>;
}

export function CardBody({ className = "", children, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return <div className={["px-4 py-3", className].filter(Boolean).join(" ")} {...rest}>{children}</div>;
}

export function CardFooter({ className = "", children, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return <div className={["px-4 py-2 border-t border-gray-800", className].filter(Boolean).join(" ")} {...rest}>{children}</div>;
}
