import { useCallback, useLayoutEffect, useRef, useState, type ReactNode } from "react";

interface IndicatorBounds {
  left: number;
  width: number;
}

/** Own the container ref so measurement runs after its DOM has been attached. */
export function ToolModeNav({ children }: { children: ReactNode }) {
  const navRef = useRef<HTMLDivElement>(null);
  const [bounds, setBounds] = useState<IndicatorBounds | null>(null);
  const measure = useCallback(() => {
    const nav = navRef.current;
    const selected = nav?.querySelector<HTMLElement>('[data-tool-nav-active="true"]');
    if (!nav || !selected) return;
    const rect = selected.getBoundingClientRect();
    if (rect.width === 0) return;
    const left = rect.left - nav.getBoundingClientRect().left + nav.scrollLeft;
    setBounds((previous) => previous?.left === left && previous.width === rect.width
      ? previous
      : { left, width: rect.width });
  }, []);

  // Selection and translated labels are committed before this measurement.
  useLayoutEffect(measure);

  useLayoutEffect(() => {
    const nav = navRef.current;
    if (!nav) return;
    const observer = new window.ResizeObserver(measure);
    observer.observe(nav);
    // Observe every control: a preceding label can move the selected control.
    nav.querySelectorAll('[data-tool-nav-active]').forEach((control) => observer.observe(control));
    nav.addEventListener("scroll", measure, true);
    return () => {
      observer.disconnect();
      nav.removeEventListener("scroll", measure, true);
    };
  }, [measure]);

  return (
    <div className="toolModeNav" ref={navRef}>
      {children}
      {bounds && (
        <span
          aria-hidden="true"
          className="toolModeIndicator"
          style={{ transform: `translateX(${bounds.left}px)`, width: bounds.width }}
        />
      )}
    </div>
  );
}
