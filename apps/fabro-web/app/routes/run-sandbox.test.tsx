import { afterEach, describe, expect, mock, test } from "bun:test";
import type { ReactNode } from "react";
import TestRenderer, { act } from "react-test-renderer";
import { MemoryRouter, Route, Routes } from "react-router";

import type { SandboxDetails } from "@qltysh/fabro-api-client";

let currentDetails: SandboxDetails | null = null;
let currentLoading = false;
let currentError: Error | null = null;

mock.module("../lib/queries", () => ({
  useRunSandboxDetails: () => ({
    data:         currentDetails,
    error:        currentError,
    isLoading:    currentLoading,
    isValidating: false,
    mutate:       mock(() => Promise.resolve(currentDetails)),
  }),
  // FilesystemPanel and VncPanel import these. They never run in this
  // file's tests, but the export shape needs to exist for module evaluation.
  useSandboxFiles: () => ({
    data:         undefined,
    error:        undefined,
    isValidating: false,
    mutate:       mock(() => Promise.resolve()),
  }),
  useSandboxFile: () => ({
    data:   undefined,
    error:  undefined,
    mutate: mock(() => Promise.resolve()),
  }),
  useSandboxVncPreview: () => ({
    data:         undefined,
    error:        undefined,
    isLoading:    false,
    isValidating: false,
    mutate:       mock(() => Promise.resolve()),
  }),
}));

mock.module("../components/terminal-view", () => ({
  // Render the leading slot so the mode toggle (now hosted inside each panel
  // header) is reachable from outer tab-presence assertions.
  default: ({ leading }: { leading?: ReactNode }) => <div>{leading}</div>,
  TERMINAL_DOCK_CLEARANCE_CLASS: "",
}));

// Stub @pierre/trees and @pierre/diffs runtime so the filesystem panel can
// render in this test without pulling in shiki/highlighter modules. The
// filesystem panel's own behavior is exercised in filesystem-panel.test.tsx.
mock.module("@pierre/trees/react", () => ({
  FileTree:             () => <div data-test-id="file-tree-stub" />,
  useFileTree:          () => ({ model: { resetPaths: () => {} } }),
  useFileTreeSelection: () => [],
}));
mock.module("@pierre/trees", () => ({ themeToTreeStyles: () => ({}) }));
mock.module("@pierre/theme/pierre-dark", () => ({ default: {} }));
mock.module("@pierre/diffs/react", () => ({
  File: () => <div data-test-id="pierre-file-stub" />,
}));

const { default: RunSandbox, formatBytesAsMemory, normalizeSandboxMode } =
  await import("./run-sandbox");
mock.restore();

const mountedRenderers: TestRenderer.ReactTestRenderer[] = [];

function renderRoute(initialPath: string = "/runs/run_1/sandbox") {
  let renderer!: TestRenderer.ReactTestRenderer;
  act(() => {
    renderer = TestRenderer.create(
      <MemoryRouter initialEntries={[initialPath]}>
        <Routes>
          <Route path="/runs/:id/sandbox" element={<RunSandbox params={{ id: "run_1" }} />} />
        </Routes>
      </MemoryRouter>,
    );
  });
  mountedRenderers.push(renderer);
  return renderer;
}

afterEach(() => {
  for (const renderer of mountedRenderers.splice(0)) {
    act(() => renderer.unmount());
  }
  currentDetails = null;
  currentLoading = false;
  currentError = null;
});

describe("formatBytesAsMemory", () => {
  test("renders gibibytes for round values", () => {
    expect(formatBytesAsMemory(2 * 1024 * 1024 * 1024)).toBe("2 GiB");
  });

  test("renders fractional gibibytes with one decimal", () => {
    expect(formatBytesAsMemory(2.5 * 1024 * 1024 * 1024)).toBe("2.5 GiB");
  });

  test("falls back to mebibytes when below a gibibyte", () => {
    expect(formatBytesAsMemory(512 * 1024 * 1024)).toBe("512 MiB");
  });
});

describe("RunSandbox route", () => {
  test("renders panels for a fully populated sandbox", () => {
    currentDetails = {
      provider:     "docker",
      name:         "fabro-run-abc",
      id:           "abcdef123456",
      state:        "running",
      native_state: "running",
      region:       null,
      image:        "ghcr.io/fabro/sandbox:latest",
      resources:    {
        cpu_cores:    2,
        memory_bytes: 4 * 1024 * 1024 * 1024,
        disk_bytes:   null,
      },
      labels: { run: "abc" },
      timestamps: {
        created_at:       "2026-05-09T12:00:00Z",
        last_activity_at: null,
      },
    };
    const renderer = renderRoute();

    const panelHeadings = renderer.root
      .findAll((node) => node.type === "h3")
      .map((node) => node.children.find((child) => typeof child === "string"))
      .filter((text): text is string => typeof text === "string");
    expect(panelHeadings).toEqual(["Overview", "Resources", "Labels", "Timestamps"]);
  });

  test("renders without crashing when most fields are null", () => {
    currentDetails = {
      provider:     "local",
      name:         null,
      id:           null,
      state:        "unknown",
      native_state: null,
      region:       null,
      image:        null,
      resources:    {
        cpu_cores:    null,
        memory_bytes: null,
        disk_bytes:   null,
      },
      labels: {},
      timestamps: {
        created_at:       null,
        last_activity_at: null,
      },
    };
    const renderer = renderRoute();

    const labelsHeading = renderer.root.findAll(
      (node) =>
        node.type === "h3" &&
        node.children.find((child) => typeof child === "string") === "Labels",
    );
    expect(labelsHeading).toHaveLength(1);

    const noLabelsCopy = renderer.root.findAll(
      (node) =>
        node.type === "div" &&
        Array.isArray(node.children) &&
        node.children.includes("No labels"),
    );
    expect(noLabelsCopy).toHaveLength(1);
  });

  test("shows the empty state when no sandbox is reported", () => {
    currentDetails = null;
    const renderer = renderRoute();

    const titles = renderer.root.findAll(
      (node) =>
        node.type === "p" &&
        Array.isArray(node.children) &&
        node.children.includes("No sandbox"),
    );
    expect(titles).toHaveLength(1);
  });

  test("Terminal is the default right-column mode", () => {
    currentDetails = {
      provider:     "docker",
      name:         "fabro-run-abc",
      id:           null,
      state:        "running",
      native_state: null,
      region:       null,
      image:        null,
      resources:    { cpu_cores: null, memory_bytes: null, disk_bytes: null },
      labels:       {},
      timestamps:   { created_at: null, last_activity_at: null },
    };
    const renderer = renderRoute();

    const tabs = renderer.root.findAll(
      (node) =>
        node.type === "button" && node.props.role === "tab",
    );
    // Docker provider hides the VNC tab.
    expect(tabs).toHaveLength(2);
    const labels = tabs.map((tab) => tab.children.find((c) => typeof c === "string"));
    expect(labels).toEqual(["Terminal", "Filesystem"]);
    const selected = tabs.find((tab) => tab.props["aria-selected"] === true);
    expect(selected?.children.find((c) => typeof c === "string")).toBe("Terminal");
  });

  test("Daytona provider exposes a VNC tab", () => {
    currentDetails = {
      provider:     "daytona",
      name:         "fabro-run-abc",
      id:           null,
      state:        "running",
      native_state: null,
      region:       null,
      image:        null,
      resources:    { cpu_cores: null, memory_bytes: null, disk_bytes: null },
      labels:       {},
      timestamps:   { created_at: null, last_activity_at: null },
    };
    const renderer = renderRoute();
    const tabs = renderer.root.findAll(
      (node) => node.type === "button" && node.props.role === "tab",
    );
    expect(tabs).toHaveLength(3);
    const labels = tabs.map((tab) => tab.children.find((c) => typeof c === "string"));
    expect(labels).toEqual(["Terminal", "Filesystem", "VNC"]);
  });

  test("Docker provider falls back to terminal when ?mode=vnc is requested", () => {
    currentDetails = {
      provider:     "docker",
      name:         "fabro-run-abc",
      id:           null,
      state:        "running",
      native_state: null,
      region:       null,
      image:        null,
      resources:    { cpu_cores: null, memory_bytes: null, disk_bytes: null },
      labels:       {},
      timestamps:   { created_at: null, last_activity_at: null },
    };
    const renderer = renderRoute("/runs/run_1/sandbox?mode=vnc");
    const tabs = renderer.root.findAll(
      (node) => node.type === "button" && node.props.role === "tab",
    );
    const selected = tabs.find((tab) => tab.props["aria-selected"] === true);
    expect(selected?.children.find((c) => typeof c === "string")).toBe("Terminal");
  });

  test("Filesystem mode keeps sandbox details visible in the left column", () => {
    currentDetails = {
      provider:     "docker",
      name:         "fabro-run-abc",
      id:           null,
      state:        "running",
      native_state: null,
      region:       null,
      image:        null,
      resources:    { cpu_cores: null, memory_bytes: null, disk_bytes: null },
      labels:       {},
      timestamps:   { created_at: null, last_activity_at: null },
    };
    const renderer = renderRoute("/runs/run_1/sandbox?mode=filesystem");

    const panelHeadings = renderer.root
      .findAll((node) => node.type === "h3")
      .map((node) => node.children.find((child) => typeof child === "string"))
      .filter((text): text is string => typeof text === "string");
    expect(panelHeadings).toEqual(["Overview", "Resources", "Labels", "Timestamps"]);

    const tabs = renderer.root.findAll(
      (node) => node.type === "button" && node.props.role === "tab",
    );
    const selected = tabs.find((tab) => tab.props["aria-selected"] === true);
    expect(selected?.children.find((c) => typeof c === "string")).toBe("Filesystem");
  });
});

describe("normalizeSandboxMode", () => {
  test("defaults to terminal", () => {
    expect(normalizeSandboxMode(null)).toBe("terminal");
    expect(normalizeSandboxMode("")).toBe("terminal");
    expect(normalizeSandboxMode("unknown")).toBe("terminal");
  });

  test("accepts filesystem and vnc", () => {
    expect(normalizeSandboxMode("filesystem")).toBe("filesystem");
    expect(normalizeSandboxMode("vnc")).toBe("vnc");
  });
});
