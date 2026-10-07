import { useEffect, useRef, useState } from "react";
import type { AdminRecordings } from "@brioche/contracts/AdminRecordings";
import type { AudioAsset } from "@brioche/contracts/AudioAsset";
import type { CharacterVoiceReference } from "@brioche/contracts/CharacterVoiceReference";
import { RecordingPlayer } from "../lib/recording-playback";
import { useLearning } from "./learning";
import { Icon } from "./icon";

export function ReferenceRecordingPicker({
  value,
  onChange,
  pending,
}: {
  value: CharacterVoiceReference;
  onChange: (value: CharacterVoiceReference) => void;
  pending: boolean;
}) {
  const [query, setQuery] = useState(""),
    [appliedQuery, setAppliedQuery] = useState(""),
    [result, setResult] = useState<AdminRecordings>({ items: [], next: null });
  const [loading, setLoading] = useState(false),
    [error, setError] = useState("");
  const [active, setActive] = useState<string | null>(null),
    [status, setStatus] = useState("idle");
  const read = useRef<AbortController | null>(null),
    player = useRef<RecordingPlayer | null>(null);
  const { toast, stop } = useLearning();
  async function load(q: string, next?: AdminRecordings["next"]) {
    read.current?.abort();
    player.current?.stop();
    setActive(null);
    setStatus("idle");
    const controller = new AbortController();
    read.current = controller;
    setLoading(true);
    setError("");
    const params = new URLSearchParams({ q });
    if (next) {
      params.set("afterId", next.assetId);
      params.set("afterRevision", String(next.revision));
    }
    try {
      const response = await fetch(`/api/v1/operator/recordings?${params}`, {
        cache: "no-store",
        signal: AbortSignal.any([
          controller.signal,
          AbortSignal.timeout(10000),
        ]),
      });
      if (!response.ok)
        throw Error("无法读取参考录音，请核对管理员登录状态后重试。");
      const data: AdminRecordings = await response.json();
      controller.signal.throwIfAborted();
      setResult(data);
      setAppliedQuery(q);
    } catch (e) {
      if (!controller.signal.aborted)
        setError(e instanceof Error ? e.message : "录音读取失败，请重试。");
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  }
  useEffect(() => {
    player.current = new RecordingPlayer();
    void load("");
    const release = () => {
      player.current?.stop();
      setActive(null);
      setStatus("idle");
    };
    window.addEventListener("pagehide", release);
    return () => {
      read.current?.abort();
      player.current?.stop();
      window.removeEventListener("pagehide", release);
    };
  }, []);
  useEffect(() => {
    if (pending) {
      player.current?.stop();
      setActive(null);
      setStatus("idle");
    }
  }, [pending]);
  function listen(asset: AudioAsset) {
    if (pending) return;
    const id = `${asset.assetId}:${asset.revision}`;
    if (active === id && player.current?.isActive) {
      if (status === "paused") player.current.resume();
      else {
        player.current.pause();
        setStatus("paused");
      }
      return;
    }
    stop();
    setActive(id);
    player.current?.play(
      { url: asset.url, startMs: 0, endMs: asset.durationMs, cues: [] },
      1,
      {
        status: setStatus,
        progress: () => {},
        end: () => {
          setActive(null);
          setStatus("idle");
        },
        error: () => {
          setActive(null);
          setStatus("idle");
          toast("参考录音无法播放，请刷新核对文件与登录状态。");
        },
      },
    );
  }
  return (
    <div className="reference-recording-picker">
      <div className="avatar-search">
        <label>
          搜索已登记录音
          <input
            type="search"
            maxLength={200}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                if (!pending && !loading) void load(query);
              }
            }}
          />
        </label>
        <button
          type="button"
          className="secondary"
          aria-disabled={pending || loading}
          onClick={() => {
            if (!pending && !loading) void load(query);
          }}
        >
          查找
        </button>
      </div>
      {loading && <p role="status">正在读取录音…</p>}
      {error && <p role="alert">{error}</p>}
      <div role="group" aria-label="选择参考录音">
        {result.items
          .filter((i) => i.asset.durationMs <= 30000)
          .map(({ asset, source, license }) => {
            const selected =
              value.assetId === asset.assetId &&
              value.revision === asset.revision;
            return (
              <div
                className="reference-recording-option"
                key={`${asset.assetId}:${asset.revision}`}
              >
                <button
                  type="button"
                  aria-pressed={selected}
                  aria-disabled={pending}
                  onClick={() => {
                    if (pending) return;
                    player.current?.stop();
                    setActive(null);
                    setStatus("idle");
                    onChange(
                      selected
                        ? value
                        : {
                            assetId: asset.assetId,
                            revision: asset.revision,
                            transcript: "",
                            cloningPermission: "",
                          },
                    );
                  }}
                >
                  <strong>
                    {asset.assetId} · v{asset.revision}
                  </strong>
                  <span>
                    {(asset.durationMs / 1000).toFixed(2)} 秒 · {asset.creditZh}
                  </span>
                  <span>
                    {source} · {license}
                  </span>
                </button>
                <button
                  type="button"
                  className="icon-button"
                  aria-label={`${active === `${asset.assetId}:${asset.revision}` && status === "playing" ? "暂停" : "试听"}参考录音 ${asset.assetId} 版本 ${asset.revision}`}
                  aria-disabled={pending}
                  onClick={() => listen(asset)}
                >
                  <Icon
                    name={
                      active === `${asset.assetId}:${asset.revision}` &&
                      status === "playing"
                        ? "pause"
                        : "play"
                    }
                  />
                </button>
              </div>
            );
          })}
      </div>
      {!loading && !result.items.some((i) => i.asset.durationMs <= 30000) && (
        <p>这一页没有30秒以内的参考录音。</p>
      )}
      {result.next && (
        <button
          type="button"
          className="text-button"
          aria-disabled={pending || loading}
          onClick={() => {
            if (!pending && !loading) void load(appliedQuery, result.next);
          }}
        >
          下一页录音
        </button>
      )}
      <p>
        已选：
        {value.assetId ? `${value.assetId} · v${value.revision}` : "尚未选择"}
      </p>
    </div>
  );
}
