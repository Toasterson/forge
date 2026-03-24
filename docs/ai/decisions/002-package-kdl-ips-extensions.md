---
status: Accepted
date: 2026-03-24
---

# ADR 002: Extend package.kdl for Complete IPS Packaging

## Decision

Extend the package.kdl format with: enhanced cargo build section, user/group actions, files section for component-local file delivery, enhanced package section with dir/link/preserve/restart-fmri, group dependency kind, driver actions, and license auto-detection.

## Rationale

The current format can express sources, builds, and basic package splitting but cannot produce complete IPS packages. Missing: system account creation, config file preservation, SMF manifest delivery, cargo build options, and explicit directory/link actions. These are all required by oi-userland's Makefile system which Forge replaces.

## Key Design Choices

- `env` in cargo section uses KDL properties (VAR="value") for natural key=value syntax
- New `files {}` section separates component-local file delivery from upstream source management
- `package {}` section handles IPS attributes (preserve, restart-fmri) via regex selectors on proto area contents
- User/group are top-level Recipe children (like dependencies)
- License auto-detection in pkgdev, no format change needed
- Both libips and non-libips manifest generation paths updated
