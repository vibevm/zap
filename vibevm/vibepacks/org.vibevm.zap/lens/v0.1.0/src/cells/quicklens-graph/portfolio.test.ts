/** Unified canvas topology proof. @scope spec://org.vibevm.zap/lens/PROP-010#bounded-acceptance */
import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceDemoPort } from "../workspace-demo/index.ts";
import { readWorkspaceCanvas } from "../workspace-client/index.ts";
import { QuicklensRefSchema, SemanticObjectSchema } from "../quicklens-model/index.ts";
import {
  ProjectIdSchema,
  ProjectObjectReferenceSchema,
  WorkContextIdSchema,
  projectObjectReferenceKey,
} from "../workspace-model/index.ts";
import { LIGHT_GRAPH_THEME, projectPortfolioGraph } from "./index.ts";

test("portfolio scene keeps projects scoped and collapse preserves remaining coordinates", async () => {
  const read = await readWorkspaceCanvas(createWorkspaceDemoPort(), [
    {
      projectId: ProjectIdSchema.parse("project.lens"),
      contextId: WorkContextIdSchema.parse("context.lens.main"),
      fallbackLabel: "Lens",
    },
    {
      projectId: ProjectIdSchema.parse("project.zap"),
      contextId: WorkContextIdSchema.parse("context.zap.main"),
      fallbackLabel: "ZAP",
    },
  ]);
  assert.equal(read.ok, true);
  if (!read.ok) return;
  assert.equal(read.value.completeness, "partial");
  const expanded = projectPortfolioGraph(
    read.value,
    { collapsedProjects: new Set(), collapsedMilestones: new Set(), selectedKey: null },
    LIGHT_GRAPH_THEME,
  );
  const kinds = new Set(expanded.graph.mapNodes((_key, attributes) => attributes.nodeKind));
  assert.equal(kinds.has("project"), true);
  assert.equal(kinds.has("goal"), true);
  assert.equal(kinds.has("milestone"), true);
  assert.equal(kinds.has("task"), true);
  assert.equal(kinds.has("agent"), true);
  assert.ok(expanded.diagnostics.some((diagnostic) => diagnostic.includes("omits backend")));
  for (const edge of expanded.graph.edges()) {
    assert.equal(
      expanded.graph.getNodeAttribute(expanded.graph.source(edge), "projectId"),
      expanded.graph.getNodeAttribute(expanded.graph.target(edge), "projectId"),
    );
  }
  const lensProject = [...expanded.cards].find(
    ([, card]) => card.kind === "project" && card.reference.projectId === "project.lens",
  )?.[0];
  assert.notEqual(lensProject, undefined);
  if (lensProject === undefined) return;
  const before = positions(expanded);
  const collapsed = projectPortfolioGraph(
    read.value,
    {
      collapsedProjects: new Set([lensProject]),
      collapsedMilestones: new Set(),
      selectedKey: lensProject,
    },
    LIGHT_GRAPH_THEME,
  );
  assert.ok(collapsed.graph.order < expanded.graph.order);
  for (const node of collapsed.graph.nodes()) {
    assert.deepEqual(
      [collapsed.graph.getNodeAttribute(node, "x"), collapsed.graph.getNodeAttribute(node, "y")],
      before.get(node),
    );
  }
});

test("canvas bound reserves the active goal ahead of alphabetical context", async () => {
  const read = await readWorkspaceCanvas(createWorkspaceDemoPort(), [
    {
      projectId: ProjectIdSchema.parse("project.lens"),
      contextId: WorkContextIdSchema.parse("context.lens.main"),
      fallbackLabel: "Lens",
    },
  ]);
  assert.equal(read.ok, true);
  if (!read.ok) return;
  const project = read.value.projects[0];
  if (project?.state !== "ready" || project.view.snapshot.state !== "ready") return;
  const snapshot = project.view.snapshot.snapshot;
  const navigation = snapshot.navigation;
  if (navigation?.activeOutcomeRef === null || navigation?.activeOutcomeRef === undefined) return;
  const goalRef = navigation.activeOutcomeRef;
  const goal = snapshot.objects.find((object) => object.ref === goalRef);
  const template = snapshot.objects[0];
  assert.notEqual(goal, undefined);
  assert.notEqual(template, undefined);
  if (goal === undefined || template === undefined) return;
  const context = Array.from({ length: 250 }, (_, index) =>
    SemanticObjectSchema.parse({
      ...template,
      ref: QuicklensRefSchema.parse(`aaa.context.${String(index).padStart(3, "0")}`),
      category: "other",
      semanticType: "context",
      title: `Context ${String(index)}`,
    }),
  );
  const crowded = {
    ...read.value,
    projects: [
      {
        ...project,
        view: {
          ...project.view,
          snapshot: {
            state: "ready" as const,
            snapshot: { ...snapshot, objects: [...context, goal] },
          },
        },
      },
    ],
  };
  const projection = projectPortfolioGraph(
    crowded,
    { collapsedProjects: new Set(), collapsedMilestones: new Set(), selectedKey: null },
    LIGHT_GRAPH_THEME,
  );
  assert.equal(
    [...projection.cards.values()].some(
      (card) => card.kind === "semantic" && card.object.ref === goalRef,
    ),
    true,
  );
  assert.ok(
    projection.diagnostics.some((diagnostic) => diagnostic.includes("outside the canvas bound")),
  );
});

test("project-scoped reference key cannot collide on a shared local ref", () => {
  const first = ProjectObjectReferenceSchema.parse({
    projectId: "project.one",
    contextId: "context.main",
    domain: "semantic_object",
    ref: "work.shared",
  });
  const second = ProjectObjectReferenceSchema.parse({
    projectId: "project.two",
    contextId: "context.main",
    domain: "semantic_object",
    ref: "work.shared",
  });
  assert.notEqual(projectObjectReferenceKey(first), projectObjectReferenceKey(second));
  assert.equal(
    ProjectObjectReferenceSchema.safeParse({ ...first, sourceBasisRef: "basis.not-identity" })
      .success,
    false,
  );
});

function positions(projection: ReturnType<typeof projectPortfolioGraph>) {
  return new Map(
    projection.graph.mapNodes((node, attributes) => [node, [attributes.x, attributes.y] as const]),
  );
}
