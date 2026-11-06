# podatui

AirPods companion (battery, noise control, ear detection, heart rate, hi-res mic) for Windows and
Linux, continued from arctumn/ntpods (itself a LibrePods port). Owner: LargeModGames. GPL-3.0; the
ACX mic driver in `windows/drivers/mic` stays MS-PL.

## Layout

- `core/`: `podatui-core`, OS-free, zero dependencies (enforced by `core/tests/no_os_deps.rs`).
  AAP protocol (`aap.rs`), heart rate (`hr.rs`), proximity adverts (`proximity.rs`), ear-pause
  decisions (`ear.rs`), replay fixtures (`fixture.rs`).
- `windows/`: daemon `podatuid` (Rust), `ipc` (NDJSON over named pipes), WinUI app (C#, to be
  replaced by a Raven GUI), KMDF drivers, MSI installer.
- `linux/daemon`: BlueZ backend (userspace L2CAP PSM 0x1001, no driver needed).
- `tui/`: ratatui TUI, binary `podatui`.
- `docs/dev/`: status, plans and research summaries. Start with `docs/dev/status.md`.

## Gates (run before every commit)

- Windows: `.\gate.ps1` (fmt, clippy -D warnings, tests for core, windows/ipc, windows/daemon, then
  the static-CRT release build). CI uses the latest stable Rust, so also run clippy with it if the
  local default is older.
- Linux: `./gate.sh` (fmt, clippy -D warnings, tests for core, linux/daemon, tui).

## Decisions (do not re-litigate)

- Name is always lowercase `podatui`; daemon `podatuid`.
- Windows end users keep Secure Boot ON and Test Mode OFF. Until the drivers are Microsoft-signed,
  Windows ships driverless: BLE proximity adverts, auto-connect prompts, default audio endpoint,
  sleep recovery. Tier 2 (anything over AAP) on Windows waits for attestation signing or a Pi
  bridge; there is no user-mode L2CAP on Windows (see `docs/dev/research-summary.md`).
- Linux gets everything (AAP works from userspace). Work is Linux-first after the Windows
  driverless steps 1-7.
- GUI: Raven (oscar-soirey/Raven), chosen for its looks. Raven bugs are fixed in a Raven fork and
  sent upstream, never worked around in podatui.
- Never use signature bypasses (leaked certs, vulnerable drivers) or exploit Windows.

## Working rules

- Small commits, one per green logical step; commit and push immediately. No Claude attribution or
  trailers. No em-dashes in prose, comments or commit messages.
- No Python, for tooling or throwaway scripts: pwsh on Windows, bash on Linux.
- Keep CRLF in existing Windows files.
- Never push to the `upstream` (arctumn) or `ripthulhu` remotes; credit upstream code you port.
