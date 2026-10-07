import type { AudioCue } from "@brioche/contracts/AudioCue";
import type { Block } from "@brioche/contracts/Block";
import type { PublicLesson } from "@brioche/contracts/PublicLesson";
import type { KnowledgeRecording } from "@brioche/contracts/KnowledgeRecording";

export type TimelineCue = {
  startMs: number;
  endMs: number;
  id: string;
  wordId?: string;
};
export type RecordingClip = {
  url: string;
  startMs: number;
  endMs: number;
  cues: TimelineCue[];
};
export type SpeechUnit = {
  id: string;
  text: string;
  locale?: string;
  recording?: RecordingClip;
};
export function knowledgeUnit(
  id: string,
  text: string,
  recording?: KnowledgeRecording | null,
): SpeechUnit {
  const unit: SpeechUnit = { id, text };
  if (!recording) return unit;
  const { asset, startMs, endMs } = recording;
  const extension =
    asset.mimeType === "audio/wav"
      ? "wav"
      : asset.mimeType === "audio/mpeg"
        ? "mp3"
        : null;
  if (
    !extension ||
    !/^[a-f0-9]{64}$/.test(asset.sha256) ||
    !/^\/api\/(?:audio\/|v1\/operator\/lessons\/[a-zA-Z0-9_-]+\/revisions\/[1-9][0-9]*\/audio\/)[a-f0-9]{64}\.(?:mp3|wav)$/.test(
      asset.url,
    ) ||
    !asset.url.endsWith(`/${asset.sha256}.${extension}`) ||
    !Number.isInteger(startMs) ||
    !Number.isInteger(endMs) ||
    !Number.isInteger(asset.durationMs) ||
    startMs < 0 ||
    startMs >= endMs ||
    endMs > asset.durationMs
  )
    return unit;
  unit.recording = {
    url: asset.url,
    startMs,
    endMs,
    cues: [{ id, startMs, endMs }],
  };
  return unit;
}
export const readingScope = (lesson: PublicLesson, blockId: string) =>
  `${lesson.id}:${lesson.revision}:${blockId}:`;
export const wordId = (
  scope: string,
  entryId: string,
  segmentId: string,
  start: number,
  end: number,
) => `${scope}${entryId}:word:${segmentId}:${start}:${end}`;

function assetTrack(lesson: PublicLesson, blockId: string) {
  const track = lesson.audioTracks?.find((track) => track.blockId === blockId);
  const asset = lesson.audio?.find((asset) => asset.assetId === track?.assetId);
  // Public and operator URLs are both same-origin API paths. Never fetch author-provided origins.
  if (
    !track ||
    !asset ||
    !/^\/api\/(?:audio\/|v1\/operator\/lessons\/[a-zA-Z0-9_-]+\/revisions\/[1-9][0-9]*\/audio\/)[0-9a-f]{64}\.(?:mp3|wav)$/.test(
      asset.url,
    )
  )
    return null;
  return { track, asset };
}
function timeline(
  lesson: PublicLesson,
  blockId: string,
  cue: AudioCue,
): TimelineCue {
  const scope = readingScope(lesson, blockId);
  return {
    startMs: cue.startMs,
    endMs: cue.endMs,
    id: scope + cue.entryId,
    ...(cue.wordRange && cue.segmentId
      ? {
          wordId: wordId(
            scope,
            cue.entryId,
            cue.segmentId,
            cue.wordRange.start,
            cue.wordRange.end,
          ),
        }
      : {}),
  };
}
export function readingUnits(
  lesson: PublicLesson,
  block: Extract<Block, { type: "dialogue" | "article" }>,
): SpeechUnit[] {
  const entries = block.type === "dialogue" ? block.turns : block.paragraphs;
  const media = assetTrack(lesson, block.id);
  return entries.map((entry) => {
    const cue = media?.track.cues.find(
      (cue) => cue.entryId === entry.id && !cue.segmentId,
    );
    return {
      id: readingScope(lesson, block.id) + entry.id,
      text: entry.segments.map((segment) => segment.text).join(""),
      ...(cue && media
        ? {
            recording: {
              url: media.asset.url,
              startMs: cue.startMs,
              endMs: cue.endMs,
              cues: media.track.cues
                .filter(
                  (cue) =>
                    cue.entryId === entry.id &&
                    (!cue.segmentId || cue.wordRange),
                )
                .map((cue) => timeline(lesson, block.id, cue)),
            },
          }
        : {}),
    };
  });
}
export function wordUnit(
  lesson: PublicLesson,
  blockId: string,
  entryId: string,
  segmentId: string,
  text: string,
  segmentText: string,
  utf16Start: number,
): SpeechUnit {
  const start = Array.from(segmentText.slice(0, utf16Start)).length;
  const end = start + Array.from(text).length;
  const id = wordId(
    readingScope(lesson, blockId),
    entryId,
    segmentId,
    start,
    end,
  );
  const media = assetTrack(lesson, blockId);
  const cue = media?.track.cues.find(
    (cue) =>
      cue.entryId === entryId &&
      cue.segmentId === segmentId &&
      cue.wordRange?.start === start &&
      cue.wordRange.end === end,
  );
  return {
    id,
    text,
    ...(cue && media
      ? {
          recording: {
            url: media.asset.url,
            startMs: cue.startMs,
            endMs: cue.endMs,
            cues: [{ startMs: cue.startMs, endMs: cue.endMs, id, wordId: id }],
          },
        }
      : {}),
  };
}
export function continuousRecording(units: SpeechUnit[]): RecordingClip | null {
  const first = units[0]?.recording;
  if (
    !first ||
    units.some(
      (unit, i) =>
        !unit.recording ||
        unit.recording.url !== first.url ||
        (i > 0 && unit.recording.startMs < units[i - 1].recording!.endMs),
    )
  )
    return null;
  return {
    url: first.url,
    startMs: first.startMs,
    endMs: units.at(-1)!.recording!.endMs,
    cues: units.flatMap((unit) => unit.recording!.cues),
  };
}

type Media = Pick<
  HTMLAudioElement,
  | "src"
  | "preload"
  | "currentTime"
  | "duration"
  | "readyState"
  | "paused"
  | "playbackRate"
  | "preservesPitch"
  | "play"
  | "pause"
  | "load"
  | "addEventListener"
  | "removeEventListener"
  | "removeAttribute"
>;
type Callbacks = {
  status: (status: "loading" | "playing" | "paused") => void;
  progress: (fraction: number, id: string | null, word: string | null) => void;
  end: () => void;
  error: (blocked: boolean) => void;
};
type Clock = {
  request: (cb: FrameRequestCallback) => number;
  cancel: (id: number) => void;
};
/** One reusable media element. Generations invalidate listeners, clocks and play promises. */
export class RecordingPlayer {
  private media: Media | null = null;
  private generation = 0;
  private playAttempt = 0;
  private detach = () => {};
  private frame: number | null = null;
  private paused = false;
  private active = false;
  private resumePlay: (() => void) | null = null;
  private boundary: ReturnType<typeof setTimeout> | null = null;
  private refreshBoundary: (() => void) | null = null;
  private factory: () => Media;
  private clock: Clock;
  constructor(
    factory: () => Media = () => new Audio(),
    clock: Clock = {
      request: (cb: FrameRequestCallback) => requestAnimationFrame(cb),
      cancel: (id: number) => cancelAnimationFrame(id),
    },
  ) {
    this.factory = factory;
    this.clock = clock;
  }
  private clearBoundary() {
    if (this.boundary !== null) clearTimeout(this.boundary);
    this.boundary = null;
  }
  get isActive() {
    return this.active;
  }
  stop(clearSource = true) {
    this.playAttempt++;
    this.generation++;
    this.active = false;
    this.resumePlay = null;
    this.refreshBoundary = null;
    this.clearBoundary();
    this.detach();
    this.detach = () => {};
    if (this.frame !== null) this.clock.cancel(this.frame);
    this.frame = null;
    this.media?.pause();
    if (clearSource && this.media) {
      this.media.removeAttribute("src");
      this.media.load();
    }
  }
  play(clip: RecordingClip, rate: number, callbacks: Callbacks) {
    this.stop(false);
    if (
      !Number.isFinite(clip.startMs) ||
      !Number.isFinite(clip.endMs) ||
      clip.startMs < 0 ||
      clip.endMs <= clip.startMs ||
      clip.endMs > 1_800_000
    ) {
      callbacks.error(false);
      return;
    }
    const media = (this.media ??= this.factory());
    const gen = this.generation;
    this.active = true;
    this.paused = false;
    const current = () => this.active && gen === this.generation;
    const finish = () => {
      if (!current()) return;
      this.stop(false);
      callbacks.end();
    };
    const fail = (blocked = false) => {
      if (!current()) return;
      this.stop();
      callbacks.error(blocked);
    };
    const update = () => {
      if (!current() || this.paused) return;
      const ms = media.currentTime * 1000;
      if (ms >= clip.endMs) {
        finish();
        return;
      }
      const matching = clip.cues.filter(
        (cue) => ms >= cue.startMs && ms < cue.endMs,
      );
      const word = matching.find((cue) => cue.wordId);
      callbacks.progress(
        Math.max(
          0,
          Math.min(1, (ms - clip.startMs) / (clip.endMs - clip.startMs)),
        ),
        word?.id ?? matching[0]?.id ?? null,
        word?.wordId ?? null,
      );
    };
    const tick = () => {
      if (!current() || this.paused) return;
      this.frame = null;
      update();
      if (current() && !this.paused) this.frame = this.clock.request(tick);
    };
    const runClock = () => {
      if (this.frame === null && current() && !this.paused)
        this.frame = this.clock.request(tick);
    };
    const scheduleBoundary = () => {
      this.clearBoundary();
      if (!current() || this.paused || media.paused) return;
      this.boundary = setTimeout(
        () => {
          if (!current()) return;
          this.boundary = null;
          update();
          if (current() && !this.paused) scheduleBoundary();
        },
        Math.max(
          10,
          (clip.endMs - media.currentTime * 1000) / media.playbackRate,
        ),
      );
    };
    this.refreshBoundary = scheduleBoundary;
    const seek = () => {
      if (!current()) return;
      if (
        !Number.isFinite(media.duration) ||
        clip.endMs > media.duration * 1000 + 100
      ) {
        fail();
        return;
      }
      try {
        media.currentTime = clip.startMs / 1000;
      } catch {
        fail();
      }
    };
    const onPlaying = () => {
      if (!current()) return;
      if (this.paused) {
        media.pause();
        return;
      }
      callbacks.status("playing");
      runClock();
      scheduleBoundary();
    };
    const onWaiting = () => {
      if (current() && !this.paused) callbacks.status("loading");
    };
    const onEnded = () => {
      if (!current()) return;
      if (media.currentTime * 1000 + 100 < clip.endMs) fail();
      else finish();
    };
    const onError = () => fail();
    const onPause = () => {
      if (!current() || this.paused || !media.paused) return;
      this.playAttempt++;
      this.paused = true;
      this.clearBoundary();
      if (this.frame !== null) this.clock.cancel(this.frame);
      this.frame = null;
      callbacks.status("paused");
    };
    const handlers: [string, () => void][] = [
      ["loadedmetadata", seek],
      ["playing", onPlaying],
      ["waiting", onWaiting],
      ["timeupdate", update],
      ["ended", onEnded],
      ["error", onError],
      ["pause", onPause],
    ];
    handlers.forEach(([name, handler]) =>
      media.addEventListener(name, handler),
    );
    this.detach = () =>
      handlers.forEach(([name, handler]) =>
        media.removeEventListener(name, handler),
      );
    if (media.src !== clip.url && !media.src.endsWith(clip.url)) {
      media.src = clip.url;
      media.preload = "metadata";
      media.load();
    }
    media.playbackRate = rate;
    media.preservesPitch = true;
    if (media.readyState >= 1) seek();
    else {
      try {
        media.currentTime = clip.startMs / 1000;
      } catch {
        /* Seek once metadata arrives. */
      }
    }
    const start = () => {
      if (!current()) return;
      const attempt = ++this.playAttempt;
      const latestAttempt = () => current() && attempt === this.playAttempt;
      callbacks.status("loading");
      try {
        void media
          .play()
          .then(() => {
            if (!latestAttempt()) return;
            if (this.paused) {
              media.pause();
              return;
            }
            onPlaying();
          })
          .catch((error: unknown) => {
            if (!latestAttempt() || this.paused) return;
            fail(error instanceof Error && error.name === "NotAllowedError");
          });
      } catch {
        fail();
      }
    };
    this.resumePlay = start;
    // Call synchronously inside the initiating click, preserving Safari's user gesture.
    start();
  }
  pause() {
    if (!this.active) return;
    this.playAttempt++;
    this.paused = true;
    this.clearBoundary();
    this.media?.pause();
    if (this.frame !== null) this.clock.cancel(this.frame);
    this.frame = null;
  }
  resume() {
    if (!this.active) return;
    this.paused = false;
    this.resumePlay?.();
  }
  setRate(rate: number) {
    if (this.media) this.media.playbackRate = rate;
    this.refreshBoundary?.();
  }
}
