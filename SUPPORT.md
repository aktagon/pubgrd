# Support

Here is where to go, depending on what you need.

## Learning how to use pubgrd

Start with the [README](README.md) for what the tool does and how to install it.
Then work through [docs/guides/getting-started.md](docs/guides/getting-started.md),
which walks you from an empty configuration to a verified tree.

## Looking up a flag or a subcommand

Run `pubgrd --help`. Every subcommand takes `--help` too, so `pubgrd cp --help`
prints the flags for that command. The help output is the authoritative command
surface; the guides only cover the common paths.

## Reporting a bug, or asking why it behaves that way

Open an issue at https://github.com/aktagon/pubgrd/issues.

A good report includes the version you ran, the command line, your
configuration, and what you expected instead of what happened. If the tool
copied a file you meant to deny, or refused a file you meant to allow, say which
patterns you wrote. That usually answers the question on its own.

Search the existing issues first. Someone may have hit it already.

## Reporting a security vulnerability

Read [SECURITY.md](SECURITY.md) and follow the process there. Do not open a
public issue for a vulnerability, and do not attach a reproduction that contains
private files.

## What to expect

One person maintains pubgrd. Answers are best-effort and arrive when time
allows, which sometimes means weeks. There is no commercial support, no paid
tier, and no service-level agreement.

Clear, reproducible reports get answered first. Pull requests that come with a
failing test get answered fastest.
