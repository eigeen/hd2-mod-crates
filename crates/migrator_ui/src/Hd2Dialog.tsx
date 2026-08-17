import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";

type AnimPhase = "enter" | "idle" | "exit";

interface Hd2DialogProps {
  ariaLabel?: string;
  open: boolean;
  onClose?: () => void;
  children: ReactNode;
  size?: "small" | "large" | "guide";
}

/**
 * HD2-style dialog.
 * Open animation: top/bottom lines expand from center (clip-path),
 * then content fades in once fully expanded.
 */
export function Hd2Dialog({ ariaLabel, open, onClose, children, size = "small" }: Hd2DialogProps) {
  const [mounted, setMounted] = useState(open);
  const [phase, setPhase] = useState<AnimPhase>(open ? "idle" : "exit");

  useEffect(() => {
    if (open) {
      setMounted(true);
      // Double rAF: ensures mount is committed + painted before animation starts.
      let inner = 0;
      const outer = requestAnimationFrame(() => {
        inner = requestAnimationFrame(() => setPhase("enter"));
      });
      return () => {
        cancelAnimationFrame(outer);
        cancelAnimationFrame(inner);
      };
    } else {
      setPhase("exit");
    }
  }, [open]);

  useEffect(() => {
    if (!open || !onClose) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [onClose, open]);

  // Watch animationName so each phase transitions at the right moment:
  // - "enter" → "idle" only after the content-reveal animation (the last one) finishes
  // - "exit" → unmount after the collapse animation finishes
  const handleAnimationEnd = (e: React.AnimationEvent) => {
    if (phase === "enter" && e.animationName === "hd2-dialog-content-reveal") {
      setPhase("idle");
    }
    if (phase === "exit" && e.animationName === "hd2-dialog-exit") {
      setMounted(false);
    }
  };

  if (!mounted) return null;

  const panelClass =
    phase === "enter" ? "hd2-dialog-enter" :
    phase === "exit"  ? "hd2-dialog-exit"  :
    "";

  const overlayOpacity = phase === "exit" ? "opacity-0" : "opacity-100";

  return createPortal(
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center transition-opacity duration-150 ${overlayOpacity}`}
      style={{ background: "rgba(0,0,0,0.55)" }}
      onClick={onClose}
    >
      {/* Only top + bottom borders, no left/right. bg semi-transparent. */}
      <div
        aria-label={ariaLabel}
        aria-modal="true"
        className={`w-full ${dialogSizeClass(size)} border-t-2 border-b-2 border-hd2-yellow ${panelClass}`}
        role="dialog"
        style={{ background: "rgba(8,8,8,0.82)" }}
        onClick={(e) => e.stopPropagation()}
        onAnimationEnd={handleAnimationEnd}
      >
        <div className="hd2-dialog-body">
          {children}
        </div>
      </div>
    </div>,
    document.body,
  );
}

function dialogSizeClass(size: NonNullable<Hd2DialogProps["size"]>): string {
  if (size === "guide") return "mx-3 max-w-4xl";
  return size === "large" ? "max-w-2xl" : "max-w-sm";
}

export function Hd2DialogTitle({ children }: { children: ReactNode }) {
  return (
    <div className="hd2-stripes-accent border-b border-hd2-border px-6 py-4">
      <h2 className="m-0 text-sm font-bold uppercase tracking-[0.1em] text-hd2-yellow">
        {children}
      </h2>
    </div>
  );
}

export function Hd2DialogContent({ children }: { children: ReactNode }) {
  return <div className="px-6 py-5">{children}</div>;
}

export function Hd2DialogActions({ children }: { children: ReactNode }) {
  return (
    <div className="flex items-center justify-end gap-3 border-t border-hd2-border bg-hd2-pit px-5 py-3">
      {children}
    </div>
  );
}
