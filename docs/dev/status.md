# Status (2026-11-06)

## Done

- Fork and rebrand of arctumn/ntpods (full history kept), NOTICE, GPL notice in the app, NTPods
  takeover in the installer, driver binaries no longer committed (CI builds them).
- Daemon fmt/clippy clean; CI runs `gate.ps1` on Windows, and fmt, clippy and tests for core,
  linux/daemon and tui on Linux.
- Core split steps 1-5 (`core-split-plan.md`): AAP and heart-rate code moved into `podatui-core`
  with tests; IPC wire format pinned by golden tests.
- Windows driverless steps 1-7 (`windows-driverless-plan.md`):
  1. `core::proximity` advert parser with public test vectors
  2. BLE watcher passes sightings and restarts itself
  3. `bt::device_props` (PID, VID, container, HFP battery)
  4. `podatuid --probe` logger writing replay fixtures to `%LOCALAPPDATA%\podatui\probe-*.txt`
  5. `core::ear` pause decisions
  6. Driverless daemon mode (picked at startup from the `PodatuiAAP` service key)
  7. IPC fields: `driverless`, `audio_connected`, `ble_match`, `product_id`, richer `ConnectPrompt`
- Linux (laptop session): BlueZ daemon in `linux/daemon`, TUI in `tui/` (`tui-design.md`).

## Next

- Windows driverless steps 8-19 need the owner's AirPods for experiments X1-X6 (section 6 of the
  plan): endpoint join, podmatch Tracker, WinUI driverless UI, installer without Test Mode
  (release R1), KS oneshot connect, default endpoint, BLE ear pause, power handling.
- Core split steps 6-14 (policy into core) are on hold until Windows has a driver again.
- Linux-first: BlueZ backend and TUI on the shared core.

## Hardware facts (owner's AirPods Pro 3, "AirPods Pro van Jay - Find My")

- PID 0x2027 and VID 0x004C sit on every BTHENUM service devnode (key `{2BD67D8B-...} 8` / `7`);
  on the `DEV_<MAC>` node only while connected (see below). Container ID is shared by all 9 devnodes. The AAP devnode exists.
- While connected (2026-11-08), the `DEV_<MAC>` node does carry PID 8231 (0x2027) and VID 76
  (0x004C), so X1's model gate passes on the DEV node itself.
- HFP battery (`{104EA319-...} 2` on the `{0000111E}` node) is empty both while disconnected and
  while connected, so the Weak tier cannot show an HFP battery.
- A 60 s probe with the buds disconnected saw only type 0x07, len 0x11, prefix 0x06 frames (no
  status frames); X2 still has to capture real status frames.
- The owner's Windows PC (MSI B650 TOMAHAWK WIFI, MediaTek RZ616) has had a magnetic antenna on
  the BT port since 2026-11-08. A 90 s probe while connected saw no ACL drop, status frames
  (type 0x07, len 0x19, prefix 0x01) about once a second, and RSSI -54 to -63 dBm (dips to -71).
- X2a/X2c/X2d (2026-11-08, `probe-20261108T130219Z.txt`): the closed case keeps sending status
  frames, and the link reconnected once with the lid closed. Two addresses per pair, rotated
  together; only one advertises once both buds are worn. Byte 10 is 0x04 idle and 0x05 while the
  PC plays, flipping within one frame.
- The probe's `RENDER` stayed 0 during spotatui playback, so render edges (needed for B4) are not
  detected on this PC. Investigate before step 8.
- AirPods also pair to the laptop; disconnect one host before testing on the other.

## Known issues

- Pro 3 does NOT echo control changes (opcode 0x09) after a command, only reports all values when
  the link opens (found on Linux). The Windows driver-mode keepalive sends an ANC command every
  30 s and treats the echo as proof of life, so on a Pro 3 it would declare the link dead and show
  "Restoring audio". Fix before driver mode ships: count any incoming packet as liveness.
- A listening-mode change sent while both buds are in the case seems to be ignored.
- Mic re-arm only fires after 8 s of total AAP silence (`windows/daemon/src/main.rs`, the
  `last_data` check); if only the 0x58 audio stops while other packets flow, it never re-arms.
- Proximity lid "counter" (lid byte bits 0-2) is not a counter on Pro 3 per LinuxPods captures;
  do not use it for lid-open edges until X2 confirms.
- AAP parsers are lax in a few pinned ways (see the comments in the `aap.rs` tests).
- Branding: `assets/logo.png` and the app icons are still NTPods art; the AirPods product images
  in the WinUI assets may be Apple renders.
