import type { ErrorResponseEntry, RunStatusResponse } from "@qltysh/fabro-api-client";

import { apiRequest } from "./api-client";
import { queryKeys } from "./query-keys";
import type { RunStatus } from "../data/runs";

export type LifecycleAction = "cancel" | "archive" | "unarchive";

export interface LifecycleActionError {
  status: number;
  errors: ErrorResponseEntry[];
}

const CANCELABLE_STATUSES = new Set<RunStatus>([
  "submitted",
  "queued",
  "starting",
  "running",
  "paused",
  "blocked",
]);

const ARCHIVABLE_STATUSES = new Set<RunStatus>([
  "succeeded",
  "failed",
  "dead",
]);

export async function cancelRun(id: string, request?: Request): Promise<RunStatusResponse> {
  return runLifecycleAction(id, "cancel", request);
}

export async function archiveRun(id: string, request?: Request): Promise<RunStatusResponse> {
  return runLifecycleAction(id, "archive", request);
}

export async function unarchiveRun(id: string, request?: Request): Promise<RunStatusResponse> {
  return runLifecycleAction(id, "unarchive", request);
}

export async function deleteRun(id: string, request?: Request): Promise<void> {
  const response = await apiRequest(`/api/v1/runs/${encodeURIComponent(id)}`, {
    init: { method: "DELETE" },
    request,
  });

  if (response.status === 204 || response.status === 404) return;
  throw await parseLifecycleActionError(response);
}

export function canCancel(status: string | null | undefined): boolean {
  return !!status && CANCELABLE_STATUSES.has(status as RunStatus);
}

export function canArchive(status: string | null | undefined): boolean {
  return !!status && ARCHIVABLE_STATUSES.has(status as RunStatus);
}

export function canUnarchive(status: string | null | undefined): boolean {
  return status === "archived";
}

export function canDelete(status: string | null | undefined): boolean {
  return status === "archived";
}

export function isTerminalCancelledRun(run: RunStatusResponse): boolean {
  return run.status.kind === "failed" && run.status.reason === "cancelled";
}

export function deleteErrorMessage(error: unknown): string {
  if (isLifecycleActionError(error)) {
    if (error.status === 409) {
      return "Active runs can't be deleted.";
    }
    const detail = error.errors[0]?.detail?.trim();
    if (detail) return detail;
  }
  return "Couldn't delete the run right now. Try again.";
}

export function mapError(error: unknown, action: LifecycleAction): string {
  if (isLifecycleActionError(error)) {
    if (error.status === 404) {
      return "This run no longer exists.";
    }
    if (error.status === 409) {
      switch (action) {
        case "cancel":
          return "This run can no longer be cancelled.";
        case "archive":
          return "Only terminal runs can be archived.";
        case "unarchive":
          return "Active runs can't be unarchived.";
      }
    }

    const detail = error.errors[0]?.detail?.trim();
    if (detail) {
      return detail;
    }
  }

  switch (action) {
    case "cancel":
      return "Couldn't cancel the run right now. Try again.";
    case "archive":
      return "Couldn't archive the run right now. Try again.";
    case "unarchive":
      return "Couldn't unarchive the run right now. Try again.";
  }
}

async function runLifecycleAction(
  id: string,
  action: LifecycleAction,
  request?: Request,
): Promise<RunStatusResponse> {
  const response = await apiRequest(queryKeys.runs[action](id), {
    init: {
      method: "POST",
    },
    request,
  });

  if (!response.ok) {
    throw await parseLifecycleActionError(response);
  }

  return response.json() as Promise<RunStatusResponse>;
}

async function parseLifecycleActionError(response: Response): Promise<LifecycleActionError> {
  let bodyText = "";
  try {
    bodyText = await response.text();
  } catch {
    // Ignore body read failures and fall back to the status only.
  }

  if (!bodyText) {
    return { status: response.status, errors: [] };
  }

  try {
    const body = JSON.parse(bodyText) as { errors?: unknown };
    if (!Array.isArray(body.errors)) {
      return { status: response.status, errors: [] };
    }

    const errors = body.errors.filter(isErrorResponseEntry);
    return { status: response.status, errors };
  } catch {
    return { status: response.status, errors: [] };
  }
}

export function isLifecycleActionError(value: unknown): value is LifecycleActionError {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  return typeof record.status === "number" && Array.isArray(record.errors);
}

function isErrorResponseEntry(value: unknown): value is ErrorResponseEntry {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  return (
    typeof record.status === "string"
    && typeof record.title === "string"
    && typeof record.detail === "string"
  );
}
