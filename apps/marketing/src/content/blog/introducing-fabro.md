---
title: "Introducing Fabro"
description: "Fabro is an open-source workflow orchestration platform that lets expert engineers define their process as a graph and let AI agents execute it."
date: 2026-03-16
author: "Fabro Team"
---

Most AI coding tools give you a chat window and hope for the best. Fabro takes a different approach: you define your engineering process as a workflow graph, and Fabro executes it — with verification gates, human checkpoints, and full observability at every step.

## Why workflows?

A chat-based agent is a single loop: prompt, act, repeat. That works for small tasks, but it falls apart when you need structure — when the plan should be approved before implementation begins, when tests must pass before the PR is opened, when a second model should cross-review the first.

Fabro workflows are Graphviz graphs. Each node is a stage with a specific role: planning, coding, reviewing, testing. Edges define the flow. Human-in-the-loop gates let you intervene where it matters and step back where it doesn't.

## Multi-model by design

Not every stage needs a frontier model. Fabro's CSS-like model stylesheets let you assign models by stage ID, class, or shape — so you can use fast models for triage and frontier models for the work that matters. Swap providers without changing your workflow.

## What's next

We're just getting started. Check the [roadmap](/roadmap) to see what we're building, and join us on [Discord](/discord) to shape what comes next.
