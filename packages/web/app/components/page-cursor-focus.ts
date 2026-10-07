import { useEffect, useRef } from "react";

// Keep initial document loading alone; move focus when this page's cursor changes.
export function usePageCursorFocus(cursor: string | null) {
  const heading = useRef<HTMLHeadingElement>(null),
    previous = useRef(cursor);
  useEffect(() => {
    if (previous.current === cursor) return;
    previous.current = cursor;
    heading.current?.focus({ preventScroll: true });
    heading.current?.scrollIntoView({ block: "start", behavior: "instant" });
  }, [cursor]);
  return heading;
}
