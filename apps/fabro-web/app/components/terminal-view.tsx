import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { Terminal as XtermTerminal } from "@xterm/xterm";
import type { FitAddon as XtermFitAddon } from "@xterm/addon-fit";
import {
  ArrowPathIcon,
  ClipboardDocumentIcon,
} from "@heroicons/react/20/solid";

import { SECONDARY_BUTTON_CLASS, Tooltip } from "./ui";
import { ErrorState } from "./state";
import { useToast } from "./toast";
import { apiData, humanInTheLoopApi } from "../lib/api-client";
import { useRunState } from "../lib/queries";

const ICON_BUTTON_CLASS =
  "inline-flex size-9 items-center justify-center rounded-lg text-fg-2 outline-1 -outline-offset-1 outline-white/10 transition-colors hover:bg-overlay hover:text-fg focus-visible:outline-2 focus-visible:-outline-offset-1 focus-visible:outline-teal-500";

export const TERMINAL_DOCK_CLEARANCE_CLASS =
  "pb-[calc(0.125rem+var(--fabro-interview-dock-clearance,0px))]";

type ConnectionStatus = "connecting" | "ready" | "closed" | "error";

interface TerminalServerMessage {
  type: "ready" | "error" | "closed";
  message?: string;
}

const TERMINAL_BACKGROUND = "#05080F";

// Pin the cell to a whole-pixel height so xterm's fit math stays exact.
// fontSize × lineHeight = 13 × (19/13) = 19px → no sub-pixel rounding,
// no bottom-row clipping.
const TERMINAL_FONT_SIZE = 13;
const TERMINAL_CELL_HEIGHT_PX = 19;
const TERMINAL_LINE_HEIGHT = TERMINAL_CELL_HEIGHT_PX / TERMINAL_FONT_SIZE;

const TERMINAL_THEME = {
  background:          TERMINAL_BACKGROUND,
  foreground:          "#E6EDF3",
  cursor:              "#7AC4E5",
  cursorAccent:        "#05080F",
  selectionBackground: "#1F4F73",

  black:   "#05080F",
  red:     "#FF6B6B",
  green:   "#5EE6A8",
  yellow:  "#FFC857",
  blue:    "#82AAFF",
  magenta: "#C792EA",
  cyan:    "#7AC4E5",
  white:   "#D5DCE3",

  brightBlack:   "#4B5563",
  brightRed:     "#FF8B8B",
  brightGreen:   "#85F5C2",
  brightYellow:  "#FFD98A",
  brightBlue:    "#A4C4FF",
  brightMagenta: "#E0B6FF",
  brightCyan:    "#A8DFF5",
  brightWhite:   "#FFFFFF",
};

export function buildTerminalWebSocketUrl(location: Location, runId: string): string {
  const protocol = location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${location.host}/api/v1/runs/${encodeURIComponent(runId)}/terminal`;
}

export function parseTerminalServerMessage(data: string): TerminalServerMessage | null {
  try {
    const parsed = JSON.parse(data);
    if (!parsed || typeof parsed !== "object") return null;
    const type = (parsed as { type?: unknown }).type;
    if (type !== "ready" && type !== "error" && type !== "closed") return null;
    const message = (parsed as { message?: unknown }).message;
    return {
      type,
      message: typeof message === "string" ? message : undefined,
    };
  } catch {
    return null;
  }
}

export function terminalAccessCommandLabel(provider: string | null): string | null {
  if (provider === "daytona") return "SSH";
  if (provider === "docker") return "Exec";
  return null;
}

function terminalAccessCommandCopiedMessage(provider: string | null): string {
  return provider === "docker" ? "Docker exec command copied." : "SSH command copied.";
}

function terminalAccessCommandErrorMessage(provider: string | null): string {
  return provider === "docker"
    ? "Could not copy Docker exec command."
    : "Could not copy SSH command.";
}

function getObject(value: unknown, key: string): Record<string, unknown> | null {
  if (!value || typeof value !== "object") return null;
  const child = (value as Record<string, unknown>)[key];
  return child && typeof child === "object" ? child as Record<string, unknown> : null;
}

function getString(value: Record<string, unknown> | null, key: string): string | null {
  const child = value?.[key];
  return typeof child === "string" ? child : null;
}

export function sandboxStatusDetail(sandbox: Record<string, unknown> | null): string | null {
  return getString(sandbox, "identifier")
    ?? getString(sandbox, "id")
    ?? getString(sandbox, "provider");
}

function sendResize(socket: WebSocket | null, terminal: XtermTerminal | null) {
  if (!socket || socket.readyState !== WebSocket.OPEN || !terminal) return;
  socket.send(JSON.stringify({
    type: "resize",
    cols: terminal.cols,
    rows: terminal.rows,
  }));
}

function statusDotClasses(status: ConnectionStatus): string {
  switch (status) {
    case "ready":
      return "bg-teal-500";
    case "error":
      return "bg-coral";
    case "closed":
      return "bg-fg-muted";
    case "connecting":
      return "bg-amber animate-pulse";
  }
}

function statusLabel(status: ConnectionStatus): string {
  switch (status) {
    case "ready":
      return "Connected";
    case "error":
      return "Error";
    case "closed":
      return "Closed";
    case "connecting":
      return "Connecting";
  }
}

function StatusPill({
  status,
  detail,
}: {
  status: ConnectionStatus;
  detail: string | null;
}) {
  return (
    <span
      role="status"
      aria-live="polite"
      className="inline-flex items-center gap-2 rounded-full bg-overlay py-1 pr-3 pl-2 text-xs font-medium text-fg-2 outline-1 -outline-offset-1 outline-white/10"
    >
      <span
        className={`size-1.5 rounded-full ${statusDotClasses(status)}`}
        aria-hidden="true"
      />
      <span>{statusLabel(status)}</span>
      {detail ? (
        <>
          <span className="text-fg-muted" aria-hidden="true">·</span>
          <span className="max-w-72 truncate font-mono text-fg-3" title={detail}>
            {detail}
          </span>
        </>
      ) : null}
    </span>
  );
}

export default function TerminalView({
  runId,
  leading,
}: {
  runId: string;
  leading?: React.ReactNode;
}) {
  const { push } = useToast();
  const stateQuery = useRunState(runId);
  const sandbox = getObject(getObject(stateQuery.data, "run"), "sandbox")
    ?? getObject(stateQuery.data, "sandbox");
  const provider = getString(sandbox, "provider");
  const sandboxDetail = sandboxStatusDetail(sandbox);
  const accessCommandLabel = terminalAccessCommandLabel(provider);
  const [connectionKey, setConnectionKey] = useState(0);
  const [status, setStatus] = useState<ConnectionStatus>("connecting");
  const [error, setError] = useState<{ message: string; recoverable: boolean } | null>(null);
  const terminalEl = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<XtermTerminal | null>(null);
  const fitRef = useRef<XtermFitAddon | null>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const headingId = useMemo(() => `run-terminal-${runId}`, [runId]);

  const reconnect = useCallback(() => {
    setError(null);
    setStatus("connecting");
    setConnectionKey((key) => key + 1);
  }, []);

  const copyAccessCommand = useCallback(async () => {
    if (!accessCommandLabel) return;
    try {
      const response = await apiData(() =>
        humanInTheLoopApi.createRunSshAccess(runId, { ttl_minutes: 60 }),
      );
      await navigator.clipboard.writeText(response.command);
      push({ message: terminalAccessCommandCopiedMessage(provider) });
    } catch (err) {
      push({
        tone: "error",
        message: err instanceof Error
          ? err.message
          : terminalAccessCommandErrorMessage(provider),
      });
    }
  }, [accessCommandLabel, runId, provider, push]);

  useEffect(() => {
    if (!terminalEl.current) return undefined;

    let disposed = false;
    let resizeObserver: ResizeObserver | null = null;
    const textEncoder = new TextEncoder();
    const disposables: Array<{ dispose: () => void }> = [];

    async function connect() {
      setStatus("connecting");
      setError(null);

      const [{ Terminal }, { FitAddon }] = await Promise.all([
        import("@xterm/xterm"),
        import("@xterm/addon-fit"),
      ]);
      if (disposed || !terminalEl.current) return;

      const terminal = new Terminal({
        cursorBlink: true,
        convertEol: true,
        fontFamily: "\"JetBrains Mono\", ui-monospace, monospace",
        fontSize: TERMINAL_FONT_SIZE,
        lineHeight: TERMINAL_LINE_HEIGHT,
        scrollback: 5000,
        theme: TERMINAL_THEME,
      });
      const fitAddon = new FitAddon();
      terminal.loadAddon(fitAddon);
      terminal.open(terminalEl.current);
      fitAddon.fit();
      terminal.focus();
      terminalRef.current = terminal;
      fitRef.current = fitAddon;

      const socket = new WebSocket(buildTerminalWebSocketUrl(window.location, runId));
      socket.binaryType = "arraybuffer";
      socketRef.current = socket;

      disposables.push(terminal.onData((data) => {
        if (socket.readyState === WebSocket.OPEN) {
          socket.send(textEncoder.encode(data));
        }
      }));

      socket.addEventListener("open", () => {
        sendResize(socket, terminal);
      });
      socket.addEventListener("message", (event) => {
        if (typeof event.data === "string") {
          const message = parseTerminalServerMessage(event.data);
          if (!message) return;
          if (message.type === "ready") {
            setStatus("ready");
            return;
          }
          if (message.type === "closed") {
            setStatus("closed");
            return;
          }
          setStatus("error");
          setError({
            message: message.message ?? "Terminal session failed.",
            recoverable: false,
          });
          return;
        }
        const bytes = event.data instanceof ArrayBuffer
          ? new Uint8Array(event.data)
          : event.data;
        terminal.write(bytes);
      });
      socket.addEventListener("close", () => {
        setStatus((current) => current === "error" ? current : "closed");
      });
      socket.addEventListener("error", () => {
        setStatus("error");
        setError({
          message: "Terminal WebSocket connection failed.",
          recoverable: true,
        });
      });

      resizeObserver = new ResizeObserver(() => {
        fitAddon.fit();
        sendResize(socket, terminal);
      });
      resizeObserver.observe(terminalEl.current);

      if (typeof document !== "undefined" && document.fonts?.ready) {
        void document.fonts.ready.then(() => {
          if (disposed) return;
          fitAddon.fit();
          sendResize(socket, terminal);
        });
      }
    }

    void connect();

    return () => {
      disposed = true;
      resizeObserver?.disconnect();
      for (const disposable of disposables) disposable.dispose();
      socketRef.current?.send(JSON.stringify({ type: "close" }));
      socketRef.current?.close();
      socketRef.current = null;
      terminalRef.current?.dispose();
      terminalRef.current = null;
      fitRef.current = null;
    };
  }, [connectionKey, runId]);

  return (
    <section
      className="flex h-full min-h-0 flex-col"
      aria-labelledby={headingId}
    >
      <h2 id={headingId} className="sr-only">Terminal</h2>
      <div className="mb-2 flex shrink-0 flex-wrap items-center gap-3">
        {leading}
        <StatusPill status={status} detail={sandboxDetail} />
        <div className="ml-auto flex items-center gap-2">
          <Tooltip label="Reconnect">
            <button
              type="button"
              className={ICON_BUTTON_CLASS}
              onClick={reconnect}
              aria-label="Reconnect terminal"
            >
              <ArrowPathIcon className="size-4" aria-hidden="true" />
            </button>
          </Tooltip>
          {accessCommandLabel && (
            <button
              type="button"
              className={SECONDARY_BUTTON_CLASS}
              onClick={() => void copyAccessCommand()}
              aria-label={`Copy ${accessCommandLabel} command`}
            >
              <ClipboardDocumentIcon className="size-4" aria-hidden="true" />
              {accessCommandLabel}
            </button>
          )}
        </div>
      </div>
      {error ? (
        <div className="flex min-h-0 flex-1 items-center justify-center" role="alert">
          <ErrorState
            title="Terminal unavailable"
            description={error.message}
            onRetry={error.recoverable ? reconnect : undefined}
          />
        </div>
      ) : (
        <div
          className="min-h-0 flex-1 overflow-hidden rounded border border-line pb-3"
          style={{ backgroundColor: TERMINAL_BACKGROUND }}
        >
          <div ref={terminalEl} className="h-full min-h-0 p-3" />
        </div>
      )}
    </section>
  );
}
