import { ThreadPrimitive } from "@assistant-ui/react";

/**
 * Example prompts shown on the empty Ask-Fabro thread. Each `prompt` is sent
 * verbatim as the first message; `heading`/`description` are display-only.
 */
const EXAMPLE_PROMPTS = [
  {
    heading: "Surface errors",
    description: "Find errors, warnings, and failed steps in this run.",
    prompt:
      "Walk me through any errors, warnings, or failed steps in this run and what caused them.",
  },
  {
    heading: "Analyze performance",
    description: "Spot the slowest stages and where time was spent.",
    prompt:
      "Which stages were the slowest, and where did this run spend most of its time?",
  },
  {
    heading: "Review key decisions",
    description: "Recap the important choices the agent made.",
    prompt:
      "What were the key decisions the agent made during this run, and why?",
  },
  {
    heading: "Suggest improvements",
    description: "Ideas to make this workflow faster and more reliable.",
    prompt:
      "Suggest improvements to make this workflow faster and more reliable.",
  },
];

/**
 * Empty-state for the Ask-Fabro sidebar. Wrapped in `ThreadPrimitive.Empty` so
 * it renders only before the first message; clicking an example sends its
 * prompt, which starts the session.
 */
export default function SidebarWelcome() {
  return (
    <ThreadPrimitive.Empty>
      <div className="flex flex-col gap-6 px-4 py-8">
        <h2 className="text-base font-semibold text-fg">How can I help?</h2>
        <ul role="list" className="flex flex-col gap-3">
          {EXAMPLE_PROMPTS.map((example) => (
            <li key={example.heading}>
              <ThreadPrimitive.Suggestion asChild prompt={example.prompt} send>
                <button
                  type="button"
                  className="flex w-full flex-col gap-1 rounded-xl bg-panel-alt/60 px-4 py-3.5 text-left ring-1 ring-line transition-colors hover:bg-panel-alt hover:ring-line-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-teal-500"
                >
                  <p className="text-sm font-medium text-fg">
                    {example.heading}
                  </p>
                  <p className="text-xs text-fg-3">{example.description}</p>
                </button>
              </ThreadPrimitive.Suggestion>
            </li>
          ))}
        </ul>
      </div>
    </ThreadPrimitive.Empty>
  );
}
