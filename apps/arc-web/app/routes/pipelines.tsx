import { useState, useCallback, useRef } from "react";
import { Link } from "react-router";
import { ChevronDownIcon, ChevronRightIcon, MagnifyingGlassIcon } from "@heroicons/react/24/outline";
import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import type { DragEndEvent } from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
  arrayMove,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { columns as staticColumns, ciConfig, statusColors } from "../data/runs";
import type { CiStatus, RunItem, RunWithStatus } from "../data/runs";
import type { Route } from "./+types/pipelines";

export function meta({}: Route.MetaArgs) {
  return [{ title: "Runs — Arc" }];
}


function GitBranchIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 16 16"
      fill="currentColor"
      className={className}
      aria-hidden="true"
    >
      <path d="M9.5 3.25a2.25 2.25 0 1 1 3 2.122V6A2.5 2.5 0 0 1 10 8.5H6a1 1 0 0 0-1 1v1.128a2.251 2.251 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.5 0v1.836A2.5 2.5 0 0 1 6 7h4a1 1 0 0 0 1-1v-.628A2.25 2.25 0 0 1 9.5 3.25Zm-6 0a.75.75 0 1 0 1.5 0 .75.75 0 0 0-1.5 0Zm8.25-.75a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5ZM4.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Z" />
    </svg>
  );
}

function GitPullRequestIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 16 16"
      fill="currentColor"
      className={className}
      aria-hidden="true"
    >
      <path d="M1.5 3.25a2.25 2.25 0 1 1 3 2.122v5.256a2.251 2.251 0 1 1-1.5 0V5.372A2.25 2.25 0 0 1 1.5 3.25Zm5.677-.177L9.573.677A.25.25 0 0 1 10 .854V2.5h1A2.5 2.5 0 0 1 13.5 5v5.628a2.251 2.251 0 1 1-1.5 0V5a1 1 0 0 0-1-1h-1v1.646a.25.25 0 0 1-.427.177L7.177 3.427a.25.25 0 0 1 0-.354ZM3.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Zm0 9.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Zm8.25.75a.75.75 0 1 0 1.5 0 .75.75 0 0 0-1.5 0Z" />
    </svg>
  );
}

const iconMap = {
  branch: GitBranchIcon,
  pr: GitPullRequestIcon,
};

function CiBadge({ status }: { status: CiStatus }) {
  const config = ciConfig[status];
  return (
    <span className={`inline-flex items-center gap-1.5 font-mono text-xs ${config.text}`}>
      <span className={`size-1.5 rounded-full ${config.dot}`} />
      {config.label}
    </span>
  );
}

const totalCards = staticColumns.reduce((sum, col) => sum + col.items.length, 0);
const totalPrs = staticColumns.reduce(
  (sum, col) => sum + col.items.filter((item) => item.number != null).length,
  0,
);

export const handle = {
  wide: true,
  headerExtra: (
    <div className="flex items-center gap-4 font-mono text-xs text-ice-300">
      <span>
        <span className="text-white">{totalCards}</span> runs
      </span>
      <span>
        <span className="text-white">{totalPrs}</span> PRs
      </span>
    </div>
  ),
};

function PrCard({
  pr,
  icon: Icon,
  iconColor,
  actions,
}: {
  pr: RunItem;
  icon: React.ComponentType<{ className?: string }>;
  iconColor: string;
  actions?: string[];
}) {
  return (
    <Link to={`/runs/${pr.id}`} className="group block rounded-md border border-white/[0.06] bg-navy-800/80 p-4 transition-all duration-200 hover:border-white/[0.12] hover:bg-navy-800 hover:shadow-lg hover:shadow-black/20">
      <div className="mb-2 flex items-center gap-1.5">
        <Icon className={`size-3.5 shrink-0 ${iconColor}`} />
        <span className="font-mono text-xs font-medium text-teal-500">
          {pr.repo}
        </span>
        {pr.number != null && (
          <span className="font-mono text-xs text-navy-600">
            #{pr.number}
          </span>
        )}
      </div>

      <p className="text-sm leading-snug text-ice-100">{pr.title}</p>

      {(pr.additions != null || pr.resources != null || pr.ci != null || pr.elapsed != null) && (
        <div className="mt-3 flex items-center gap-3 font-mono text-xs">
          {pr.resources != null && (
            <span className="text-ice-300">{pr.resources}</span>
          )}
          {pr.additions != null && pr.deletions != null && (
            <>
              <span className="text-mint">
                +{pr.additions.toLocaleString()}
              </span>
              <span className="text-coral">
                -{pr.deletions.toLocaleString()}
              </span>
            </>
          )}
          {pr.comments != null && (
            <span className="inline-flex items-center gap-1 text-navy-600">
              <svg viewBox="0 0 16 16" fill="currentColor" className="size-3" aria-hidden="true">
                <path d="M1 2.75C1 1.784 1.784 1 2.75 1h10.5c.966 0 1.75.784 1.75 1.75v7.5A1.75 1.75 0 0 1 13.25 12H9.06l-2.573 2.573A1.458 1.458 0 0 1 4 13.543V12H2.75A1.75 1.75 0 0 1 1 10.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.19l2.72-2.72a.749.749 0 0 1 .53-.22h4.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25Z" />
              </svg>
              {pr.comments}
            </span>
          )}
          {pr.elapsed != null && (
            <span className={`ml-auto font-mono ${pr.elapsedWarning ? "text-amber" : "text-navy-600"}`}>{pr.elapsed}</span>
          )}
        </div>
      )}

      {(actions != null && actions.length > 0 || pr.ci != null) && (
        <div className="mt-3 flex items-center gap-1.5">
          {actions?.map((label) => (
            <button
              key={label}
              type="button"
              disabled={pr.actionDisabled}
              className={`inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-[11px] font-medium transition-colors disabled:cursor-not-allowed disabled:text-navy-600 disabled:border-white/[0.04] ${
                label === "Merge"
                  ? "border-mint/20 text-mint hover:border-mint/50 hover:text-white"
                  : label === "Answer Question"
                    ? "border-amber/20 text-amber hover:border-amber/50 hover:text-white"
                    : label === "Resolve"
                      ? "border-teal-500/20 text-teal-500 hover:border-teal-500/50 hover:text-white"
                      : "border-white/[0.08] text-ice-300 hover:border-teal-500/40 hover:text-white"
              }`}
            >
              {label === "Watch" && (
                <svg viewBox="0 0 16 16" fill="currentColor" className="size-3" aria-hidden="true">
                  <path d="M8 2c1.981 0 3.671.992 4.933 2.078 1.27 1.091 2.187 2.345 2.637 3.023a1.62 1.62 0 0 1 0 1.798c-.45.678-1.367 1.932-2.637 3.023C11.67 13.008 9.981 14 8 14c-1.981 0-3.671-.992-4.933-2.078C1.797 10.831.88 9.577.43 8.899a1.62 1.62 0 0 1 0-1.798c.45-.678 1.367-1.932 2.637-3.023C4.33 2.992 6.019 2 8 2ZM1.679 7.932a.12.12 0 0 0 0 .136c.411.622 1.241 1.75 2.366 2.717C5.176 11.758 6.527 12.5 8 12.5c1.473 0 2.825-.742 3.955-1.715 1.124-.967 1.954-2.096 2.366-2.717a.12.12 0 0 0 0-.136c-.412-.621-1.242-1.75-2.366-2.717C10.824 4.242 9.473 3.5 8 3.5c-1.473 0-2.824.742-3.955 1.715-1.124.967-1.954 2.096-2.366 2.717ZM8 10a2 2 0 1 1-.001-3.999A2 2 0 0 1 8 10Z" />
                </svg>
              )}
              {label === "Steer" && (
                <svg viewBox="0 0 16 16" fill="currentColor" className="size-3" aria-hidden="true">
                  <path d="M8 0a8 8 0 1 1 0 16A8 8 0 0 1 8 0ZM1.5 8a6.5 6.5 0 1 0 13 0 6.5 6.5 0 0 0-13 0Zm7.25-4.5a.75.75 0 0 0-1.5 0v.582a2.75 2.75 0 0 0-2.168 2.168H4.5a.75.75 0 0 0 0 1.5h.582a2.75 2.75 0 0 0 2.168 2.168v.582a.75.75 0 0 0 1.5 0v-.582a2.75 2.75 0 0 0 2.168-2.168h.582a.75.75 0 0 0 0-1.5h-.582A2.75 2.75 0 0 0 8.75 4.082ZM8 6.75a1.25 1.25 0 1 0 0 2.5 1.25 1.25 0 0 0 0-2.5Z" />
                </svg>
              )}
              {label === "Answer Question" && (
                <svg viewBox="0 0 16 16" fill="currentColor" className="size-3" aria-hidden="true">
                  <path d="M1 2.75C1 1.784 1.784 1 2.75 1h10.5c.966 0 1.75.784 1.75 1.75v7.5A1.75 1.75 0 0 1 13.25 12H9.06l-2.573 2.573A1.458 1.458 0 0 1 4 13.543V12H2.75A1.75 1.75 0 0 1 1 10.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.19l2.72-2.72a.749.749 0 0 1 .53-.22h4.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25Z" />
                </svg>
              )}
              {label === "Resolve" && (
                <svg viewBox="0 0 16 16" fill="currentColor" className="size-3" aria-hidden="true">
                  <path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.751.751 0 0 1 .018-1.042.751.751 0 0 1 1.042-.018L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0Z" />
                </svg>
              )}
              {label === "Merge" && (
                <svg viewBox="0 0 16 16" fill="currentColor" className="size-3" aria-hidden="true">
                  <path d="M5.45 5.154A4.25 4.25 0 0 0 9.25 7.5h1.378a2.251 2.251 0 1 1 0 1.5H9.25A5.734 5.734 0 0 1 5 7.123v3.505a2.25 2.25 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.95-.218ZM4.25 13.5a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5Zm8-8a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5ZM4.25 4a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5Z" />
                </svg>
              )}
              {label}
            </button>
          ))}
          {pr.ci != null && <span className="ml-auto flex items-center"><CiBadge status={pr.ci} /></span>}
        </div>
      )}
    </Link>
  );
}

function SortablePrCard({
  pr,
  icon,
  iconColor,
  actions,
}: {
  pr: RunItem;
  icon: React.ComponentType<{ className?: string }>;
  iconColor: string;
  actions?: string[];
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: pr.id });
  const wasDragging = useRef(false);
  if (isDragging) wasDragging.current = true;
  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : undefined,
    position: "relative" as const,
    zIndex: isDragging ? 10 : undefined,
  };
  return (
    <div
      ref={setNodeRef}
      style={style}
      {...attributes}
      {...listeners}
      onClickCapture={(e) => {
        if (wasDragging.current) {
          e.preventDefault();
          e.stopPropagation();
          wasDragging.current = false;
        }
      }}
    >
      <PrCard pr={pr} icon={icon} iconColor={iconColor} actions={actions} />
    </div>
  );
}

function BoardColumn({ column }: { column: (typeof staticColumns)[number] }) {
  const Icon = iconMap[column.iconType];
  return (
    <div className="flex min-w-[280px] flex-1 flex-col">
      <div className="mb-4 flex items-center gap-3">
        <div className={`h-2.5 w-2.5 rounded-full ${column.accent}`} />
        <h3 className="text-sm font-semibold tracking-wide text-ice-100">
          {column.name}
        </h3>
        <span className="rounded-full bg-white/[0.06] px-2 py-0.5 font-mono text-xs text-navy-600">
          {column.items.length}
        </span>
      </div>

      <SortableContext items={column.items.map((pr) => pr.id)} strategy={verticalListSortingStrategy}>
        <div className="flex flex-1 flex-col gap-3">
          {column.items.map((pr) => (
            <SortablePrCard
              key={pr.id}
              pr={pr}
              icon={Icon}
              iconColor={column.iconColor}
              actions={column.actions}
            />
          ))}
        </div>
      </SortableContext>
    </div>
  );
}

type ViewMode = "columns" | "list";

function RunRow({ run }: { run: RunWithStatus }) {
  return (
    <Link to={`/runs/${run.id}`} className="grid items-center rounded-md border border-white/[0.06] bg-navy-800/80 px-4 py-3 transition-all duration-200 hover:border-white/[0.12] hover:bg-navy-800" style={{ gridColumn: "1 / -1", gridTemplateColumns: "subgrid" }}>
      <span className={`font-mono text-xs pr-2 ${run.elapsedWarning ? "text-amber" : "text-navy-600"}`}>
        {run.elapsed}
      </span>

      <span className="flex items-center gap-2 min-w-0">
        <span className="font-mono text-xs font-medium text-teal-500">{run.repo}</span>
        <span className="truncate text-sm text-ice-100">{run.title}</span>
        {run.comments != null && run.comments > 0 && (
          <span className="inline-flex shrink-0 items-center gap-1 font-mono text-xs text-navy-600">
            <svg viewBox="0 0 16 16" fill="currentColor" className="size-3" aria-hidden="true">
              <path d="M1 2.75C1 1.784 1.784 1 2.75 1h10.5c.966 0 1.75.784 1.75 1.75v7.5A1.75 1.75 0 0 1 13.25 12H9.06l-2.573 2.573A1.458 1.458 0 0 1 4 13.543V12H2.75A1.75 1.75 0 0 1 1 10.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.19l2.72-2.72a.749.749 0 0 1 .53-.22h4.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25Z" />
            </svg>
            {run.comments}
          </span>
        )}
      </span>

      <span className="flex items-center justify-end gap-2 pr-4 font-mono text-xs tabular-nums">
        {run.additions != null && <span className="text-mint">+{run.additions.toLocaleString()}</span>}
        {run.deletions != null && <span className="text-coral">-{run.deletions.toLocaleString()}</span>}
      </span>

      <span className="inline-flex items-center justify-end gap-1.5 font-mono text-xs text-navy-600">
        {run.number != null && (
          <>
            <GitPullRequestIcon className="size-3" />
            #{run.number}
            {run.ci != null && <span className={`size-1.5 rounded-full ${ciConfig[run.ci].dot}`} />}
          </>
        )}
      </span>
    </Link>
  );
}

function SortableRunRow({ run }: { run: RunWithStatus }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: run.id });
  const wasDragging = useRef(false);
  if (isDragging) wasDragging.current = true;
  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : undefined,
    position: "relative" as const,
    zIndex: isDragging ? 10 : undefined,
    gridColumn: "1 / -1",
    display: "grid",
    gridTemplateColumns: "subgrid",
  };
  return (
    <div
      ref={setNodeRef}
      style={style}
      {...attributes}
      {...listeners}
      onClickCapture={(e) => {
        if (wasDragging.current) {
          e.preventDefault();
          e.stopPropagation();
          wasDragging.current = false;
        }
      }}
    >
      <RunRow run={run} />
    </div>
  );
}

const allRepos = [...new Set(staticColumns.flatMap((col) => col.items.map((item) => item.repo)))].sort();

export default function Pipelines() {
  const [query, setQuery] = useState("");
  const [repoFilter, setRepoFilter] = useState("all");
  const [view, setView] = useState<ViewMode>("columns");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [columns, setColumns] = useState(staticColumns);
  const lowerQuery = query.toLowerCase();

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const handleDragEnd = useCallback((event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;

    setColumns((prev) =>
      prev.map((col) => {
        const oldIndex = col.items.findIndex((item) => item.id === active.id);
        const newIndex = col.items.findIndex((item) => item.id === over.id);
        if (oldIndex === -1 || newIndex === -1) return col;
        return { ...col, items: arrayMove(col.items, oldIndex, newIndex) };
      }),
    );
  }, []);

  const filteredColumns = columns.map((col) => ({
    ...col,
    items: col.items.filter(
      (item) =>
        (repoFilter === "all" || item.repo === repoFilter) &&
        (!query ||
          item.title.toLowerCase().includes(lowerQuery) ||
          item.repo.toLowerCase().includes(lowerQuery) ||
          (item.number != null && `#${item.number}`.includes(lowerQuery))),
    ),
  }));

  return (
    <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
      <div className="space-y-4">
        <div className="flex gap-3">
          <div className="relative flex-1">
            <MagnifyingGlassIcon className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-navy-600" />
            <input
              type="text"
              placeholder="Search runs..."
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              className="w-full rounded-md border border-white/[0.06] bg-navy-800/80 py-2 pl-9 pr-3 text-sm text-ice-100 placeholder-navy-600 outline-none transition-colors focus:border-teal-500/40 focus:ring-0"
            />
          </div>
          <div className="relative">
            <select
              value={repoFilter}
              onChange={(e) => setRepoFilter(e.target.value)}
              className="appearance-none rounded-md border border-white/[0.06] bg-navy-800/80 py-2 pl-3 pr-8 text-sm text-ice-100 outline-none transition-colors focus:border-teal-500/40 focus:ring-0"
            >
              <option value="all">All repos</option>
              {allRepos.map((repo) => (
                <option key={repo} value={repo}>{repo}</option>
              ))}
            </select>
            <ChevronDownIcon className="pointer-events-none absolute right-2 top-1/2 size-4 -translate-y-1/2 text-navy-600" />
          </div>
          <div className="flex rounded-md border border-white/[0.06] bg-navy-800/80">
            <button
              type="button"
              onClick={() => setView("columns")}
              className={`inline-flex items-center gap-1.5 px-3 py-2 text-xs font-medium transition-colors ${view === "columns" ? "text-teal-500" : "text-navy-600 hover:text-ice-300"}`}
              aria-label="Columns view"
            >
              <svg viewBox="0 0 20 20" fill="currentColor" className="size-4" aria-hidden="true">
                <path d="M2 4.75A.75.75 0 0 1 2.75 4h2.5a.75.75 0 0 1 .75.75v10.5a.75.75 0 0 1-.75.75h-2.5a.75.75 0 0 1-.75-.75V4.75ZM8.25 4a.75.75 0 0 0-.75.75v10.5c0 .414.336.75.75.75h2.5a.75.75 0 0 0 .75-.75V4.75a.75.75 0 0 0-.75-.75h-2.5ZM14 4.75a.75.75 0 0 1 .75-.75h2.5a.75.75 0 0 1 .75.75v10.5a.75.75 0 0 1-.75.75h-2.5a.75.75 0 0 1-.75-.75V4.75Z" />
              </svg>
            </button>
            <button
              type="button"
              onClick={() => setView("list")}
              className={`inline-flex items-center gap-1.5 px-3 py-2 text-xs font-medium transition-colors ${view === "list" ? "text-teal-500" : "text-navy-600 hover:text-ice-300"}`}
              aria-label="List view"
            >
              <svg viewBox="0 0 20 20" fill="currentColor" className="size-4" aria-hidden="true">
                <path fillRule="evenodd" d="M2 4.75A.75.75 0 0 1 2.75 4h14.5a.75.75 0 0 1 0 1.5H2.75A.75.75 0 0 1 2 4.75Zm0 5A.75.75 0 0 1 2.75 9h14.5a.75.75 0 0 1 0 1.5H2.75A.75.75 0 0 1 2 9.75Zm0 5a.75.75 0 0 1 .75-.75h14.5a.75.75 0 0 1 0 1.5H2.75a.75.75 0 0 1-.75-.75Z" clipRule="evenodd" />
              </svg>
            </button>
          </div>
        </div>

        {view === "columns" ? (
          <div className="flex gap-5 overflow-x-auto pb-4">
            {filteredColumns.map((col) => (
              <BoardColumn key={col.id} column={col} />
            ))}
          </div>
        ) : (
          <div className="space-y-4">
            {filteredColumns.map((col) => {
              const isCollapsed = collapsed.has(col.id);
              return (
                <div key={col.id}>
                  <button
                    type="button"
                    onClick={() => setCollapsed((prev) => {
                      const next = new Set(prev);
                      if (next.has(col.id)) next.delete(col.id);
                      else next.add(col.id);
                      return next;
                    })}
                    className="mb-3 flex w-full items-center gap-2 text-left"
                  >
                    {isCollapsed
                      ? <ChevronRightIcon className="size-3.5 text-navy-600" />
                      : <ChevronDownIcon className="size-3.5 text-navy-600" />}
                    <div className={`h-2.5 w-2.5 rounded-full ${col.accent}`} />
                    <h3 className="text-sm font-semibold tracking-wide text-ice-100">{col.name}</h3>
                    <span className="rounded-full bg-white/[0.06] px-2 py-0.5 font-mono text-xs text-navy-600">
                      {col.items.length}
                    </span>
                  </button>
                  {!isCollapsed && (col.items.length > 0 ? (
                    <SortableContext items={col.items.map((item) => item.id)} strategy={verticalListSortingStrategy}>
                      <div className="grid gap-2" style={{ gridTemplateColumns: "5rem 1fr 8rem auto" }}>
                        {col.items.map((item) => (
                          <SortableRunRow key={item.id} run={{ ...item, status: col.id, statusLabel: col.name }} />
                        ))}
                      </div>
                    </SortableContext>
                  ) : (
                    <p className="py-4 text-center text-sm text-navy-600">No runs</p>
                  ))}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </DndContext>
  );
}
