// ──────────────────────────────────────────────────────────────────────────────
// ToastContext — toast notification system
//
// Mirrors: crates/nabu-ui/src/components/ui/feedback (ToastContext)
// Contract: read-only spec consumed by Wave 3 view components.
// ──────────────────────────────────────────────────────────────────────────────

import {
  createContext,
  useContext,
  useState,
  useCallback,
  useRef,
  type ReactNode,
} from "react";

export type ToastVariant = "info" | "success" | "warning" | "error";

export interface Toast {
  id: string;
  message: string;
  variant: ToastVariant;
  /** Auto-dismiss delay in ms (null = no auto-dismiss). */
  duration: number | null;
  /** Timestamp when the toast was created. */
  createdAt: number;
}

export interface ToastContextValue {
  /** Currently active toasts. */
  toasts: Toast[];
  /** Show a toast notification. */
  toast: (message: string, opts?: { variant?: ToastVariant; duration?: number | null }) => void;
  /** Dismiss a specific toast by id. */
  dismiss: (id: string) => void;
  /** Dismiss all toasts. */
  dismissAll: () => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

/** Hook to access the toast context. */
export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error("useToast must be used within a ToastProvider");
  }
  return ctx;
}

let toastCounter = 0;
function nextToastId(): string {
  return `toast-${++toastCounter}-${Date.now()}`;
}

interface ToastProviderProps {
  children: ReactNode;
}

/** Provider component for toast notifications. */
export function ToastProvider({ children }: ToastProviderProps) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const timersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(
    new Map()
  );

  const dismiss = useCallback((id: string) => {
    const timer = timersRef.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timersRef.current.delete(id);
    }
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const toast = useCallback(
    (
      message: string,
      opts?: { variant?: ToastVariant; duration?: number | null }
    ) => {
      const id = nextToastId();
      const variant: ToastVariant = opts?.variant ?? "info";
      const duration = opts?.duration ?? 4000;
      const newToast: Toast = {
        id,
        message,
        variant,
        duration,
        createdAt: Date.now(),
      };
      setToasts((prev) => [...prev, newToast]);

      if (duration !== null) {
        const timer = setTimeout(() => {
          dismiss(id);
        }, duration);
        timersRef.current.set(id, timer);
      }
    },
    [dismiss]
  );

  const dismissAll = useCallback(() => {
    for (const timer of timersRef.current.values()) {
      clearTimeout(timer);
    }
    timersRef.current.clear();
    setToasts([]);
  }, []);

  const value: ToastContextValue = {
    toasts,
    toast,
    dismiss,
    dismissAll,
  };

  return (
    <ToastContext.Provider value={value}>{children}</ToastContext.Provider>
  );
}
