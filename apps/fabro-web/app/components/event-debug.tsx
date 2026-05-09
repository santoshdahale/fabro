import { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import {
  Listbox,
  ListboxButton,
  ListboxOption,
  ListboxOptions,
} from "@headlessui/react";
import { XMarkIcon } from "@heroicons/react/24/outline";
import {
  CheckIcon,
  ChevronUpDownIcon,
  FunnelIcon,
  MagnifyingGlassIcon,
} from "@heroicons/react/16/solid";
import type { EventEnvelope } from "@qltysh/fabro-api-client";

import { Tooltip } from "./ui";
import { formatAbsoluteTs } from "../lib/format";

export type DebugCategory =
  | "agent"
  | "command"
  | "lifecycle"
  | "human"
  | "system";

export const DEBUG_CATEGORIES: readonly DebugCategory[] = [
  "agent",
  "command",
  "lifecycle",
  "human",
  "system",
] as const;

const PREFIX_TO_CATEGORY: Record<string, DebugCategory> = {
  agent: "agent",
  command: "command",
  run: "lifecycle",
  stage: "lifecycle",
  parallel: "lifecycle",
  subgraph: "lifecycle",
  edge: "lifecycle",
  loop: "lifecycle",
  prompt: "lifecycle",
  interview: "human",
};

const CATEGORY_LABEL: Record<DebugCategory, string> = {
  agent: "Agent",
  command: "Command",
  lifecycle: "Lifecycle",
  human: "Human",
  system: "System",
};

const CATEGORY_TONE: Record<DebugCategory, string> = {
  agent: "bg-teal-500/15 text-teal-500",
  command: "bg-mint/15 text-mint",
  lifecycle: "bg-amber/15 text-amber",
  human: "bg-coral/15 text-coral",
  system: "bg-overlay-strong text-fg-3",
};

const CATEGORY_COLOR: Record<DebugCategory, string> = {
  agent: "var(--color-teal-500)",
  command: "var(--color-mint)",
  lifecycle: "var(--color-amber)",
  human: "var(--color-coral)",
  system: "var(--color-ice-300)",
};

export function debugCategory(eventName: string | null | undefined): DebugCategory {
  if (!eventName) return "system";
  const dot = eventName.indexOf(".");
  const prefix = dot < 0 ? eventName : eventName.slice(0, dot);
  return PREFIX_TO_CATEGORY[prefix] ?? "system";
}

export function debugCategoryLabel(category: DebugCategory): string {
  return CATEGORY_LABEL[category];
}

export function debugCategoryTone(category: DebugCategory): string {
  return CATEGORY_TONE[category];
}

export function debugCategoryColor(category: DebugCategory): string {
  return CATEGORY_COLOR[category];
}

export function formatElapsed(eventTs: string, runStart: string | undefined): string {
  if (!runStart) return "";
  const startMs = Date.parse(runStart);
  const eventMs = Date.parse(eventTs);
  if (Number.isNaN(startMs) || Number.isNaN(eventMs)) return "";
  const delta = Math.max(0, Math.floor((eventMs - startMs) / 1000));
  const hours = Math.floor(delta / 3600);
  const minutes = Math.floor((delta % 3600) / 60);
  const seconds = delta % 60;
  return `${hours}:${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
}

const JSON_TOKEN_RE =
  /"(?:\\.|[^"\\])*"|\b(?:true|false|null)\b|-?\d+(?:\.\d+)?(?:[eE][+\-]?\d+)?/g;

export function highlightJson(text: string): React.ReactNode[] {
  const parts: React.ReactNode[] = [];
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  let key = 0;
  JSON_TOKEN_RE.lastIndex = 0;
  while ((match = JSON_TOKEN_RE.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push(text.slice(lastIndex, match.index));
    }
    const token = match[0];
    let cls: string;
    if (token.startsWith('"')) {
      const after = text.slice(JSON_TOKEN_RE.lastIndex);
      cls = /^\s*:/.test(after) ? "text-teal-300" : "text-mint";
    } else if (token === "true" || token === "false") {
      cls = "text-coral";
    } else if (token === "null") {
      cls = "text-fg-muted";
    } else {
      cls = "text-amber";
    }
    parts.push(
      <span key={key++} className={cls}>
        {token}
      </span>,
    );
    lastIndex = JSON_TOKEN_RE.lastIndex;
  }
  if (lastIndex < text.length) parts.push(text.slice(lastIndex));
  return parts;
}

export function DebugEventRow({
  event,
  runStart,
  selected,
  onSelect,
}: {
  event: EventEnvelope;
  runStart: string | undefined;
  selected: boolean;
  onSelect: () => void;
}) {
  const eventName = event.event ?? "";
  const category = debugCategory(eventName);
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      className={`grid w-full grid-cols-[5rem_1fr_auto] items-center gap-4 px-5 py-2.5 text-left transition-colors hover:bg-overlay focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-teal-500 ${
        selected ? "bg-overlay" : ""
      }`}
    >
      <span
        className={`inline-flex w-fit items-center rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider ${debugCategoryTone(category)}`}
      >
        {debugCategoryLabel(category)}
      </span>
      <span className="min-w-0 truncate font-mono text-xs text-fg-2">
        {eventName}
      </span>
      <Tooltip label={formatAbsoluteTs(event.ts)}>
        <span className="font-mono text-xs tabular-nums text-fg-muted">
          {formatElapsed(event.ts, runStart)}
        </span>
      </Tooltip>
    </button>
  );
}

export function DetailsPanel({
  title,
  isOpen,
  onClose,
  children,
}: {
  title: string;
  isOpen: boolean;
  onClose: () => void;
  children: React.ReactNode;
}) {
  useEffect(() => {
    if (!isOpen) return;
    function handleKey(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [isOpen, onClose]);

  return (
    <div
      className={`relative shrink-0 self-stretch overflow-hidden transition-[width] duration-200 ease-out ${
        isOpen ? "w-[28rem]" : "w-0"
      }`}
      aria-hidden={isOpen ? undefined : true}
    >
      <div className="absolute inset-y-0 right-0 flex w-[28rem] flex-col border-l border-line bg-panel">
        <div className="flex shrink-0 items-center justify-between border-b border-line px-5 py-3">
          <h2 className="text-sm font-medium text-fg">{title}</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close details"
            className="rounded-md p-1 text-fg-muted transition-colors hover:bg-overlay hover:text-fg focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-teal-500"
          >
            <XMarkIcon className="size-5" />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 pt-4 pb-[calc(1rem+var(--fabro-interview-dock-clearance,0px))]">
          {isOpen ? children : null}
        </div>
      </div>
    </div>
  );
}

export function DebugEventDetailsPanel({
  event,
  onClose,
}: {
  event: EventEnvelope | null;
  onClose: () => void;
}) {
  return (
    <DetailsPanel
      title={event?.event ?? ""}
      isOpen={event != null}
      onClose={onClose}
    >
      {event ? <DebugEventDetails event={event} /> : null}
    </DetailsPanel>
  );
}

function DebugEventDetails({ event }: { event: EventEnvelope }) {
  const text = useMemo(() => JSON.stringify(event, null, 2), [event]);
  const tokens = useMemo(() => highlightJson(text), [text]);
  return (
    <pre className="whitespace-pre-wrap rounded-md bg-overlay-strong p-3 font-mono text-xs leading-relaxed text-fg-3">
      {tokens}
    </pre>
  );
}

export function MultiSelectFilter<T extends string>({
  selected,
  options,
  labelOf,
  onChange,
  emptyMeansAll = false,
}: {
  selected: T[];
  options: readonly T[];
  labelOf: (item: T) => string;
  onChange: (next: T[]) => void;
  emptyMeansAll?: boolean;
}) {
  const allSelected = selected.length === options.length;
  const summary = useMemo(() => {
    if (allSelected || (emptyMeansAll && selected.length === 0)) return "All types";
    if (selected.length === 0) return "No types";
    if (selected.length <= 2) {
      return options
        .filter((o) => selected.includes(o))
        .map(labelOf)
        .join(", ");
    }
    return `${selected.length} types`;
  }, [allSelected, emptyMeansAll, selected, options, labelOf]);

  return (
    <Listbox value={selected} onChange={onChange} multiple>
      <ListboxButton className="inline-flex items-center gap-2 rounded-md bg-panel px-2.5 py-1.5 text-xs text-fg-2 outline-1 -outline-offset-1 outline-line-strong transition-colors hover:bg-overlay-strong focus-visible:outline-2 focus-visible:-outline-offset-1 focus-visible:outline-teal-500">
        <FunnelIcon className="size-3.5 text-fg-muted" aria-hidden="true" />
        <span className="tabular-nums">{summary}</span>
        <ChevronUpDownIcon className="size-3.5 text-fg-muted" aria-hidden="true" />
      </ListboxButton>
      <ListboxOptions
        transition
        anchor={{ to: "bottom start", gap: 4 }}
        className="z-20 w-44 rounded-md bg-panel py-1 outline-1 -outline-offset-1 outline-line-strong transition data-closed:scale-95 data-closed:opacity-0 data-enter:duration-100 data-enter:ease-out data-leave:duration-75 data-leave:ease-in"
      >
        {options.map((option) => (
          <ListboxOption
            key={option}
            value={option}
            className="group flex cursor-pointer items-center gap-2.5 px-3 py-1.5 text-xs text-fg-3 data-focus:bg-overlay data-focus:text-fg data-focus:outline-hidden"
          >
            <span className="flex size-3.5 items-center justify-center rounded-sm border border-line-strong bg-panel-alt group-data-selected:border-teal-500 group-data-selected:bg-teal-500">
              <CheckIcon
                className="size-2.5 text-on-primary opacity-0 group-data-selected:opacity-100"
                aria-hidden="true"
              />
            </span>
            <span>{labelOf(option)}</span>
          </ListboxOption>
        ))}
      </ListboxOptions>
    </Listbox>
  );
}

export function EventSearchInput({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="relative w-full max-w-sm min-w-48 flex-1">
      <MagnifyingGlassIcon
        className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-fg-muted"
        aria-hidden="true"
      />
      <input
        type="search"
        name="event-search"
        aria-label="Search events"
        placeholder="Search events"
        autoComplete="off"
        spellCheck={false}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="block w-full rounded-md bg-panel py-1.5 pl-8 pr-2.5 text-xs text-fg outline-1 -outline-offset-1 outline-line-strong placeholder:text-fg-muted focus:outline-2 focus:-outline-offset-1 focus:outline-teal-500 max-sm:text-base/5"
      />
    </div>
  );
}

const STRIP_HEIGHT = 32;
const BAR_NORMAL_HEIGHT = 22;
const BAR_HOVER_HEIGHT = 26;
const BAR_SELECTED_HEIGHT = 28;
const BAR_WIDTH = 4;

function friendlyEventName(eventName: string): string {
  const parts = eventName.split(".");
  if (parts.length <= 1) return eventName;
  return parts.slice(1).join(".");
}

export function DebugDnaStrip({
  events,
  selectedSeq,
  onSelect,
  runStart,
}: {
  events: EventEnvelope[];
  selectedSeq: number | null;
  onSelect: (seq: number) => void;
  runStart: string | undefined;
}) {
  const [hover, setHover] = useState<{
    seq: number;
    rect: DOMRect;
  } | null>(null);

  const range = useMemo(() => {
    if (events.length === 0) return null;
    let min = Number.POSITIVE_INFINITY;
    let max = Number.NEGATIVE_INFINITY;
    for (const event of events) {
      const ms = Date.parse(event.ts);
      if (Number.isNaN(ms)) continue;
      if (ms < min) min = ms;
      if (ms > max) max = ms;
    }
    if (!Number.isFinite(min) || !Number.isFinite(max)) return null;
    const startCandidate = runStart ? Date.parse(runStart) : Number.NaN;
    const start = Number.isFinite(startCandidate)
      ? Math.min(startCandidate, min)
      : min;
    const duration = Math.max(1, max - start);
    return { start, duration };
  }, [events, runStart]);

  if (!range) {
    return (
      <div
        className="rounded-md bg-overlay"
        style={{ height: STRIP_HEIGHT }}
        aria-hidden="true"
      />
    );
  }

  const hoveredEvent =
    hover != null ? events.find((e) => e.seq === hover.seq) ?? null : null;

  return (
    <div
      className="relative rounded-md bg-overlay px-1.5"
      style={{ height: STRIP_HEIGHT }}
    >
      <div className="relative h-full">
        {events.map((event) => {
          const ms = Date.parse(event.ts);
          if (Number.isNaN(ms)) return null;
          const pct = ((ms - range.start) / range.duration) * 100;
          const category = debugCategory(event.event);
          const color = debugCategoryColor(category);
          const isSelected = event.seq === selectedSeq;
          const isHovered = hover?.seq === event.seq;

          let height = BAR_NORMAL_HEIGHT;
          let opacity = 0.78;
          let boxShadow = "none";
          if (isSelected) {
            height = BAR_SELECTED_HEIGHT;
            opacity = 1;
            boxShadow = "0 0 0 1px rgba(255,255,255,0.55)";
          } else if (isHovered) {
            height = BAR_HOVER_HEIGHT;
            opacity = 1;
          }
          const top = (STRIP_HEIGHT - height) / 2;

          return (
            <div
              key={event.seq}
              role="button"
              tabIndex={-1}
              aria-label={`${debugCategoryLabel(category)} · ${event.event}`}
              aria-pressed={isSelected}
              onMouseEnter={(e) =>
                setHover({
                  seq: event.seq,
                  rect: e.currentTarget.getBoundingClientRect(),
                })
              }
              onMouseLeave={() =>
                setHover((cur) => (cur?.seq === event.seq ? null : cur))
              }
              onClick={() => onSelect(event.seq)}
              className="absolute -translate-x-1/2 cursor-pointer rounded-[1.5px] transition-all duration-100 ease-out"
              style={{
                left: `${pct}%`,
                width: BAR_WIDTH,
                height,
                top,
                opacity,
                background: color,
                boxShadow,
              }}
            />
          );
        })}
      </div>
      {hoveredEvent != null && hover != null && (
        <DnaPopover event={hoveredEvent} anchorRect={hover.rect} runStart={runStart} />
      )}
    </div>
  );
}

function DnaPopover({
  event,
  anchorRect,
  runStart,
}: {
  event: EventEnvelope;
  anchorRect: DOMRect;
  runStart: string | undefined;
}) {
  if (typeof document === "undefined") return null;
  const category = debugCategory(event.event);
  const left = anchorRect.left + anchorRect.width / 2;
  const top = anchorRect.top;
  return createPortal(
    <div
      role="tooltip"
      style={{ left, top }}
      className="pointer-events-none fixed z-50 -translate-x-1/2 -translate-y-[calc(100%+8px)] whitespace-nowrap rounded-md bg-panel-alt px-2.5 py-1 text-xs text-fg shadow-lg outline-1 -outline-offset-1 outline-line-strong"
    >
      {`${debugCategoryLabel(category)} · ${friendlyEventName(event.event)} · ${formatElapsed(event.ts, runStart)}`}
    </div>,
    document.body,
  );
}
