import { useEffect, useRef } from "react";
import { useLocation } from "react-router";

export function focusPageContent() {
  const main = document.getElementById("page-content");
  // Preserve a modal or focus already placed by the destination page.
  if (
    !main ||
    document.querySelector("dialog:modal") ||
    main.contains(document.activeElement)
  )
    return;
  const target = main.querySelector<HTMLElement>("h1") ?? main;
  if (!target.hasAttribute("tabindex")) target.tabIndex = -1;
  target.focus({ preventScroll: true });
}

export function RouteFocus() {
  const { pathname } = useLocation();
  const previous = useRef(pathname);
  useEffect(() => {
    if (previous.current === pathname) return;
    previous.current = pathname;
    const frame = requestAnimationFrame(focusPageContent);
    return () => cancelAnimationFrame(frame);
  }, [pathname]);
  return null;
}
