# Security policy

`pubgrd` copies a public repository tree out of a private one and checks that
nothing else came along. You write an allow-set of paths that may be published.
Anything not on that list fails the check. A denied path stays denied: deny
always wins over allow, and nothing re-admits it. `.envrc` and the `.env` family
are denied on every run, and no configuration can turn that off.

Because people run this tool to decide what the public sees, a defect in it can
publish a secret. This document explains which defects we treat as security
issues and how you report one.

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x | Yes |
| Anything older | Does not exist |

0.1.0 is the first release. There is exactly one supported line, and fixes land
on it. When a later line ships, this table will say how long 0.1.x keeps
receiving fixes.

## Reporting a vulnerability

Report privately. Do not open a public issue, a pull request, or a discussion
thread that describes the problem.

1. Go to https://github.com/aktagon/pubgrd, open the Security tab, and choose
   "Report a vulnerability". This opens a private advisory that only the
   maintainer sees.
2. If that form does not work for you, email christian@aktagon.com instead.

Please include the `pubgrd` version, your configuration file with secrets
removed, the tree layout that triggers the problem, and the exit code you saw.
A minimal reproduction is worth more than a long description.

### What happens next

One person maintains this project. The maintainer acknowledges your report
within one week. After that, you get a plain assessment: whether the report
reproduces, whether it counts as a vulnerability under the next section, and
roughly when a fix lands. If a report goes quiet for two weeks, email
christian@aktagon.com and ask again.

There is no bug bounty and no security team. Reporters are credited in the
release notes unless they ask not to be.

## What counts as a vulnerability here

The severe class is narrow and specific. A report is a vulnerability when
`pubgrd` reports success over a tree it did not fully examine, or when a path
reaches the public tree without matching an allow entry.

Concrete shapes of that bug:

- **A file skipped before the rules run.** Traversal misses a path, so no rule
  ever sees it. Examples: a symlink that escapes the tree, a directory the
  walker silently gives up on, a filename encoding that breaks matching. The run
  exits 0 and the file ships unreviewed.
- **A check that examines zero files and reports success.** An empty allow-set,
  an empty tree, or a rule whose input never arrives compares nothing against
  nothing and calls it a pass. Any path from a real tree to a green,
  zero-count run is a vulnerability.
- **A guard that a caller can bypass.** Anything that copies uncommitted,
  ignored, or unapproved bytes into the public tree despite the configuration
  saying otherwise. The same applies to a way to publish `.envrc` or a `.env`
  file, which no configuration may permit.

Also report these: a crafted path that escapes the destination directory during
a copy, and a way to make `pubgrd` exit 0 while a violation exists.

## What is not a vulnerability

- **Over-denial.** `pubgrd` withholds a file you wanted published. You notice
  it, you fix the configuration, you publish. Nothing leaked. Report it as an
  ordinary bug.
- **A denied path staying denied.** The tool refuses to re-admit anything the
  deny rules matched, at any precedence or specificity. That is deliberate.
- **A configuration that fails loudly.** Exit code 2 means your configuration or
  command line is wrong. Refusing to run is the safe outcome.
- **Vulnerabilities in your own publish script.** `pubgrd` does not build the
  tree, does not run git on the destination, and does not lint documents.

## Licence

`pubgrd` ships under Elastic-2.0. Reporting a vulnerability grants no additional
rights, and the licence does not restrict your right to report one.
