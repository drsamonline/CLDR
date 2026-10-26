<!--
  CLDR — Command Line Dispatch & Route · SECURITY
  © 2026 Dr. Sohil Momin (drsamonline). PolyForm Noncommercial-licensed: retain this notice.
-->

# Security Policy

## Threat model

CLDR is a **single-user desktop launcher**. It is designed for the case where
the person running it already has a shell on that machine. It is *not* a
privilege boundary and should not be run setuid or as a service that other users
can drive.

In scope:

- Other **local, unprivileged users** on the same machine must not be able to
  make CLDR execute commands as you, or deny you service.
- The background daemon's request queue must not be writable by other users.

Out of scope: a compromised account with your own privileges, malware already
running as you, and physical access.

## Design notes relevant to security

- The summon control channel listens on **127.0.0.1 only** and never binds a
  public interface.
- The daemon state directory is resolved per-user (`$XDG_RUNTIME_DIR` →
  `~/.cache` → a per-user temp sub-directory) and tightened to mode `0700` on
  Unix; the request queue is never placed in a shared, world-writable location.
- CLDR never reads or writes the system clipboard.
- `/run`, `/open` and queued daemon requests execute what you type, **by
  design** — that is the product. Treat the command window like a shell prompt.

## Reporting a vulnerability

Please open a private report via GitHub's
[Security Advisories](https://github.com/drsamonline/CLDR/security/advisories/new)
rather than a public issue. Include: your OS and `cldr --version`, steps to
reproduce, and the impact. You will get an acknowledgement as soon as possible.
