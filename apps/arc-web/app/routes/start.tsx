import { useState, useRef, useEffect } from "react";
import {
  Listbox,
  ListboxButton,
  ListboxOption,
  ListboxOptions,
} from "@headlessui/react";
import { ArrowUpIcon } from "@heroicons/react/24/solid";
import {
  ChevronUpDownIcon,
  FolderIcon,
} from "@heroicons/react/16/solid";
import {
  BoltIcon,
  BugAntIcon,
  CodeBracketIcon,
  MagnifyingGlassIcon,
  PencilSquareIcon,
  ShieldCheckIcon,
  XMarkIcon,
} from "@heroicons/react/24/outline";
import { Link } from "react-router";
import type { Route } from "./+types/start";

export const handle = { hideHeader: true, wide: true };

export function meta({}: Route.MetaArgs) {
  return [{ title: "Start — Arc" }];
}

const projects = [
  { id: "arc-web", name: "arc-web" },
  { id: "arc-attractor", name: "arc-attractor" },
  { id: "arc-cli", name: "arc-cli" },
];

const branches = [
  { id: "main", name: "main" },
  { id: "develop", name: "develop" },
  { id: "feature/start-page", name: "feature/start-page" },
];

const sessionGroups = [
  {
    label: "Today",
    sessions: [
      { id: "s1", title: "Add rate limiting to auth endpoints", repo: "api-server", time: "2h ago" },
      { id: "s2", title: "Fix config parsing for nested values", repo: "cli-tools", time: "4h ago" },
    ],
  },
  {
    label: "Yesterday",
    sessions: [
      { id: "s3", title: "Migrate to React Router v7", repo: "web-dashboard", time: "1d ago" },
      { id: "s4", title: "Add dark mode toggle", repo: "web-dashboard", time: "1d ago" },
      { id: "s5", title: "Update OpenAPI spec for v3", repo: "api-server", time: "1d ago" },
    ],
  },
  {
    label: "Previous 7 days",
    sessions: [
      { id: "s6", title: "Terraform module for Redis cluster", repo: "infrastructure", time: "3d ago" },
      { id: "s7", title: "Add pipeline event types", repo: "shared-types", time: "5d ago" },
      { id: "s8", title: "Implement webhook retry logic", repo: "api-server", time: "6d ago" },
    ],
  },
];

function BranchIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" className={className}>
      <path d="M9.5 3.25a2.25 2.25 0 1 1 3 2.122V6A2.5 2.5 0 0 1 10 8.5H6a1 1 0 0 0-1 1v1.128a2.251 2.251 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.5 0v1.836A2.5 2.5 0 0 1 6 7h4a1 1 0 0 0 1-1v-.628A2.25 2.25 0 0 1 9.5 3.25Zm-6 0a.75.75 0 1 0 1.5 0 .75.75 0 0 0-1.5 0Zm8.25-.75a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5ZM4.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Z" />
    </svg>
  );
}

function SessionSidebar() {
  return (
    <aside className="w-64 shrink-0 border-r border-white/[0.06] flex flex-col h-[calc(100vh-4rem)]">
      <div className="p-3">
        <div className="flex w-full items-center gap-2 rounded-lg border border-teal-500/20 bg-navy-800/60 px-3 py-2 text-sm text-ice-100">
          <PencilSquareIcon className="size-4 text-teal-500" />
          New session
        </div>
      </div>
      <nav className="flex-1 overflow-y-auto px-3 pb-4">
        {sessionGroups.map((group) => (
          <div key={group.label} className="mt-4 first:mt-1">
            <p className="px-2 mb-1.5 text-[11px] font-medium uppercase tracking-wider text-navy-600">
              {group.label}
            </p>
            <ul className="space-y-0.5">
              {group.sessions.map((session) => (
                <li key={session.id}>
                  <Link
                    to={`/sessions/${session.id}`}
                    className="flex w-full flex-col rounded-lg px-2.5 py-2 text-left transition-colors text-ice-300 hover:bg-white/[0.04]"
                  >
                    <span className="truncate text-sm">{session.title}</span>
                    <span className="flex items-center gap-1.5 mt-0.5">
                      <span className="font-mono text-[11px] text-teal-500">{session.repo}</span>
                      <span className="text-[11px] text-navy-600">{session.time}</span>
                    </span>
                  </Link>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </nav>
    </aside>
  );
}

export default function Start() {
  const [prompt, setPrompt] = useState("");
  const [project, setProject] = useState(projects[0]);
  const [branch, setBranch] = useState(branches[0]);
  const [openCategory, setOpenCategory] = useState<string | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  function autoResize() {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, 280) + "px";
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (prompt.trim()) handleSubmit();
    }
  }

  function handleSubmit() {
    if (!prompt.trim()) return;
    // TODO: wire up submission
  }

  return (
    <div className="flex -mx-4 sm:-mx-6 lg:-mx-8 -my-6">
      <SessionSidebar />

      <div className="flex-1 flex flex-col items-center pt-[12vh] px-4">
        <div className="w-full max-w-2xl">
          <h1 className="flex items-center justify-center gap-3 text-[2rem] font-medium tracking-tight text-ice-100 text-center mb-8">
            <img src="/logo.svg" alt="" className="size-9" />
            What do you want to build?
          </h1>

          <div className="relative group">
            <div className="absolute -inset-px rounded-xl bg-gradient-to-b from-teal-500/30 to-mint/20 opacity-0 blur-sm transition-opacity duration-300 group-focus-within:opacity-100" />

            <div className="relative rounded-xl bg-navy-800 border border-white/[0.08] group-focus-within:border-teal-500/40 transition-colors duration-300">
              <textarea
                ref={textareaRef}
                value={prompt}
                onChange={(e) => {
                  setPrompt(e.target.value);
                  autoResize();
                }}
                onKeyDown={handleKeyDown}
                placeholder="Describe a workflow, pipeline, or automation..."
                rows={3}
                className="w-full resize-none bg-transparent px-5 pt-4 pb-14 text-[15px] leading-relaxed text-ice-100 placeholder:text-navy-600 focus:outline-none"
              />

              <div className="absolute bottom-3 inset-x-3 flex items-center justify-between">
                <div className="flex items-center gap-1.5">
                  <Picker
                    value={project}
                    onChange={setProject}
                    options={projects}
                    icon={<FolderIcon className="size-3.5 text-navy-600" />}
                  />
                  <Picker
                    value={branch}
                    onChange={setBranch}
                    options={branches}
                    icon={<BranchIcon className="size-3.5 text-navy-600" />}
                  />
                </div>

                <div className="flex items-center gap-3">
                  <span className="text-xs text-navy-600 select-none">
                    <kbd className="font-mono">Enter</kbd> to submit
                  </span>
                  <button
                    onClick={handleSubmit}
                    disabled={!prompt.trim()}
                    className="flex items-center justify-center size-8 rounded-lg bg-teal-500 text-navy-950 transition-all duration-200 hover:bg-teal-300 disabled:opacity-30 disabled:hover:bg-teal-500 cursor-pointer disabled:cursor-default"
                  >
                    <ArrowUpIcon className="size-4" />
                  </button>
                </div>
              </div>
            </div>
          </div>

          <div className="relative mt-5">
            <div className="flex items-center justify-center gap-2">
              {categories.map((cat) => (
                <button
                  key={cat.label}
                  onClick={() => setOpenCategory(openCategory === cat.label ? null : cat.label)}
                  className={`inline-flex items-center gap-2 rounded-full border px-4 py-2 text-sm transition-colors cursor-pointer ${
                    openCategory === cat.label
                      ? "border-teal-500/30 bg-teal-500/10 text-teal-300"
                      : "border-white/[0.06] bg-navy-800/50 text-ice-300 hover:bg-navy-800 hover:border-white/[0.12]"
                  }`}
                >
                  <cat.icon className="size-4" />
                  {cat.label}
                </button>
              ))}
            </div>

            {openCategory && (
              <div className="absolute inset-x-0 top-0 z-10">
                <CategoryPanel
                  category={categories.find((c) => c.label === openCategory)!}
                  onClose={() => setOpenCategory(null)}
                  onSelect={(p) => {
                    setPrompt(p);
                    setOpenCategory(null);
                    textareaRef.current?.focus();
                    setTimeout(() => {
                      const el = textareaRef.current;
                      if (!el) return;
                      el.style.height = "auto";
                      el.style.height = Math.min(el.scrollHeight, 280) + "px";
                    }, 0);
                  }}
                />
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

interface Category {
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  items: { title: string; prompt: string }[];
}

const categories: Category[] = [
  {
    label: "Build",
    icon: CodeBracketIcon,
    items: [
      { title: "Implement a new feature", prompt: "Implement a new feature that adds user authentication with OAuth2, including login, logout, and session management." },
      { title: "Create an API endpoint", prompt: "Create a new REST API endpoint with request validation, error handling, and proper HTTP status codes." },
      { title: "Set up a CI/CD pipeline", prompt: "Set up a CI/CD pipeline with build, test, lint, and deploy stages for the main branch." },
      { title: "Add database migrations", prompt: "Add database migrations to create the new tables and indexes needed for the upcoming feature." },
    ],
  },
  {
    label: "Review",
    icon: MagnifyingGlassIcon,
    items: [
      { title: "Review a pull request", prompt: "Review the latest pull request for bugs, security vulnerabilities, and code style issues. Summarize findings and suggest fixes." },
      { title: "Audit dependencies", prompt: "Audit all project dependencies for known vulnerabilities, outdated versions, and unused packages." },
      { title: "Analyze test coverage", prompt: "Analyze the current test coverage, identify untested code paths, and recommend which areas need tests most." },
      { title: "Check for security issues", prompt: "Scan the codebase for common security vulnerabilities including injection, XSS, and authentication flaws." },
    ],
  },
  {
    label: "Fix",
    icon: BugAntIcon,
    items: [
      { title: "Debug a failing test", prompt: "Debug the failing test suite, identify the root cause of each failure, and apply fixes." },
      { title: "Fix a production bug", prompt: "Investigate and fix the reported production bug, including root cause analysis and a regression test." },
      { title: "Resolve merge conflicts", prompt: "Resolve the merge conflicts in the current branch, preserving the intended changes from both sides." },
      { title: "Fix type errors", prompt: "Fix all TypeScript type errors in the project, ensuring strict type safety without using any type assertions." },
    ],
  },
];

function CategoryPanel({
  category,
  onClose,
  onSelect,
}: {
  category: Category;
  onClose: () => void;
  onSelect: (prompt: string) => void;
}) {
  return (
    <div className="rounded-xl border border-white/[0.08] bg-navy-800 overflow-hidden">
      <div className="flex items-center gap-2 px-4 py-3 border-b border-white/[0.06]">
        <category.icon className="size-4 text-teal-500" />
        <span className="text-sm font-medium text-ice-100">{category.label}</span>
        <button
          onClick={onClose}
          className="ml-auto flex items-center justify-center size-6 rounded-md text-navy-600 hover:text-ice-300 hover:bg-white/[0.06] transition-colors cursor-pointer"
        >
          <XMarkIcon className="size-4" />
        </button>
      </div>
      <ul>
        {category.items.map((item, i) => (
          <li key={item.title} className={i > 0 ? "border-t border-white/[0.04]" : ""}>
            <button
              onClick={() => onSelect(item.prompt)}
              className="w-full px-4 py-3 text-left text-sm text-ice-300 transition-colors hover:bg-white/[0.03] hover:text-ice-100 cursor-pointer"
            >
              {item.title}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

function Picker<T extends { id: string; name: string }>({
  value,
  onChange,
  options,
  icon,
}: {
  value: T;
  onChange: (v: T) => void;
  options: T[];
  icon: React.ReactNode;
}) {
  return (
    <Listbox value={value} onChange={onChange}>
      <div className="relative">
        <ListboxButton className="flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs text-ice-300 bg-navy-950/60 border border-white/[0.06] hover:border-white/[0.12] hover:bg-navy-950/80 transition-colors cursor-pointer">
          {icon}
          <span className="max-w-[120px] truncate">{value.name}</span>
          <ChevronUpDownIcon className="size-3.5 text-navy-600" />
        </ListboxButton>

        <ListboxOptions anchor="top start" className="z-20 w-56 rounded-lg bg-navy-800 border border-white/[0.08] py-1 shadow-xl shadow-black/30 focus:outline-none [--anchor-gap:4px]">
          {options.map((option) => (
            <ListboxOption
              key={option.id}
              value={option}
              className="flex items-center gap-2 px-3 py-1.5 text-xs text-ice-300 cursor-pointer data-focus:bg-white/[0.06] data-selected:text-teal-300"
            >
              {option.name}
            </ListboxOption>
          ))}
        </ListboxOptions>
      </div>
    </Listbox>
  );
}
