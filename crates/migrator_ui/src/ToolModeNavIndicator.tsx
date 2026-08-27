import { useLayoutEffect, useState, type RefObject } from "react";

export interface ToolModeNavIndicatorProps {
  /** Ref to the `.toolModeNav` container element. */
  navRef: RefObject<HTMLElement | null>;
  /** Currently active tool mode: migrate | repatch | merge | head-merge. */
  active: string;
}

/**
 * One shared sliding underline for the whole tool-mode navigation (primary
 * tabs + advanced-tools dropdown).
 *
 * Replaces both MUI's per-Tabs indicator — which collapses to `left:0,width:0`
 * (and animates there) whenever no primary tab is selected — and the dropdown
 * button's own box-shadow bar. With a single bar positioned from the active
 * element (the matching tab, or the dropdown trigger for advanced tools), the
 * highlight slides continuously between the tabs and the button instead of
 * appearing to "come from the left".
 */
export function ToolModeNavIndicator({ navRef, active }: ToolModeNavIndicatorProps) {
  const [bar, setBar] = useState({ left: 0, width: 0 });

  useLayoutEffect(() => {
    const nav = navRef.current;
    if (!nav) return;
    const measure = () => {
      const isPrimary = active === "migrate" || active === "repatch";
      const el = isPrimary
        ? nav.querySelector('[role="tab"][aria-selected="true"]')
        : nav.querySelector(".advancedToolsTab");
      if (!el) return;
      const navRect = nav.getBoundingClientRect();
      const elRect = el.getBoundingClientRect();
      setBar({ left: elRect.left - navRect.left, width: elRect.width });
    };
    // Measure before first paint so the bar never animates in from the left.
    measure();
    // Re-measure when the nav resizes (tab labels / button label change width).
    const observer = new ResizeObserver(measure);
    observer.observe(nav);
    window.addEventListener("resize", measure);
    // Re-measure after fonts load so tab widths settle.
    document.fonts?.ready?.then(measure).catch(() => {});
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, [navRef, active]);

  return (
    <span
      aria-hidden="true"
      className="toolModeIndicator"
      style={{ left: bar.left, width: bar.width }}
    />
  );
}
