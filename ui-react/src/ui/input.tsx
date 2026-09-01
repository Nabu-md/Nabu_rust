import { type InputHTMLAttributes, type TextareaHTMLAttributes } from "react";
import { Icon } from "../components/layout/icons";

export interface TextInputProps extends InputHTMLAttributes<HTMLInputElement> { label?: string; error?: string; }
export function TextInput({ label, error, className = "", ...rest }: TextInputProps) {
  const inputClasses = [
    "w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700",
    "focus:border-blue-500 focus:outline-none",
    error && "border-red-500 focus:border-red-500",
    className,
  ].filter(Boolean).join(" ");
  return (
    <div className="flex flex-col gap-1">
      {label && <label className="text-xs text-gray-400">{label}</label>}
      <input className={inputClasses} {...rest} />
      {error && <span className="text-xs text-red-400">{error}</span>}
    </div>
  );
}

export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {}
export function Textarea({ className = "", ...rest }: TextareaProps) {
  const classes = ["w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700", "focus:border-blue-500 focus:outline-none resize-none", className].filter(Boolean).join(" ");
  return <textarea className={classes} {...rest} />;
}

export interface SearchInputProps extends InputHTMLAttributes<HTMLInputElement> {}
export function SearchInput({ className = "", ...rest }: SearchInputProps) {
  const classes = ["w-full bg-gray-800 text-gray-100 rounded pl-8 pr-3 py-1.5 text-sm border border-gray-700", "focus:border-blue-500 focus:outline-none", className].filter(Boolean).join(" ");
  return (
    <div className="relative">
      <Icon name="search" className="absolute left-2 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-500" />
      <input className={classes} {...rest} />
    </div>
  );
}

export interface NumberInputProps extends InputHTMLAttributes<HTMLInputElement> {}
export function NumberInput({ className = "", ...rest }: NumberInputProps) {
  const classes = ["w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700", "focus:border-blue-500 focus:outline-none", className].filter(Boolean).join(" ");
  return <input type="number" className={classes} {...rest} />;
}
