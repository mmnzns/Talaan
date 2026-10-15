import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from "react";

const ToastContext = createContext<(message: string) => void>(() => {});

export const useToast = () => useContext(ToastContext);

/** One short message at the bottom of the window. Used for actions that finish elsewhere or in a later build. */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [message, setMessage] = useState<string>();
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const show = useCallback((m: string) => {
    setMessage(m);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setMessage(undefined), 2800);
  }, []);

  return (
    <ToastContext.Provider value={show}>
      {children}
      <div aria-live="polite" className="pointer-events-none fixed inset-x-0 bottom-5 z-[60] flex justify-center px-4">
        {message && (
          <div className="rounded-control border border-edge bg-strong px-3.5 py-2 text-[13px] font-semibold text-strong-fg shadow-card">
            {message}
          </div>
        )}
      </div>
    </ToastContext.Provider>
  );
}
