import { type HTMLAttributes, type ReactNode, useEffect, useRef } from "react";
import { Button, ButtonVariant } from "./button";

export enum DialogSize { Sm = "sm", Md = "md", Lg = "lg", Xl = "xl", Full = "full" }
const SIZE_CLASSES: Record<DialogSize, string> = {
  [DialogSize.Sm]: "max-w-sm", [DialogSize.Md]: "max-w-md", [DialogSize.Lg]: "max-w-lg",
  [DialogSize.Xl]: "max-w-xl", [DialogSize.Full]: "max-w-6xl",
};

export interface DialogProps extends HTMLAttributes<HTMLDivElement> {
  open: boolean; size?: DialogSize; title: string; onClose: () => void; children: ReactNode;
}
export function Dialog({ open, size = DialogSize.Md, title, onClose, children, className = "", ...rest }: DialogProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);
  useEffect(() => {
    if (open) { const prev = document.body.style.overflow; document.body.style.overflow = "hidden"; return () => { document.body.style.overflow = prev; }; }
  }, [open]);
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }} {...rest}>
      <div ref={panelRef}
        className={["bg-gray-900 border border-gray-700 rounded-lg shadow-2xl w-full mx-4", SIZE_CLASSES[size], className].filter(Boolean).join(" ")}
        role="dialog" aria-modal="true" aria-label={title}>
        {children}
      </div>
    </div>
  );
}

export interface ConfirmDialogProps {
  open: boolean; title: string; message: string; confirmLabel?: string;
  cancelLabel?: string; danger?: boolean; onClose: () => void; onConfirm: () => void;
}
export function ConfirmDialog({ open, title, message, confirmLabel = "Confirm", cancelLabel = "Cancel", danger = false, onClose, onConfirm }: ConfirmDialogProps) {
  return (
    <Dialog open={open} title={title} onClose={onClose}>
      <div className="p-4">
        <h3 className="font-semibold text-gray-100 mb-2">{title}</h3>
        <p className="text-sm text-gray-400 mb-4">{message}</p>
        <div className="flex gap-2 justify-end">
          <Button variant={ButtonVariant.Secondary} onClick={onClose}>{cancelLabel}</Button>
          <Button variant={danger ? ButtonVariant.Destructive : ButtonVariant.Primary}
            onClick={() => { onConfirm(); onClose(); }}>{confirmLabel}</Button>
        </div>
      </div>
    </Dialog>
  );
}

export interface AlertDialogProps {
  open: boolean; title: string; message: string; confirmLabel?: string;
  danger?: boolean; onClose: () => void; onConfirm: () => void;
}
export function AlertDialog({ open, title, message, confirmLabel = "OK", danger = true, onClose, onConfirm }: AlertDialogProps) {
  return <ConfirmDialog open={open} title={title} message={message} confirmLabel={confirmLabel} danger={danger} onClose={onClose} onConfirm={onConfirm} />;
}

export interface PromptDialogProps {
  open: boolean; title: string; message: string; defaultValue?: string;
  confirmLabel?: string; onClose: () => void; onConfirm: (value: string) => void;
}
export function PromptDialog({ open, title, message, defaultValue = "", confirmLabel = "OK", onClose, onConfirm }: PromptDialogProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => { if (open) { inputRef.current?.focus(); inputRef.current?.select(); } }, [open]);
  return (
    <Dialog open={open} title={title} onClose={onClose}>
      <div className="p-4">
        <h3 className="font-semibold text-gray-100 mb-2">{title}</h3>
        <p className="text-sm text-gray-400 mb-3">{message}</p>
        <input ref={inputRef} type="text" defaultValue={defaultValue}
          className="w-full bg-gray-800 text-gray-100 rounded px-3 py-1.5 text-sm border border-gray-700 focus:border-blue-500 focus:outline-none mb-4"
          onKeyDown={(e) => { if (e.key === "Enter") { onConfirm((e.target as HTMLInputElement).value); onClose(); } if (e.key === "Escape") onClose(); }} />
        <div className="flex gap-2 justify-end">
          <Button variant={ButtonVariant.Secondary} onClick={onClose}>Cancel</Button>
          <Button variant={ButtonVariant.Primary} onClick={() => { onConfirm(inputRef.current?.value ?? defaultValue); onClose(); }}>{confirmLabel}</Button>
        </div>
      </div>
    </Dialog>
  );
}
