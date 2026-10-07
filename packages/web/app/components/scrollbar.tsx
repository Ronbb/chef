import { useEffect, useRef, useState } from "react";
export function Scrollbar() {
  const track = useRef<HTMLDivElement>(null),
    drag = useRef<{ y: number; scroll: number } | null>(null),
    [metrics, setMetrics] = useState({ max: 0, value: 0, size: 32, offset: 0 }),
    [visible, setVisible] = useState(false);
  useEffect(() => {
    let frame = 0,
      timer: ReturnType<typeof setTimeout>;
    const update = () => {
      frame = 0;
      const root = document.documentElement,
        height = track.current?.clientHeight || Math.max(0, innerHeight - 16),
        max = Math.max(0, root.scrollHeight - innerHeight),
        size = Math.min(
          height,
          Math.max(32, (height * innerHeight) / root.scrollHeight),
        );
      setMetrics({
        max,
        value: Math.round(scrollY),
        size,
        offset: max ? (scrollY / max) * (height - size) : 0,
      });
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    const scroll = () => {
      schedule();
      setVisible(true);
      clearTimeout(timer);
      timer = setTimeout(() => {
        if (!drag.current) setVisible(false);
      }, 1000);
    };
    const observer = new ResizeObserver(schedule);
    observer.observe(document.body);
    addEventListener("resize", schedule);
    addEventListener("scroll", scroll, { passive: true });
    update();
    return () => {
      observer.disconnect();
      removeEventListener("resize", schedule);
      removeEventListener("scroll", scroll);
      cancelAnimationFrame(frame);
      clearTimeout(timer);
    };
  }, []);
  return (
    <div
      ref={track}
      className={"page-scrollbar" + (visible ? " visible" : "")}
      hidden={!metrics.max}
      role="scrollbar"
      aria-label="页面滚动"
      aria-orientation="vertical"
      aria-controls="page-content"
      aria-valuemin={0}
      aria-valuemax={metrics.max}
      aria-valuenow={metrics.value}
      tabIndex={0}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        const thumb =
          e.currentTarget.firstElementChild!.getBoundingClientRect();
        if (e.clientY < thumb.top || e.clientY > thumb.bottom) {
          scrollBy(0, (e.clientY < thumb.top ? -1 : 1) * innerHeight * 0.85);
          return;
        }
        drag.current = { y: e.clientY, scroll: scrollY };
        e.currentTarget.setPointerCapture(e.pointerId);
        e.preventDefault();
      }}
      onPointerMove={(e) => {
        if (drag.current) {
          const travel = e.currentTarget.clientHeight - metrics.size;
          if (travel > 0)
            scrollTo(
              0,
              drag.current.scroll +
                ((e.clientY - drag.current.y) / travel) * metrics.max,
            );
        }
      }}
      onPointerUp={() => {
        drag.current = null;
      }}
      onPointerCancel={() => {
        drag.current = null;
      }}
      onLostPointerCapture={() => {
        drag.current = null;
      }}
      onKeyDown={(e) => {
        const steps: Record<string, number> = {
          ArrowDown: 48,
          ArrowUp: -48,
          PageDown: innerHeight * 0.85,
          PageUp: -innerHeight * 0.85,
        };
        if (e.key in steps) {
          e.preventDefault();
          scrollBy(0, steps[e.key]);
        } else if (e.key === "Home" || e.key === "End") {
          e.preventDefault();
          scrollTo(
            0,
            e.key === "Home" ? 0 : document.documentElement.scrollHeight,
          );
        }
      }}
    >
      <div
        className="page-scrollbar-thumb"
        style={{
          height: metrics.size,
          transform: `translateY(${metrics.offset}px)`,
        }}
      />
    </div>
  );
}
