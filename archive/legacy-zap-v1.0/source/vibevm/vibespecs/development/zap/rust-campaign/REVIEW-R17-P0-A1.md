# Root acceptance of bounded R17 P0/A1

Root reviewed the candidate report, direct StrictValue conversion, finite
number compatibility path, private validated-output constructor, unchanged
public canonical ingress, and streaming projection digest/table framing.
The differential and real payload/projection receipts support keeping A1.

Accepted boundary: byte-identical allocation reduction in wire encoding and
projection hashing, plus the already authorized real import-only initializer
correction. The measured paired payload peak reduction is39.79% debug and42.22%
release; paired projection reduction is32.62% and39.58%. Scoped wire15/store6/
tiny app import1 receipts pass. No new tests were run solely for this review.

Measurement limits remain explicit: commit_and_verify is combined, not an
isolated commit; fresh processes are not cold disk; no old/reference full-path
baseline exists. Root corrected the report's invalid inference from equal
debug/release peaks to an A1 end-to-end improvement percentage. The remaining
approximately1.49GB/1.37GB phase peaks are measured defects to investigate.

Record plus duplicated history values account for75.07% of table values,
meeting the conditional A2 trigger. A2 has not started and needs its own bounded
assignment. Raw cloned bytes cannot be equated with encoded attributable bytes;
B remains unselected and requires its independent artifact-to-projection replay
contract. Full graph/query scalability and full R17 acceptance remain open.
