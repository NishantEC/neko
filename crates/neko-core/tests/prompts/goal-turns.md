Keep working toward the goal. Turn 4 of at most 12.

Goal: Make the invoice export handle empty rows
Constraints: Do not change the CSV header

Neko ran the check `cargo test -p acme-export`: exit 101.
test header_is_stable ... FAILED

Judge: The header test still fails.

Fix what is failing and finish the goal. Neko will run the check again after you stop; do not claim it passes without running it. If you are done, say where the result is (a file path or a link).
---

This ticket works a goal that keeps going until its evidence says done.
Goal (the person's words): Make the invoice export handle empty rows
Constraints: Do not change the CSV header
Check (Neko runs it in this folder after each turn; passing is what done means): cargo test -p acme-export
Turn 3 of at most 12.
Journal so far (older first):
- turn 2: done: Handled the empty row in the writer | next: Fix the header test (CONTINUE)
Evidence ledger (newest last):
- `cargo test -p acme-export` exit 101

---
Evidence gate: this change touches code, and Neko has no passing check for it. Run the repository's checks (tests, build, linters) as their own commands, fix what fails, and run them again until they pass. Do not claim a check you did not run. What Neko found:
No check was run for the code you changed.