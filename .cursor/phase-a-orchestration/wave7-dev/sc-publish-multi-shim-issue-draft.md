Title: Extension point: list the binaries a Scoop package puts on PATH (mirror Homebrew `binaries`)

## Gap

A consumer can declare several `[[release_binaries]]`. The release archive and
the Homebrew formula already ship all of them: `channels.homebrew.formulas[].binaries`
is a list. The Scoop channel cannot do the same. `[channels.scoop].binary` must be
one string, and `release/scoop/manifest.json.j2` renders it as a single
`architecture.64bit.bin` value. Scoop itself accepts a list for `bin`, with one
shim per entry. So a multi-binary package installs every binary but puts only
one on PATH.

## Proposed extension point (opt-in, generic)

- `[channels.scoop]` accepts an optional `binaries` list, the same shape as
  the Homebrew field. When it is present, `manifest.json.j2` renders `bin` as a
  JSON array with one shim per listed binary.
- The existing `binary` string is unchanged. A consumer that does not set
  `binaries` renders a byte-identical manifest. The PR includes a test that
  proves this.
- When `binaries` is set, each entry is validated with the existing Homebrew
  rule ("is a declared `[[release_binaries]]` entry"). This check runs only for
  consumers that opt in. It adds no new mandatory gate.

## Why it helps every consumer

- It makes the Scoop channel as expressive as Homebrew, which simplifies the
  configuration model because one shape now serves both channels.
- It adds no required input, secret, file, tool, version floor or manual step.
  Single-binary consumers change nothing.

## Out of scope

WinGet command aliases are left to a separate issue. That issue should be
filed only if the WinGet manifest schema supports more than one alias per
package, which needs checking first.
