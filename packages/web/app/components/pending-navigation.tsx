import { useEffect, useId, useRef } from "react";
import { useBlocker } from "react-router";
import { Icon } from "./icon";

// Call once per route. The original request is already retained by its writer;
// leaving does not cancel it or create a replacement request.
export function PendingNavigation({
  active,
  onStay,
}: {
  active: boolean;
  onStay: () => void;
}) {
  const blocker = useBlocker(active),
    dialog = useRef<HTMLDialogElement>(null),
    heading = useRef<HTMLHeadingElement>(null),
    titleId = useId();
  useEffect(() => {
    if (blocker.state !== "blocked") {
      dialog.current?.close();
      return;
    }
    if (!active) {
      blocker.reset();
      dialog.current?.close();
      onStay();
      return;
    }
    if (!dialog.current?.open) {
      dialog.current?.showModal();
      heading.current?.focus();
    }
  }, [active, blocker, onStay]);
  function stay() {
    if (blocker.state === "blocked") blocker.reset();
    dialog.current?.close();
    onStay();
  }
  return (
    <dialog
      ref={dialog}
      className="choice-dialog pending-navigation"
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        stay();
      }}
    >
      <div className="rate-heading">
        <h2 id={titleId} ref={heading} tabIndex={-1}>
          这次提交尚未确认
        </h2>
        <button
          type="button"
          className="icon-button"
          aria-label="留在当前页"
          onClick={stay}
        >
          <Icon name="close" />
        </button>
      </div>
      <p>原提交已保留在此标签页。现在离开，返回后可以确认保存结果。</p>
      <button type="button" className="primary" onClick={stay}>
        留在当前页
      </button>
      <button
        type="button"
        className="text-button"
        onClick={() => {
          if (blocker.state === "blocked") blocker.proceed();
        }}
      >
        继续离开
      </button>
    </dialog>
  );
}
