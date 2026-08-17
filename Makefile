# The public Makefile, injected over the copied one by `make public`.
#
# It is a SUBSET of the gate this project runs internally, and the subset is
# chosen by what a stranger can actually run. The targets left out call linting
# tools that are not published anywhere — not as a repository, not on crates.io,
# not as a release binary. A `check` target invoking one of them would fail for
# every reader of this repository, on a tool they cannot obtain.
#
# The alternative, a target that skips when the tool is missing, was rejected:
# a check that cannot fail is not a check, and this project refuses that shape
# everywhere else.

.PHONY: check fmt clippy test

check: fmt clippy test

fmt:
	cargo fmt --check

clippy:
	cargo clippy --all-targets -- -D warnings

# Two guards, and the second was missing for a day.
#
# A suite of zero tests reports `test result: ok. 0 passed` and exits 0. That is
# a green gate over an empty set, which is the failure this whole tool exists to
# detect one level up, so the gate refuses it. Delete this guard and `make
# check` passes on a repository with no tests at all.
#
# Counting passes says nothing about failures, and this recipe used to end on an
# `echo`, so the recipe's exit status was the echo's. `cargo test` could exit
# 101 with three failed targets and `make test` still reported `ok: N test(s)
# ran` and exited 0. Watched: with one deliberately failing test the target
# exited 0 before this line existed and 101 after. A second linter re-ran the
# suite and caught it internally; this Makefile has no such second opinion,
# which is where the defect actually bit.
test:
	@out=$$(cargo test --no-fail-fast 2>&1); status=$$?; echo "$$out"; \
	 n=$$(printf '%s\n' "$$out" | sed -n 's/^test result: .* \([0-9][0-9]*\) passed.*/\1/p' \
	      | awk '{s+=$$1} END {print s+0}'); \
	 test "$$status" -eq 0 || { echo; echo "FAIL: cargo test exited $$status. Counting passes says nothing about the ones that failed."; exit "$$status"; }; \
	 test "$$n" -gt 0 || { echo; echo "FAIL: $$n tests ran. A suite that examines nothing is not a pass."; exit 1; }; \
	 echo "ok: $$n test(s) ran"
