# SM04 development-campaign pilot source

[pilot-source.json](pilot-source.json) captures the completed 1.0.0 development
campaign as a source fixture for the 1.1.0 strategic-map work.

Source: [campaign.json](../../../../../../v1.0.0/vibevm/vibespecs/development/zap/rust-campaign/campaign.json),
revision **211**, captured **2026-09-14T14:14:11Z**. SHA-256:
`327ad2b3e09f5e8134ad06c589e65c45adffda3daf6beb4c41900b4f391ad4d9`.
This is a qualified file snapshot, not a runtime store revision or RelevantBasis.

- **21** exact task IDs and outcome labels, including separate R01 and
  R01-FOUNDATION.
- **49** `depends_on` and **9** `acceptance_depends_on` edges, with distinct
  kinds and original source pointers. There are 58 unique typed edges and no
  overlapping endpoint pairs between the two kinds.
- **6** agreed derived labelled groups cover the 21 IDs once. Their 14 coarse
  pairs retain the contributing typed edges and create no whole-group barrier.
- Missing effort, elapsed/wait ranges, complexity/difficulty grades and
  uncertainty remain **Unassessed**, including for already accepted source
  tasks. No zero, low grade or route distance is inferred.
- Names and result text preserve source outcomes. Purpose expansions and group
  descriptions are explicitly editorial summaries, not new requirements or
  acceptance conditions. Early-start examples retain all acceptance prerequisites.

Uniqueness, endpoints, group coverage, edge counts and acyclicity were checked
mechanically; all four named input hashes were unchanged before writing.
The earlier inventory used revision 210; its membership and both edge kinds
were revalidated against this captured revision 211.

Only these two development files were created. No source-plan or canonical
graph write, runtime execution, migration, NEXT action, public API/guide,
test, Git or publication operation occurred. This fixture is not a runtime
projection proof or a planning-effectiveness experiment.

