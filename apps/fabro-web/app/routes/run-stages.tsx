import { useParams } from "react-router";
import { CommandLineIcon, ChatBubbleLeftIcon } from "@heroicons/react/24/outline";
import { ToolBlock } from "../components/tool-use";
import type { ToolUse } from "../components/tool-use";
import { StageSidebar, statusConfig } from "../components/stage-sidebar";
import type { Stage } from "../components/stage-sidebar";
import { apiJson, apiJsonOrNull } from "../api";
import { isVisibleStage } from "../data/runs";
import { formatDurationSecs } from "../lib/format";
import type { PaginatedRunStageList, StageTurn as ApiStageTurn, PaginatedStageTurnList, PaginatedEventList } from "@qltysh/fabro-api-client";

export const handle = { wide: true };

type TurnType =
  | { kind: "system"; content: string }
  | { kind: "assistant"; content: string }
  | { kind: "tool"; tools: ToolUse[] };

interface RawEvent {
  node_id?: string;
  event: string;
  properties?: Record<string, unknown>;
  text?: string;
  tool_name?: string;
  tool_call_id?: string;
  arguments?: unknown;
  output?: unknown;
  is_error?: boolean;
}

function turnsFromEvents(events: RawEvent[], stageId: string): TurnType[] {
  const stageEvents = events.filter((e) => e.node_id === stageId);
  const turns: TurnType[] = [];
  // Collect tool pairs: started → completed
  const pendingTools = new Map<string, { toolName: string; input: string }>();

  for (const e of stageEvents) {
    switch (e.event) {
      case "stage.prompt":
        turns.push({ kind: "system", content: e.properties?.text as string ?? e.text ?? "" });
        break;
      case "agent.message": {
        const msg = e.properties?.text as string ?? e.text ?? "";
        if (msg) turns.push({ kind: "assistant", content: msg });
        break;
      }
      case "agent.tool.started": {
        const callId = e.properties?.tool_call_id as string ?? e.tool_call_id ?? "";
        pendingTools.set(callId, {
          toolName: e.properties?.tool_name as string ?? e.tool_name ?? "",
          input: typeof (e.properties?.arguments ?? e.arguments) === "string"
            ? (e.properties?.arguments ?? e.arguments) as string
            : JSON.stringify(e.properties?.arguments ?? e.arguments ?? ""),
        });
        break;
      }
      case "agent.tool.completed": {
        const callId = e.properties?.tool_call_id as string ?? e.tool_call_id ?? "";
        const started = pendingTools.get(callId);
        const output = e.properties?.output ?? e.output ?? "";
        const result = typeof output === "string" ? output : JSON.stringify(output);
        const tool: ToolUse = {
          id: callId,
          toolName: started?.toolName ?? e.properties?.tool_name as string ?? e.tool_name ?? "",
          input: started?.input ?? "",
          result,
          isError: (e.properties?.is_error ?? e.is_error) === true,
        };
        pendingTools.delete(callId);
        turns.push({ kind: "tool", tools: [tool] });
        break;
      }
    }
  }
  return turns;
}

export async function loader({ request, params }: any) {
  const { data: apiStages } = await apiJson<PaginatedRunStageList>(`/runs/${params.id}/stages`, { request });
  const stages: Stage[] = apiStages.filter((s) => isVisibleStage(s.id)).map((s) => ({
    id: s.id,
    name: s.name,
    status: s.status as Stage["status"],
    duration: s.duration_secs != null ? formatDurationSecs(s.duration_secs) : "--",
  }));

  const selectedStageId = params.stageId ?? stages[0]?.id;

  // Try demo turns endpoint first, fall back to events.
  let turns: TurnType[] = [];
  if (selectedStageId) {
    const turnsResult = await apiJsonOrNull<PaginatedStageTurnList>(
      `/runs/${params.id}/stages/${selectedStageId}/turns`,
      { request },
    );
    if (turnsResult?.data?.length) {
      turns = turnsResult.data.map((t: ApiStageTurn): TurnType => {
        if (t.kind === "tool" && t.tools) {
          return {
            kind: "tool",
            tools: t.tools.map((tu) => ({
              id: tu.id,
              toolName: tu.tool_name,
              input: tu.input,
              result: tu.result,
              isError: tu.is_error,
              durationMs: tu.duration_ms,
            })),
          };
        }
        return { kind: t.kind as "system" | "assistant", content: t.content ?? "" };
      });
    } else {
      // Fetch events and build turns from them.
      const eventsResult = await apiJsonOrNull<PaginatedEventList>(
        `/runs/${params.id}/events?limit=1000`,
        { request },
      );
      if (eventsResult?.data) {
        turns = turnsFromEvents(eventsResult.data as unknown as RawEvent[], selectedStageId);
      }
    }
  }

  return { stages, turns };
}

function SystemBlock({ content }: { content: string }) {
  return (
    <div className="rounded-md border border-amber/10 bg-amber/5 overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2">
        <CommandLineIcon className="size-4 shrink-0 text-amber" />
        <span className="text-xs font-medium text-fg-3">System Prompt</span>
      </div>
      <div className="border-t border-line px-3 py-2.5">
        <pre className="whitespace-pre-wrap font-mono text-xs leading-relaxed text-fg-3">{content}</pre>
      </div>
    </div>
  );
}

function AssistantBlock({ content }: { content: string }) {
  return (
    <div className="rounded-md border border-teal-500/10 bg-teal-500/5 overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2">
        <ChatBubbleLeftIcon className="size-4 shrink-0 text-teal-500" />
        <span className="text-xs font-medium text-fg-3">Assistant</span>
      </div>
      <div className="border-t border-line px-3 py-2.5">
        <pre className="whitespace-pre-wrap font-mono text-xs leading-relaxed text-fg-3">{content}</pre>
      </div>
    </div>
  );
}

export default function RunStages({ loaderData }: any) {
  const { id, stageId } = useParams();
  const { stages, turns } = loaderData;

  const selectedStage = stages.find((s: Stage) => s.id === stageId) ?? stages[0];
  const selectedConfig = selectedStage ? statusConfig[selectedStage.status] : statusConfig.pending;
  const SelectedIcon = selectedConfig.icon;

  return (
    <div className="flex gap-6">
      <StageSidebar stages={stages} runId={id!} selectedStageId={selectedStage?.id} />

      <div className="min-w-0 flex-1 space-y-3">
        <div className="flex items-center gap-2">
          <SelectedIcon className={`size-5 ${selectedConfig.color}`} />
          <h3 className="text-sm font-medium text-fg">{selectedStage.name}</h3>
          <span className="font-mono text-xs text-fg-muted">{selectedStage.duration}</span>
        </div>

        {turns.map((turn: TurnType, i: number) => {
          switch (turn.kind) {
            case "system":
              return <SystemBlock key={`turn-${i}`} content={turn.content} />;
            case "assistant":
              return <AssistantBlock key={`turn-${i}`} content={turn.content} />;
            case "tool":
              return <ToolBlock key={`turn-${i}`} tools={turn.tools} />;
          }
        })}
      </div>
    </div>
  );
}
