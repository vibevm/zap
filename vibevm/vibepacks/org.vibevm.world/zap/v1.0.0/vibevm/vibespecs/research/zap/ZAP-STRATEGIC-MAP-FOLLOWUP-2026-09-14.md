# Strategic map structure: deferred Owner commission

Status: authorized follow-up; saved now, research and implementation deferred
until the already planned main ZAP Rust development campaign is complete.

Source: OWNER-STRATEGIC-MAP-INPUT-2026-09-14.txt preserves the Owner's Russian
request. This document is an English planning brief, not an accepted algorithm
or a change to current product requirements.

## Question to investigate

The Owner proposes a strategic-game metaphor, especially Heroes III. A useful
map has a comprehensible set of important towns or castles, routes and obstacles
between them, smaller actionable objects, and resources that support exploration.
It is not an indefinitely expanding flat collection of tiny objectives.

Investigate whether this structure helps neural agents and ZAP, as well as
humans. Game popularity and the proposed similarity to human thinking are
hypotheses, not evidence that this is an effective planning algorithm.

Candidate concepts, with terminology open to revision:

- Milestones: substantial, observable outcomes, analogous to towns or castles.
- Tasks: smaller executable objectives and obstacles along the route.
- Resources: research, questions, evidence, capabilities or other opportunities
  that may improve a decision or exploration without directly completing an
  objective.
- Routes and agents: dependencies, alternative approaches and actual execution
  paths that could later be represented as heroes moving through the map.

During initial planning and every adaptive revision, examine how ZAP should
discover, revise and relate these structures while updating uncertainty, value,
cost and the known frontier. Address uncontrolled task proliferation and loss
of the important outcomes without imposing arbitrary product size limits.

## Required work after the main campaign

1. Review the idea critically against the implemented ZAP algorithm and its
   existing work, strategy, lowering, obligation, resource and evidence types.
   Distinguish genuinely useful new meaning from a visual rename of existing
   structures. Investigate effects on neural-agent execution, not only appeal
   to a human viewer.
2. Write a substantial English design review that the Owner can read while work
   continues. Explain evidence, competing approaches, the proposed algorithm,
   tradeoffs, uncertainty and reasons to adopt, modify or reject the metaphor.
3. If justified, design and implement the selected concepts and adaptive
   behavior under the already authorized autonomous workflow. Owner review of
   the design document is not a prerequisite to implementation. Preserve
   existing authority, economics, stop conditions, proof and recovery rules.
4. Prefer explicit, mechanical graph transformations with stable identities,
   migration/evidence records and machine-readable events. Use the existing
   campaign graph as a pilot where lawful; do not activate NEXT execution.
5. Describe the data and event mapping for a future game-like canvas: milestone
   towns, task objects, helpful resources, routes, fog and agents as heroes.
   A future visualization should reflect real algorithmic state and actions.
   This request does not start a Qwik/3D frontend during the current core work.

## Continuation and authority

Do not interrupt or replace the current Rust MVP work with this research.
After the main campaign, return to this commission automatically; the earlier
instruction to stop and wait after that campaign is superseded for this exact
follow-up. The Owner explicitly authorizes implementation if the review finds
the approach suitable, without waiting for their return or a new approval.
Maintain the standing coding-worker selection (Sol/xhigh), protocol test-worker
selection (Sol/medium), native delegation and frequent filesystem checkpoints.
Do not silently treat the metaphor or proposed names as already accepted facts.
