/**
 * Deterministic semantic placement computed before display filtering.
 * @scope spec://org.vibevm.zap/lens/PROP-004#root
 */
import type {
  QuicklensRef,
  QuicklensSnapshot,
  SemanticObject,
  SemanticRelationship,
} from "../quicklens-model/index.ts";

export type GraphView = "goal" | "work_order";
export type GraphEdgeClass = "execution" | "hierarchy" | "resource" | "reference" | "other";
export type GraphNodeRole =
  | "goal"
  | "strategy"
  | "member"
  | "branch"
  | "focus"
  | "current"
  | "start"
  | "step"
  | "cycle"
  | "unresolved"
  | "unassigned"
  | "context";

export interface GraphLayoutDiagnostic {
  readonly code: "missing_goal" | "partial" | "missing_prerequisite" | "cycle" | "disconnected";
  readonly message: string;
}

export interface GraphLayout {
  readonly view: GraphView;
  readonly positions: ReadonlyMap<QuicklensRef, { readonly x: number; readonly y: number }>;
  readonly roles: ReadonlyMap<QuicklensRef, GraphNodeRole>;
  readonly stages: ReadonlyMap<QuicklensRef, number>;
  readonly diagnostics: readonly GraphLayoutDiagnostic[];
}

const EXECUTION_KINDS = new Set([
  "work_prerequisite",
  "milestone_preparation_prerequisite",
  "milestone_achievement_prerequisite",
]);
const HIERARCHY_KINDS = new Set(["contains", "outcome_scope", "contributes_to"]);
const RESOURCE_KINDS = new Set(["consumes", "resource_use"]);
const REFERENCE_KINDS = new Set([
  "acceptance_duty",
  "applies_to",
  "derived_from",
  "evidence_reference",
  "read_subject",
  "source_reference",
  "supports",
  "verifies",
  "write_subject",
]);

export function classifyGraphEdge(relationship: SemanticRelationship): GraphEdgeClass {
  const kind = normalizeKind(relationship.semanticType);
  if (EXECUTION_KINDS.has(kind)) return "execution";
  if (HIERARCHY_KINDS.has(kind)) return "hierarchy";
  if (RESOURCE_KINDS.has(kind)) return "resource";
  if (REFERENCE_KINDS.has(kind)) return "reference";
  return "other";
}

/** @implements spec://org.vibevm.zap/lens/PROP-004#partial-and-stability */
export function layoutQuicklensGraph(snapshot: QuicklensSnapshot, view: GraphView): GraphLayout {
  return view === "goal" ? goalLayout(snapshot) : workOrderLayout(snapshot);
}

export function graphNodeLabel(object: SemanticObject, layout: GraphLayout): string {
  const role = layout.roles.get(object.ref) ?? "context";
  const stage = layout.stages.get(object.ref);
  if (role === "goal") return `Goal · ${object.title}`;
  if (role === "focus") return `Focus · ${object.title}`;
  if (role === "current") {
    return `${stage === undefined ? "Current" : `Current · Step ${String(stage + 1)}`} · ${object.title}`;
  }
  if (role === "start") return `Start · Step 1 · ${object.title}`;
  if (role === "step" && stage !== undefined) return `Step ${String(stage + 1)} · ${object.title}`;
  if (role === "cycle") return `Cycle · order unknown · ${object.title}`;
  if (role === "unresolved") return `Missing prerequisite · ${object.title}`;
  if (role === "strategy") return `Strategy context · ${object.title}`;
  if (role === "member") return `Milestone · ${object.title}`;
  if (role === "branch") return `Branch · ${object.title}`;
  if (role === "unassigned") return `Unassigned · ${object.title}`;
  return `Context · ${object.title}`;
}

export function relationshipVisibleInGraph(
  relationship: SemanticRelationship,
  view: GraphView,
): boolean {
  const edgeClass = classifyGraphEdge(relationship);
  return view === "work_order"
    ? edgeClass === "execution"
    : edgeClass === "hierarchy" || edgeClass === "resource" || edgeClass === "reference";
}

export function graphEdgeLabel(relationship: SemanticRelationship, layout: GraphLayout): string {
  if (layout.view === "goal") return "";
  if (classifyGraphEdge(relationship) !== "execution") return relationship.label;
  const from = layout.stages.get(relationship.source);
  const to = layout.stages.get(relationship.target);
  return from === undefined || to === undefined
    ? "Prerequisite → dependent · order unresolved"
    : `Step ${String(from + 1)} → ${String(to + 1)} · prerequisite`;
}

function goalLayout(snapshot: QuicklensSnapshot): GraphLayout {
  const positions = new Map<QuicklensRef, { readonly x: number; readonly y: number }>();
  const roles = new Map<QuicklensRef, GraphNodeRole>();
  const stages = new Map<QuicklensRef, number>();
  const diagnostics = partialDiagnostics(snapshot);
  const objects = sortedObjects(snapshot);
  const present = new Set(objects.map((object) => object.ref));
  const navigation = snapshot.navigation;
  const members = new Set(navigation?.members.map((member) => member.ref) ?? []);
  const goal = navigation?.activeOutcomeRef ?? null;
  if (goal === null || !present.has(goal)) {
    diagnostics.push({
      code: "missing_goal",
      message:
        "No loaded object is identified as the active goal; positions are unordered context.",
    });
    placeBand(
      objects.map((object) => object.ref),
      positions,
      0,
    );
    for (const object of objects) roles.set(object.ref, "context");
    return { view: "goal", positions, roles, stages, diagnostics };
  }

  positions.set(goal, { x: 0, y: 0 });
  const assigned = placeGoalBranches(snapshot, members, present, positions);
  assigned.add(goal);
  const context = objects.filter((object) => !assigned.has(object.ref)).map((object) => object.ref);
  placeGoalContext(context, positions);
  const unassignedWork = new Set(
    context.filter((ref) => objects.find((object) => object.ref === ref)?.category === "task"),
  );
  if (unassignedWork.size > 0) {
    diagnostics.push({
      code: "disconnected",
      message:
        "Some work has no single resolved plan-milestone parent and stays explicitly unassigned.",
    });
  }
  for (const object of objects) {
    const ref = object.ref;
    if (ref === goal) roles.set(ref, "goal");
    else if (ref === navigation?.focusRef) roles.set(ref, "focus");
    else if (navigation?.currentWorkRefs.includes(ref)) roles.set(ref, "current");
    else if (ref === navigation?.currentStrategyRef) roles.set(ref, "strategy");
    else if (members.has(ref)) roles.set(ref, "member");
    else if (assigned.has(ref)) roles.set(ref, "branch");
    else if (unassignedWork.has(ref)) roles.set(ref, "unassigned");
    else roles.set(ref, "context");
  }
  return { view: "goal", positions, roles, stages, diagnostics };
}

function placeGoalBranches(
  snapshot: QuicklensSnapshot,
  members: ReadonlySet<QuicklensRef>,
  present: ReadonlySet<QuicklensRef>,
  positions: Map<QuicklensRef, { readonly x: number; readonly y: number }>,
): Set<QuicklensRef> {
  const assigned = new Set<QuicklensRef>();
  const memberList = [...members].filter((ref) => present.has(ref)).sort();
  const parents = new Map<QuicklensRef, Set<QuicklensRef>>();
  for (const relationship of snapshot.relationships) {
    if (
      normalizeKind(relationship.semanticType) === "contributes_to" &&
      members.has(relationship.target) &&
      present.has(relationship.source)
    ) {
      const targets = parents.get(relationship.source) ?? new Set<QuicklensRef>();
      targets.add(relationship.target);
      parents.set(relationship.source, targets);
    }
  }
  memberList.forEach((member, index) => {
    const angle = memberList.length === 1 ? 0 : Math.PI - (index / memberList.length) * Math.PI * 2;
    positions.set(member, { x: Math.cos(angle) * 0.62, y: Math.sin(angle) * 0.62 });
    assigned.add(member);
    const children = [...parents]
      .filter(([, targets]) => targets.size === 1 && targets.has(member))
      .map(([child]) => child)
      .sort();
    children.forEach((child, childIndex) => {
      const offset =
        children.length === 1
          ? index % 2 === 0
            ? 0.32
            : -0.32
          : spread(childIndex, children.length, Math.min(0.42, Math.PI / 7));
      positions.set(child, {
        x: Math.cos(angle + offset) * 1.18,
        y: Math.sin(angle + offset) * 1.18,
      });
      assigned.add(child);
    });
  });
  return assigned;
}

function workOrderLayout(snapshot: QuicklensSnapshot): GraphLayout {
  const positions = new Map<QuicklensRef, { readonly x: number; readonly y: number }>();
  const roles = new Map<QuicklensRef, GraphNodeRole>();
  const stages = new Map<QuicklensRef, number>();
  const diagnostics = partialDiagnostics(snapshot);
  const objects = sortedObjects(snapshot);
  const present = new Set(objects.map((object) => object.ref));
  const execution = snapshot.relationships.filter(
    (relationship) => classifyGraphEdge(relationship) === "execution",
  );
  const workRefs = new Set<QuicklensRef>(
    objects.filter((object) => object.category === "task").map((object) => object.ref),
  );
  for (const relationship of execution) {
    if (present.has(relationship.source)) workRefs.add(relationship.source);
    if (present.has(relationship.target)) workRefs.add(relationship.target);
  }
  const outgoing = adjacency(workRefs);
  const incoming = new Map([...workRefs].map((ref) => [ref, 0]));
  const unresolved = new Set<QuicklensRef>();
  let missingCount = 0;
  for (const relationship of execution) {
    const sourcePresent = workRefs.has(relationship.source);
    const targetPresent = workRefs.has(relationship.target);
    if (!sourcePresent || !targetPresent) missingCount += 1;
    if (!sourcePresent && targetPresent) unresolved.add(relationship.target);
    if (sourcePresent && targetPresent) {
      const dependents = outgoing.get(relationship.source);
      if (dependents !== undefined && !dependents.has(relationship.target)) {
        dependents.add(relationship.target);
        incoming.set(relationship.target, (incoming.get(relationship.target) ?? 0) + 1);
      }
    }
  }
  propagateUnresolved(unresolved, outgoing);
  if (missingCount > 0) {
    diagnostics.push({
      code: "missing_prerequisite",
      message: `${String(missingCount)} prerequisite relation(s) have an unloaded endpoint; affected order stays unknown.`,
    });
  }
  const queue = [...workRefs]
    .filter((ref) => !unresolved.has(ref) && incoming.get(ref) === 0)
    .sort();
  for (const ref of queue) stages.set(ref, 0);
  const processed = new Set<QuicklensRef>();
  while (queue.length > 0) {
    const ref = queue.shift();
    if (ref === undefined) break;
    processed.add(ref);
    for (const dependent of [...(outgoing.get(ref) ?? [])].sort()) {
      if (unresolved.has(dependent)) continue;
      stages.set(dependent, Math.max(stages.get(dependent) ?? 0, (stages.get(ref) ?? 0) + 1));
      const remaining = (incoming.get(dependent) ?? 0) - 1;
      incoming.set(dependent, remaining);
      if (remaining === 0) insertSorted(queue, dependent);
    }
  }
  const cycles = new Set(
    [...workRefs].filter((ref) => !unresolved.has(ref) && !processed.has(ref)),
  );
  for (const ref of [...unresolved, ...cycles]) stages.delete(ref);
  if (cycles.size > 0) {
    diagnostics.push({
      code: "cycle",
      message: `${String(cycles.size)} work item(s) participate in or depend on a prerequisite cycle; their order stays unknown.`,
    });
  }
  const components = componentCount(workRefs, outgoing);
  if (components > 1) {
    diagnostics.push({
      code: "disconnected",
      message: `${String(components)} independent work chains are shown; same-stage branches have no order between them.`,
    });
  }
  placeStages(objects, stages, positions);
  placeBand([...unresolved, ...cycles].sort(), positions, 0.72);
  const contextGoal = snapshot.navigation?.activeOutcomeRef;
  const contextFocus = snapshot.navigation?.focusRef;
  const context = orderedContextRefs(
    objects.filter((object) => !workRefs.has(object.ref)).map((object) => object.ref),
    contextGoal,
    contextFocus,
  );
  placeContextGrid(context, positions);
  for (const object of objects) {
    const ref = object.ref;
    if (unresolved.has(ref)) roles.set(ref, "unresolved");
    else if (cycles.has(ref)) roles.set(ref, "cycle");
    else if (snapshot.navigation?.focusRef === ref) roles.set(ref, "focus");
    else if (snapshot.navigation?.currentWorkRefs.includes(ref)) roles.set(ref, "current");
    else if (stages.get(ref) === 0) roles.set(ref, "start");
    else if (stages.has(ref)) roles.set(ref, "step");
    else if (snapshot.navigation?.activeOutcomeRef === ref) roles.set(ref, "goal");
    else roles.set(ref, "context");
  }
  return { view: "work_order", positions, roles, stages, diagnostics };
}

function placeStages(
  objects: readonly SemanticObject[],
  stages: ReadonlyMap<QuicklensRef, number>,
  positions: Map<QuicklensRef, { readonly x: number; readonly y: number }>,
): void {
  const maximum = Math.max(0, ...stages.values());
  for (let stage = 0; stage <= maximum; stage += 1) {
    const refs = objects
      .filter((object) => stages.get(object.ref) === stage)
      .map((object) => object.ref);
    const x = maximum === 0 ? -0.6 : -0.82 + (stage / maximum) * 1.64;
    refs.forEach((ref, index) => {
      positions.set(ref, { x, y: spread(index, refs.length, 0.48) });
    });
  }
}

function placeBand(
  refs: readonly QuicklensRef[],
  positions: Map<QuicklensRef, { readonly x: number; readonly y: number }>,
  y: number,
): void {
  refs.forEach((ref, index) => positions.set(ref, { x: spread(index, refs.length, 0.85), y }));
}

function placeContextGrid(
  refs: readonly QuicklensRef[],
  positions: Map<QuicklensRef, { readonly x: number; readonly y: number }>,
): void {
  refs.forEach((ref, index) => {
    positions.set(ref, {
      x: index % 2 === 0 ? -0.84 : 0.12,
      y: 1.42 - Math.floor(index / 2) * 0.3,
    });
  });
}

function placeGoalContext(
  refs: readonly QuicklensRef[],
  positions: Map<QuicklensRef, { readonly x: number; readonly y: number }>,
): void {
  refs.forEach((ref, index) => {
    positions.set(ref, {
      x: index % 2 === 0 ? -0.84 : 0.12,
      y: 1.7 - Math.floor(index / 2) * 0.3,
    });
  });
}

function orderedContextRefs(
  refs: readonly QuicklensRef[],
  goal: QuicklensRef | null | undefined,
  focus: QuicklensRef | null | undefined,
): QuicklensRef[] {
  const prioritized = [goal, focus].filter(
    (ref): ref is QuicklensRef => ref !== null && ref !== undefined && refs.includes(ref),
  );
  return [...prioritized, ...refs.filter((ref) => !prioritized.includes(ref))];
}

function spread(index: number, count: number, extent: number): number {
  return count <= 1 ? 0 : -extent + (index / (count - 1)) * extent * 2;
}

function adjacency(refs: ReadonlySet<QuicklensRef>): Map<QuicklensRef, Set<QuicklensRef>> {
  return new Map([...refs].map((ref) => [ref, new Set<QuicklensRef>()]));
}

function propagateUnresolved(
  unresolved: Set<QuicklensRef>,
  outgoing: ReadonlyMap<QuicklensRef, ReadonlySet<QuicklensRef>>,
): void {
  const queue = [...unresolved].sort();
  while (queue.length > 0) {
    const ref = queue.shift();
    if (ref === undefined) return;
    for (const dependent of outgoing.get(ref) ?? []) {
      if (!unresolved.has(dependent)) {
        unresolved.add(dependent);
        insertSorted(queue, dependent);
      }
    }
  }
}

function componentCount(
  refs: ReadonlySet<QuicklensRef>,
  outgoing: ReadonlyMap<QuicklensRef, ReadonlySet<QuicklensRef>>,
): number {
  const neighbors = adjacency(refs);
  for (const [source, dependents] of outgoing) {
    for (const target of dependents) {
      neighbors.get(source)?.add(target);
      neighbors.get(target)?.add(source);
    }
  }
  const seen = new Set<QuicklensRef>();
  let count = 0;
  for (const start of [...refs].sort()) {
    if (seen.has(start)) continue;
    count += 1;
    const queue = [start];
    while (queue.length > 0) {
      const ref = queue.shift();
      if (ref === undefined || seen.has(ref)) continue;
      seen.add(ref);
      queue.push(...[...(neighbors.get(ref) ?? [])].filter((next) => !seen.has(next)).sort());
    }
  }
  return count;
}

function insertSorted(values: QuicklensRef[], value: QuicklensRef): void {
  values.push(value);
  values.sort();
}

function partialDiagnostics(snapshot: QuicklensSnapshot): GraphLayoutDiagnostic[] {
  const partial =
    snapshot.navigation === undefined
      ? snapshot.phase === "partial"
      : snapshot.navigation.completeness === "partial";
  return partial
    ? [
        {
          code: "partial",
          message:
            snapshot.navigation?.reassessmentReason ??
            "The loaded structure is partial; absent links are not inferred.",
        },
      ]
    : [];
}

function sortedObjects(snapshot: QuicklensSnapshot): readonly SemanticObject[] {
  return [...snapshot.objects].sort((left, right) => left.ref.localeCompare(right.ref));
}

function normalizeKind(value: string): string {
  return value.trim().toLocaleLowerCase().replaceAll("-", "_");
}
