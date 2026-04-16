# Why this stack (short version)

## MCP is the transport, not the product

This repo uses MCP to connect tools to clients, but the differentiator is the harness design: orchestrator sessions, constrained agent roles, and scoped tool surfaces.

For the detailed architecture, see [`../workflow.md`](../workflow.md).

## Designed constraints, not tool bloat

Key themes:

- Scoped tool allowlists and role-specific agents
- Structured `just` workflows instead of default shell access
- Review tooling isolation and explicit session orchestration

See the diagrams and tool matrices in [`../workflow.md`](../workflow.md).

## A practical loop: research -> plan -> implement

The repo's workflow is encoded as explicit stages rather than ad hoc prompting.

Start with [`../workflow.md`](../workflow.md) for the full map.
