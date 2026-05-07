import { useEffect, useMemo, useState } from "react";
import { useParams } from "react-router";
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
import { Marked } from "marked";

import { StageSidebar } from "../components/stage-sidebar";
import type { Stage } from "../components/stage-sidebar";
import { EmptyState } from "../components/state";
import { useRun, useRunStageEvents, useRunStages } from "../lib/queries";
import { STAGE_ACTIVITY_EVENT_TYPES, type StageActivityEventType } from "../lib/run-events";
import { mapRunStagesToSidebarStages } from "../lib/stage-sidebar";
import { getNumber, getString, type UnknownRecord } from "../lib/unknown";
import type { EventEnvelope } from "@qltysh/fabro-api-client";

export const handle = { wide: true, fullHeight: true };

type TurnType =
  | { kind: "system"; ts: string; content: string }
  | { kind: "assistant"; ts: string; content: string; inputTokens: number; outputTokens: number }
  | { kind: "tool"; ts: string; toolName: string; input: string; result: string; isError: boolean; durationMs: number }
  | { kind: "command"; ts: string; script: string; running: boolean; exitCode: number | null; durationMs: number };

const STAGE_ACTIVITY_EVENT_SET = new Set<string>(STAGE_ACTIVITY_EVENT_TYPES);

const EVENT_KINDS = ["system", "assistant", "tool", "command"] as const;
type EventKind = (typeof EVENT_KINDS)[number];

const EVENT_KIND_LABEL: Record<EventKind, string> = {
  system: "System",
  assistant: "Agent",
  tool: "Tool",
  command: "Command",
};

const EVENTS_TABS = ["transcript", "debug"] as const;
type EventsTab = (typeof EVENTS_TABS)[number];

const EVENTS_TAB_LABEL: Record<EventsTab, string> = {
  transcript: "Transcript",
  debug: "Debug",
};

function assertNever(value: never): never {
  throw new Error(`Unhandled stage activity event type: ${value}`);
}

function activityEventStageId(event: EventEnvelope): string | undefined {
  if (typeof event.stage_id === "string") return event.stage_id;
  if (typeof event.node_id === "string") return event.node_id;
  return getString(event.properties ?? {}, "node_id");
}

interface PendingTool {
  ts: string;
  toolName: string;
  input: string;
}

interface PendingCommand {
  ts: string;
  script: string;
}

export function eventsToActivity(events: EventEnvelope[], stageId: string): TurnType[] {
  const turns: TurnType[] = [];
  const pendingTools = new Map<string, PendingTool>();
  let pendingCommand: PendingCommand | undefined;

  for (const e of events) {
    const eventName = e.event;
    if (
      activityEventStageId(e) !== stageId ||
      !eventName ||
      !STAGE_ACTIVITY_EVENT_SET.has(eventName)
    ) {
      continue;
    }
    const eventType = eventName as StageActivityEventType;
    const props: UnknownRecord = e.properties ?? {};
    switch (eventType) {
      case "stage.prompt":
        turns.push({ kind: "system", ts: e.ts, content: getString(props, "text") ?? e.text ?? "" });
        break;
      case "agent.message": {
        const msg = getString(props, "text") ?? e.text ?? "";
        if (msg) {
          const billing = (props.billing ?? {}) as UnknownRecord;
          turns.push({
            kind: "assistant",
            ts: e.ts,
            content: msg,
            inputTokens: getNumber(billing, "input_tokens") ?? 0,
            outputTokens: getNumber(billing, "output_tokens") ?? 0,
          });
        }
        break;
      }
      case "agent.tool.started": {
        const callId = getString(props, "tool_call_id") ?? e.tool_call_id ?? "";
        const args = props.arguments ?? e.arguments;
        pendingTools.set(callId, {
          ts: e.ts,
          toolName: getString(props, "tool_name") ?? e.tool_name ?? "",
          input: typeof args === "string" ? args : JSON.stringify(args ?? ""),
        });
        break;
      }
      case "agent.tool.completed": {
        const callId = getString(props, "tool_call_id") ?? e.tool_call_id ?? "";
        const started = pendingTools.get(callId);
        pendingTools.delete(callId);
        const output = props.output ?? e.output ?? "";
        const result = typeof output === "string" ? output : JSON.stringify(output, null, 2);
        turns.push({
          kind: "tool",
          ts: started?.ts ?? e.ts,
          toolName: started?.toolName ?? getString(props, "tool_name") ?? e.tool_name ?? "",
          input: started?.input ?? "",
          result,
          isError: (props.is_error ?? e.is_error) === true,
          durationMs: durationBetween(started?.ts, e.ts),
        });
        break;
      }
      case "command.started": {
        pendingCommand = {
          ts: e.ts,
          script: getString(props, "script") ?? "",
        };
        break;
      }
      case "command.completed": {
        turns.push({
          kind: "command",
          ts: pendingCommand?.ts ?? e.ts,
          script: pendingCommand?.script ?? "",
          running: false,
          exitCode: getNumber(props, "exit_code") ?? null,
          durationMs: getNumber(props, "duration_ms") ?? 0,
        });
        pendingCommand = undefined;
        break;
      }
      default:
        assertNever(eventType);
    }
  }

  if (pendingCommand) {
    turns.push({
      kind: "command",
      ts: pendingCommand.ts,
      script: pendingCommand.script,
      running: true,
      exitCode: null,
      durationMs: 0,
    });
  }

  return turns;
}

function turnLabel(turn: TurnType): string {
  switch (turn.kind) {
    case "system":
      return "System";
    case "assistant":
      return "Agent";
    case "tool":
      return "Tool";
    case "command":
      return "Command";
  }
}

function turnTone(turn: TurnType): string {
  if (turn.kind === "tool" && turn.isError) {
    return "bg-coral/15 text-coral";
  }
  switch (turn.kind) {
    case "system":
      return "bg-amber/15 text-amber";
    case "assistant":
      return "bg-teal-500/15 text-teal-500";
    case "tool":
    case "command":
      return "bg-mint/15 text-mint";
  }
}

const SUMMARY_MAX_CHARS = 80;

function oneLine(text: string): string {
  const collapsed = text.replace(/\s+/g, " ").trim();
  if (collapsed.length <= SUMMARY_MAX_CHARS) return collapsed;
  return `${collapsed.slice(0, SUMMARY_MAX_CHARS - 1)}…`;
}

const SAFE_HTTP_URL_RE = /^https?:\/\//i;
const SAFE_MAILTO_URL_RE = /^mailto:/i;

function isSafeMarkdownHref(href: string): boolean {
  return (
    SAFE_HTTP_URL_RE.test(href) ||
    SAFE_MAILTO_URL_RE.test(href) ||
    href.startsWith("#") ||
    (href.startsWith("/") && !href.startsWith("//"))
  );
}

const markedSafe = new Marked();
markedSafe.use({
  async: false,
  walkTokens(token) {
    if (
      (token.type === "link" || token.type === "image") &&
      typeof token.href === "string" &&
      !isSafeMarkdownHref(token.href)
    ) {
      token.href = "";
    }
  },
  renderer: {
    html() {
      return "";
    },
  },
});

function Markdown({ content }: { content: string }) {
  const html = useMemo(
    () => markedSafe.parse(content, { async: false }) as string,
    [content],
  );
  return (
    <div
      className="prose prose-sm max-w-none text-fg-3 prose-headings:text-fg-2 prose-strong:text-fg-2 prose-code:rounded prose-code:bg-overlay-strong prose-code:px-1 prose-code:py-0.5 prose-code:text-[0.8em] prose-code:font-mono prose-code:text-fg-3 prose-code:before:content-none prose-code:after:content-none prose-pre:bg-overlay-strong prose-pre:text-fg-3 prose-a:text-teal-500"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

const TOOL_NAME_DISPLAY: Record<string, string> = {
  read_file: "Read",
  write_file: "Write",
  edit_file: "Edit",
  shell: "Bash",
  grep: "Grep",
  glob: "Glob",
  read_many_files: "Read Many",
  list_dir: "List Dir",
  web_search: "Web Search",
  web_fetch: "Web Fetch",
};

export function humanizeToolName(raw: string): string {
  if (!raw) return "tool";
  if (TOOL_NAME_DISPLAY[raw]) return TOOL_NAME_DISPLAY[raw];
  // MCP tools are namespaced like `mcp__<server>__<tool>`; display the trailing segment.
  const lastSegment = raw.split("__").pop() ?? raw;
  return lastSegment
    .split(/[_-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

export function turnSummary(turn: TurnType): string {
  switch (turn.kind) {
    case "system":
    case "assistant":
      return oneLine(turn.content);
    case "tool":
      return humanizeToolName(turn.toolName);
    case "command":
      return oneLine(turn.script) || (turn.running ? "running…" : "");
  }
}

function durationBetween(startTs: string | undefined, endTs: string): number {
  if (!startTs) return 0;
  const startMs = Date.parse(startTs);
  const endMs = Date.parse(endTs);
  if (Number.isNaN(startMs) || Number.isNaN(endMs)) return 0;
  return Math.max(0, endMs - startMs);
}

function formatDurationMs(ms: number): string {
  return `${(ms / 1000).toFixed(1)}s`;
}

function formatTokenCount(n: number): string {
  if (n < 1000) return `${n}`;
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

export function turnMetric(turn: TurnType): string | null {
  switch (turn.kind) {
    case "assistant": {
      if (turn.inputTokens === 0 && turn.outputTokens === 0) return null;
      return `${formatTokenCount(turn.inputTokens)} / ${formatTokenCount(turn.outputTokens)}`;
    }
    case "tool":
    case "command":
      return turn.durationMs > 0 ? formatDurationMs(turn.durationMs) : null;
    case "system":
      return null;
  }
}

export function searchableText(turn: TurnType): string {
  switch (turn.kind) {
    case "system":
    case "assistant":
      return turn.content;
    case "tool":
      return `${humanizeToolName(turn.toolName)} ${turn.toolName} ${turn.input} ${turn.result}`;
    case "command":
      return turn.script;
  }
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

function EventRow({
  turn,
  runStart,
  selected,
  onSelect,
}: {
  turn: TurnType;
  runStart: string | undefined;
  selected: boolean;
  onSelect: () => void;
}) {
  const metric = turnMetric(turn);
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      className={`grid w-full grid-cols-[5rem_1fr_auto_auto] items-center gap-4 px-5 py-2.5 text-left transition-colors hover:bg-overlay focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-teal-500 ${
        selected ? "bg-overlay" : ""
      }`}
    >
      <span
        className={`inline-flex w-fit items-center rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider ${turnTone(turn)}`}
      >
        {turnLabel(turn)}
      </span>
      <span className="min-w-0 truncate text-sm text-fg-3">
        {turnSummary(turn)}
      </span>
      <span className="text-right font-mono text-xs tabular-nums text-fg-muted">
        {metric ?? ""}
      </span>
      <span className="font-mono text-xs tabular-nums text-fg-muted">
        {formatElapsed(turn.ts, runStart)}
      </span>
    </button>
  );
}

function DetailField({
  label,
  children,
  mono = false,
}: {
  label: string;
  children: React.ReactNode;
  mono?: boolean;
}) {
  return (
    <div>
      <div className="mb-1 text-xs font-medium uppercase tracking-wider text-fg-muted">
        {label}
      </div>
      <div className={mono ? "font-mono text-sm text-fg-3" : "text-sm text-fg-3"}>
        {children}
      </div>
    </div>
  );
}

function CodeBlock({ children }: { children: string }) {
  return (
    <pre className="max-h-96 overflow-auto whitespace-pre-wrap rounded-md bg-overlay-strong p-3 font-mono text-xs leading-relaxed text-fg-3">
      {children || <span className="text-fg-muted">empty</span>}
    </pre>
  );
}

function prettyJson(raw: string): { text: string; isJson: boolean } {
  if (!raw || !raw.trim()) return { text: "", isJson: false };
  try {
    return { text: JSON.stringify(JSON.parse(raw), null, 2), isJson: true };
  } catch {
    return { text: raw, isJson: false };
  }
}

const JSON_TOKEN_RE =
  /"(?:\\.|[^"\\])*"|\b(?:true|false|null)\b|-?\d+(?:\.\d+)?(?:[eE][+\-]?\d+)?/g;

function highlightJson(text: string): React.ReactNode[] {
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

function JsonBlock({ value }: { value: string }) {
  const pretty = useMemo(() => prettyJson(value), [value]);
  const tokens = useMemo(
    () => (pretty.isJson ? highlightJson(pretty.text) : null),
    [pretty.isJson, pretty.text],
  );
  return (
    <pre className="max-h-96 overflow-auto whitespace-pre-wrap rounded-md bg-overlay-strong p-3 font-mono text-xs leading-relaxed text-fg-3">
      {!pretty.text ? (
        <span className="text-fg-muted">empty</span>
      ) : (
        tokens ?? pretty.text
      )}
    </pre>
  );
}

function EventDetails({ turn, runStart }: { turn: TurnType; runStart: string | undefined }) {
  const elapsed = formatElapsed(turn.ts, runStart);
  const absolute = (() => {
    const ms = Date.parse(turn.ts);
    if (Number.isNaN(ms)) return turn.ts;
    return new Date(ms).toLocaleString();
  })();

  return (
    <div className="space-y-5">
      <DetailField label="When" mono>
        {elapsed ? `${elapsed} · ${absolute}` : absolute}
      </DetailField>

      {(turn.kind === "system" || turn.kind === "assistant") && (
        <DetailField label="Content">
          <Markdown content={turn.content} />
        </DetailField>
      )}

      {turn.kind === "tool" && (
        <>
          <DetailField label="Tool" mono>
            {humanizeToolName(turn.toolName)}{" "}
            <span className="text-fg-muted">({turn.toolName})</span>
          </DetailField>
          <DetailField label="Input">
            <JsonBlock value={turn.input} />
          </DetailField>
          <DetailField label={turn.isError ? "Error" : "Result"}>
            <JsonBlock value={turn.result} />
          </DetailField>
        </>
      )}

      {turn.kind === "command" && (
        <>
          <DetailField label="Status" mono>
            {turn.running
              ? "Running…"
              : `exit ${turn.exitCode ?? "?"}${
                  turn.durationMs ? ` · ${formatDurationMs(turn.durationMs)}` : ""
                }`}
          </DetailField>
          <DetailField label="Script">
            <CodeBlock>{turn.script}</CodeBlock>
          </DetailField>
        </>
      )}
    </div>
  );
}

function EventDetailsPanel({
  turn,
  runStart,
  onClose,
}: {
  turn: TurnType | null;
  runStart: string | undefined;
  onClose: () => void;
}) {
  useEffect(() => {
    if (!turn) return;
    function handleKey(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [turn, onClose]);

  return (
    <div
      className={`relative shrink-0 self-stretch overflow-hidden transition-[width] duration-200 ease-out ${
        turn ? "w-[28rem]" : "w-0"
      }`}
      aria-hidden={turn ? undefined : true}
    >
      <div className="absolute inset-y-0 right-0 flex w-[28rem] flex-col border-l border-line bg-panel">
        <div className="flex shrink-0 items-center justify-between border-b border-line px-5 py-3">
          <h2 className="text-sm font-medium text-fg">
            {turn ? `${turnLabel(turn)} event` : ""}
          </h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close details"
            className="rounded-md p-1 text-fg-muted transition-colors hover:bg-overlay hover:text-fg focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-teal-500"
          >
            <XMarkIcon className="size-5" />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          {turn ? <EventDetails turn={turn} runStart={runStart} /> : null}
        </div>
      </div>
    </div>
  );
}

function EventsTabToggle({
  tab,
  onTabChange,
}: {
  tab: EventsTab;
  onTabChange: (tab: EventsTab) => void;
}) {
  return (
    <div
      role="group"
      aria-label="View"
      className="inline-flex rounded-md bg-panel p-0.5 outline-1 -outline-offset-1 outline-line-strong"
    >
      {EVENTS_TABS.map((value) => {
        const active = tab === value;
        return (
          <button
            key={value}
            type="button"
            onClick={() => onTabChange(value)}
            aria-pressed={active}
            className={`rounded px-2.5 py-1 text-xs font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-teal-500 ${
              active
                ? "bg-overlay-strong text-fg"
                : "text-fg-muted hover:text-fg-2"
            }`}
          >
            {EVENTS_TAB_LABEL[value]}
          </button>
        );
      })}
    </div>
  );
}

function KindFilter({
  selected,
  onChange,
}: {
  selected: EventKind[];
  onChange: (kinds: EventKind[]) => void;
}) {
  const summary = useMemo(() => {
    if (selected.length === EVENT_KINDS.length) return "All types";
    if (selected.length === 0) return "No types";
    if (selected.length <= 2) {
      return EVENT_KINDS.filter((k) => selected.includes(k))
        .map((k) => EVENT_KIND_LABEL[k])
        .join(", ");
    }
    return `${selected.length} types`;
  }, [selected]);

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
        {EVENT_KINDS.map((kind) => (
          <ListboxOption
            key={kind}
            value={kind}
            className="group flex cursor-pointer items-center gap-2.5 px-3 py-1.5 text-xs text-fg-3 data-focus:bg-overlay data-focus:text-fg data-focus:outline-hidden"
          >
            <span className="flex size-3.5 items-center justify-center rounded-sm border border-line-strong bg-panel-alt group-data-selected:border-teal-500 group-data-selected:bg-teal-500">
              <CheckIcon
                className="size-2.5 text-on-primary opacity-0 group-data-selected:opacity-100"
                aria-hidden="true"
              />
            </span>
            <span>{EVENT_KIND_LABEL[kind]}</span>
          </ListboxOption>
        ))}
      </ListboxOptions>
    </Listbox>
  );
}

function SearchInput({
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

function EventsToolbar({
  tab,
  onTabChange,
  selectedKinds,
  onKindsChange,
  search,
  onSearchChange,
  filteredCount,
  totalCount,
}: {
  tab: EventsTab;
  onTabChange: (tab: EventsTab) => void;
  selectedKinds: EventKind[];
  onKindsChange: (kinds: EventKind[]) => void;
  search: string;
  onSearchChange: (value: string) => void;
  filteredCount: number;
  totalCount: number;
}) {
  const allKindsSelected = selectedKinds.length === EVENT_KINDS.length;
  const isFiltering = !allKindsSelected || search.length > 0;
  const showTranscriptControls = tab === "transcript";

  function clearFilters() {
    onKindsChange([...EVENT_KINDS]);
    onSearchChange("");
  }

  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-2 pb-3">
      <EventsTabToggle tab={tab} onTabChange={onTabChange} />
      {showTranscriptControls ? (
        <div className="flex flex-1 flex-wrap items-center gap-2">
          <KindFilter selected={selectedKinds} onChange={onKindsChange} />
          <SearchInput value={search} onChange={onSearchChange} />
          {isFiltering && (
            <button
              type="button"
              onClick={clearFilters}
              className="rounded px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg-2 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-teal-500"
            >
              Clear
            </button>
          )}
        </div>
      ) : (
        <div className="flex-1" />
      )}
      {showTranscriptControls && isFiltering && totalCount > 0 && (
        <span className="text-xs tabular-nums text-fg-muted">
          {filteredCount.toLocaleString()} of {totalCount.toLocaleString()} events
        </span>
      )}
    </div>
  );
}

export default function RunStages() {
  const { id, stageId } = useParams();
  const runQuery = useRun(id);
  const stagesQuery = useRunStages(id);
  const stages = useMemo(
    () => mapRunStagesToSidebarStages(stagesQuery.data),
    [stagesQuery.data],
  );

  const selectedStage = stages.find((s: Stage) => s.id === stageId) ?? stages[0];
  const selectedStageId = selectedStage?.id;
  const stageEventsQuery = useRunStageEvents(id, selectedStageId);
  const turns = useMemo(
    () =>
      selectedStageId
        ? eventsToActivity(stageEventsQuery.data ?? [], selectedStageId)
        : [],
    [stageEventsQuery.data, selectedStageId],
  );

  const [openIndex, setOpenIndex] = useState<number | null>(null);
  useEffect(() => {
    setOpenIndex(null);
  }, [selectedStageId]);
  const openTurn = openIndex != null ? turns[openIndex] ?? null : null;

  const [tab, setTab] = useState<EventsTab>("transcript");
  const [selectedKinds, setSelectedKinds] = useState<EventKind[]>([
    ...EVENT_KINDS,
  ]);
  const [search, setSearch] = useState("");
  const filteredTurns = useMemo<{ turn: TurnType; index: number }[]>(() => {
    const kindSet = new Set(selectedKinds);
    const needle = search.toLowerCase();
    const out: { turn: TurnType; index: number }[] = [];
    turns.forEach((turn, i) => {
      if (!kindSet.has(turn.kind)) return;
      if (needle && !searchableText(turn).toLowerCase().includes(needle)) return;
      out.push({ turn, index: i });
    });
    return out;
  }, [turns, selectedKinds, search]);

  if (!id || !stages.length) {
    return (
      <div className="py-12">
        <EmptyState
          title="No stages yet"
          description="Stages will appear here once the run begins executing."
        />
      </div>
    );
  }

  const runStart = runQuery.data?.created_at;

  return (
    <div className="-mr-4 -mt-6 flex min-h-0 flex-1 sm:-mr-6 lg:-mr-8">
      <div className="shrink-0 pb-6 pr-3 pt-6">
        <StageSidebar stages={stages} runId={id} selectedStageId={selectedStage.id} />
      </div>

      <div className="relative w-px shrink-0">
        <div
          aria-hidden="true"
          className="absolute inset-x-0 top-0 -bottom-6 bg-line"
        />
      </div>

      <div className="flex min-h-0 min-w-0 flex-1 flex-col pt-3">
        <div className="shrink-0 border-b border-line">
          <div className="pl-3 pr-4 sm:pr-6 lg:pr-8">
            <EventsToolbar
              tab={tab}
              onTabChange={setTab}
              selectedKinds={selectedKinds}
              onKindsChange={setSelectedKinds}
              search={search}
              onSearchChange={setSearch}
              filteredCount={filteredTurns.length}
              totalCount={turns.length}
            />
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto pb-6 pt-2">
          {tab === "transcript" ? (
            turns.length > 0 && filteredTurns.length === 0 ? (
              <div className="px-2 py-6 text-sm text-fg-muted">
                No events match these filters.
              </div>
            ) : (
              filteredTurns.map(({ turn, index }) => (
                <EventRow
                  key={`turn-${index}`}
                  turn={turn}
                  runStart={runStart}
                  selected={openIndex === index}
                  onSelect={() => setOpenIndex(index)}
                />
              ))
            )
          ) : null}
        </div>
      </div>

      <EventDetailsPanel
        turn={tab === "transcript" ? openTurn : null}
        runStart={runStart}
        onClose={() => setOpenIndex(null)}
      />
    </div>
  );
}
