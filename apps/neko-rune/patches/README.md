# Local patches to Rune

Empty today. Rune takes no pull requests, so a fix we cannot wait for goes here as
`NNNN-short-name.patch`: a `git diff` made in a Rune checkout at the revision in `../RUNE_REV`,
with a first comment line saying what it fixes and the upstream issue to follow. The build applies
them in order (`../scripts/patched-rune.sh`, called by `check.sh` and `run.sh`) to a clean checkout
under the Rune cache and points every Rune crate at it. A patch that stops applying after a bump
stops the build, and is re-made against the new revision or dropped when the issue is fixed.

Keep the set small: each patch is a fork to carry. Open an issue upstream for each one.
