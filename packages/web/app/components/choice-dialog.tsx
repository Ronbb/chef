import { useEffect, useId, useMemo, useRef, useState } from "react";
import { Icon } from "./icon";
export type Choice = { value: string; label: string; detail?: string };
export function ChoiceDialog({
  title,
  value,
  choices,
  onChange,
  searchable = false,
  disabled = false,
}: {
  title: string;
  value: string;
  choices: Choice[];
  onChange: (value: string) => void;
  searchable?: boolean;
  disabled?: boolean;
}) {
  const dialog = useRef<HTMLDialogElement>(null),
    id = useId();
  const [search, setSearch] = useState("");
  const visible = useMemo(
    () =>
      choices.filter((choice) =>
        (choice.label + " " + choice.value + " " + (choice.detail ?? ""))
          .toLowerCase()
          .includes(search.trim().toLowerCase()),
      ),
    [choices, search],
  );
  useEffect(() => {
    if (disabled) dialog.current?.close();
  }, [disabled]);
  return (
    <>
      <button
        type="button"
        className="speed-trigger"
        disabled={disabled}
        aria-haspopup="dialog"
        aria-label={`${title}：${choices.find((choice) => choice.value === value)?.label ?? value}`}
        onClick={() => {
          setSearch("");
          dialog.current?.showModal();
          if (searchable)
            dialog.current?.querySelector<HTMLInputElement>("input")?.focus();
          else
            dialog.current
              ?.querySelector<HTMLButtonElement>('[aria-checked="true"]')
              ?.focus();
        }}
      >
        {choices.find((choice) => choice.value === value)?.label ?? value}
        <Icon name="chevron" />
      </button>
      <dialog
        ref={dialog}
        className="choice-dialog"
        aria-labelledby={id}
        onClick={(event) => {
          if (event.target !== dialog.current || !dialog.current) return;
          const rect = dialog.current.getBoundingClientRect();
          if (
            event.clientX < rect.left ||
            event.clientX > rect.right ||
            event.clientY < rect.top ||
            event.clientY > rect.bottom
          )
            dialog.current.close();
        }}
      >
        <div className="rate-heading">
          <h2 id={id}>{title}</h2>
          <button
            type="button"
            className="icon-button"
            aria-label="关闭选项"
            onClick={() => dialog.current?.close()}
          >
            <Icon name="close" />
          </button>
        </div>
        {searchable && (
          <input
            className="choice-search"
            type="search"
            placeholder="搜索城市或时区"
            aria-label="搜索时区"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        )}
        <div className="choice-options" role="radiogroup" aria-label={title}>
          {visible.map((choice, index) => (
            <button
              type="button"
              className="rate-option"
              role="radio"
              aria-checked={choice.value === value}
              tabIndex={
                choice.value === value ||
                (index === 0 &&
                  !visible.some((option) => option.value === value))
                  ? 0
                  : -1
              }
              key={choice.value}
              onKeyDown={(event) => {
                let next = index;
                if (["ArrowDown", "ArrowRight"].includes(event.key))
                  next = (index + 1) % visible.length;
                else if (["ArrowUp", "ArrowLeft"].includes(event.key))
                  next = (index + visible.length - 1) % visible.length;
                else if (event.key === "Home") next = 0;
                else if (event.key === "End") next = visible.length - 1;
                else return;
                event.preventDefault();
                onChange(visible[next].value);
                dialog.current
                  ?.querySelectorAll<HTMLButtonElement>('[role="radio"]')
                  [next]?.focus();
              }}
              onClick={() => {
                onChange(choice.value);
                dialog.current?.close();
              }}
            >
              <span>
                {choice.label}
                {choice.detail && <small>{choice.detail}</small>}
              </span>
              <Icon name="check" />
            </button>
          ))}
          {!visible.length && (
            <p className="profile-note">没有找到匹配的时区。</p>
          )}
        </div>
      </dialog>
    </>
  );
}
