# USER_NEEDS.md

## 1. Executive Summary

`fathomdb` exists to serve **local AI agents that maintain an ongoing relationship with a human and their work**. In this domain, the datastore is not only a place to put documents. It must help the system remember what matters, connect related information, support action over time, and remain inspectable and reversible when the agent is wrong.

The key user need is a datastore that can support a durable world model for the human and agent while also supporting multimodal recall: structured facts, relationships, full-text lookup, semantic similarity, temporal context, provenance, and operational history. It must be fast enough for interactive use, safe enough for high-trust personal workflows, and simple enough to run locally.

## 2. Who This Serves

### 2.1 The Human Principal

The human relies on the agent to manage sensitive, interconnected personal and professional information such as notes, goals, schedules, meetings, email-derived work, and long-running projects.

The human needs:

- strong privacy and local control over data
- fast recall and continuity across sessions and interfaces
- a way to inspect why the agent answered, acted, or stored something
- the ability to approve, reject, correct, or undo harmful changes
- confidence that the system will not silently lose or distort important context

### 2.2 The Agent Runtime

The agent is the primary operator of the datastore. It works under token limits, imperfect reasoning, and pressure to act across many kinds of information.

The agent needs:

- deterministic, programmatic access patterns instead of brittle bespoke query strings
- a unified way to work with documents, relationships, semantic similarity, full-text lookup, and temporal state
- help with ingestion, indexing, and housekeeping so it does not have to manually synchronize multiple memory surfaces
- a durable place to store intent, actions, observations, corrections, outcomes, and learned context over time

### 2.3 The Application Developer And Operator

The surrounding application needs a datastore that is embeddable, observable, and maintainable.

The developer or operator needs:

- a zero-ops local deployment model
- reliable schema evolution and repair paths
- visibility into failures, degraded modes, and behavioral regressions
- replay and auditability for debugging, evaluation, and trust

## 3. Domain Context

The target system is a local-first human-agent partnership rather than a stateless chatbot. Systems in this space typically:

- run through multiple interfaces such as CLI, TUI, chat, messaging, MCP, and meeting workflows
- ingest mixed sources such as notes, URLs, files, email, calendars, transcripts, and system events
- track long-lived goals, tasks, commitments, reminders, and blocked work
- perform background work through tools, schedulers, queues, and review flows
- need to balance fast retrieval, durable memory, human approval, and error recovery

Because of this, the datastore must support both **recall** and **governed action over time**.

## 4. Core User Needs

### 4.1 Persistent World Model

The system needs more than a pile of saved documents. It needs durable memory of:

- the human's preferences, goals, working context, and recurring patterns
- the agent's own state, limitations, and recent activity
- ongoing tasks, plans, commitments, and blockers
- observations and events that change what the agent should do next
- what has been learned, corrected, promoted, or superseded over time

### 4.2 Multi-Modal Recall And Reasoning

The agent must be able to retrieve and combine:

- structured records and typed facts
- document content and attached metadata
- graph-like relationships between people, projects, events, tasks, and ideas
- full-text matches
- semantic similarity matches
- temporal context such as recency, chronology, and session continuity
- provenance about where information came from and how trustworthy it is

This is a user need because agent questions are rarely only lexical or only relational. They often combine all of these at once.

### 4.3 Deterministic Agent Ergonomics

Agents do not perform well when forced to invent fragile query dialects or manually orchestrate multiple stores.

The datastore should support:

- deterministic, code-friendly interaction patterns
- clear data shapes and predictable access patterns
- minimal boilerplate for common agent tasks
- safe defaults for reading, writing, updating, and promoting memory

This is not only a developer preference. It directly affects agent correctness and reliability.

### 4.4 Automated Housekeeping

The agent should not have to think about low-level synchronization work every time it writes memory.

The system needs built-in help for:

- ingesting raw content from many sources
- maintaining searchable and semantically useful memory
- keeping related memory surfaces consistent
- promoting raw observations into durable knowledge, tasks, decisions, and commitments
- repairing or rebuilding derived memory surfaces when needed

### 4.5 Prompt-Control, Governance, And Trust

A local AI agent must store more than end-state knowledge. It must preserve the reasoning and control context around action.

The system needs durable records of:

- what the human asked
- how the request was interpreted
- whether the system was uncertain, ambiguous, or high-risk
- what route, policy, or response contract was chosen
- whether clarification, abstention, approval, or escalation was required
- what memory writes were attempted or suppressed

Without this, the human cannot understand or trust the system's behavior.

### 4.6 Provenance, Replay, And Evaluation

The system needs to answer questions such as:

- Why did the agent say or do this?
- What source caused this fact or task to appear?
- What changed after a policy or routing update?
- Which layer failed: interpretation, retrieval, validation, tool execution, or answer quality?

That means the datastore must support:

- source tracking
- temporal history
- correction history
- replayable interaction traces
- explicit and implicit feedback
- evaluation and comparison of behavior over time

### 4.7 Reversibility And Human Control

Autonomous agents will make mistakes. The datastore must support:

- selective undo rather than only irreversible mutation
- correction without losing history
- branch-like or proposal-oriented workflows for risky changes
- approvals for high-impact actions
- durable records of what was accepted, rejected, or revised

This is a core human trust requirement.

### 4.8 Operational Continuity

The datastore must support a system that works across:

- interactive chat and review loops
- background scheduling and reminders
- meeting ingestion and follow-up workflows
- notifications, approvals, and blocked-item review
- audits, logs, and self-diagnostic surfaces

This is necessary because the agent is not just answering questions. It is participating in ongoing work.

## 5. Non-Functional Needs

- **Privacy and locality:** data should live on the user's device or private infrastructure by default.
- **Low operational burden:** the system should be easy to embed and run without heavyweight infrastructure.
- **Interactive speed:** recall and writes must be responsive enough for conversational use.
- **Reliability:** failures should be visible and recoverable rather than silent or corrupting.
- **Portability:** the datastore should move cleanly between common local environments such as a laptop or home server.
- **Low resource footprint:** it must coexist with local inference and normal application workloads.
- **Scalable enough for personal knowledge growth:** it should remain practical as years of agent memory accumulate.

## 6. Important Memory Layers

Not all memory should be treated the same. The system needs distinctions between:

- ephemeral turn state
- session continuity state
- durable semantic memory
- learned preferences and interaction patterns
- correction history
- intentionally non-persistent or suppressed artifacts

This is necessary to avoid both amnesia and unbounded memory pollution.

## 7. Failure Modes The Datastore Must Make Visible Or Recoverable

- harmful or hallucinated writes with no practical undo path
- missing or distorted provenance for facts, tasks, or commitments
- inability to tell whether failure came from interpretation, retrieval, tool use, or policy
- silent loss of synchronization between different memory surfaces
- poor retrieval of relevant context across text, relationships, semantics, and time
- history being overwritten instead of corrected transparently
- background ingestion or scheduler failures that disappear without reviewable traces
- latency or resource spikes that make the agent unusable in interactive workflows

## 8. Summary

The user need is a **local, high-trust datastore for persistent AI agents**. It must support multimodal recall, durable world modeling, deterministic agent ergonomics, automated housekeeping, provenance, replay, evaluation, approvals, and reversible memory. If it only stores documents or only accelerates search, it does not solve the real problem space.
