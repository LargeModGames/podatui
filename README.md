# podatui

Use your AirPods with a Windows PC the way you would with a Mac: battery, noise
control, ear detection, conversational awareness, the hi-res microphone as a normal
input, heart rate on AirPods Pro 3, and more. Free and open source, drivers included.

> [!NOTE]
> podatui is my continuation of [NTPods](https://github.com/arctumn/ntpods) by
> [arctumn](https://github.com/arctumn), which started as the Windows port of
> [LibrePods](https://github.com/kavishdevar/librepods). The full history of both is
> kept in this repo. podatui is not affiliated with NTPods or LibrePods, so please
> open issues here.

## Where it's going

What I'm working towards, roughly in this order:

- **Connecting like a Mac does**: open the case near the PC and see a popup with
  battery, connect automatically, recover cleanly after sleep, and make the AirPods
  the default audio device when they connect.
- **Switching with your iPhone**: hand the AirPods between the PC and the phone
  the way Apple devices do. This is experimental and may not be possible.
- **A terminal UI** next to the desktop app, built on a shared core.
- **Linux**, through BlueZ, once the core is split from the Windows code.
- **A new desktop app** built with [Raven](https://github.com/oscar-soirey/Raven).

Today it does everything NTPods does, on Windows 10/11 only (everything is in
[`windows/`](windows/README.md)). On Android or Linux, use
[LibrePods](https://github.com/kavishdevar/librepods).

## Compared to LibrePods and MagicPods

Based on each project's README and store page as of September 2026. If something
here is wrong, open an issue and I'll fix it.

| | podatui | [LibrePods](https://github.com/kavishdevar/librepods) | [MagicPods](https://magicpods.app/) |
|---|---|---|---|
| Platforms | Windows | Android, Linux | Windows, Steam Deck |
| Price | Free (GPL-3.0, drivers too; the ACX mic driver is MS-PL) | Free (GPL-3.0) | Paid (Microsoft Store) |
| Battery | yes | yes | yes |
| Noise control | yes | yes | yes, with its driver |
| Ear detection | yes | yes | yes |
| Conversational awareness | yes | yes | yes, with its driver |
| Adaptive / personalized volume, allow off | yes | Android | partial |
| Hi-res mic as a system input | yes | no | no |
| Heart rate (AirPods Pro 3) | yes | Android | no |
| Hearing aid | experimental | Android, needs VendorID spoofing | no |
| Head gestures | no | Android | no |
| Other earbuds | no | Nothing (Linux) | yes |
| Low-latency mode | no | no | yes |
| Needs Test Mode on Windows | yes | n/a | yes, for its driver (ANC and most AirPods features) |

Like podatui, MagicPods needs a driver for noise control and most AirPods features,
and that driver needs Test Mode (a community-signed build only works on Windows
versions before the April 2026 update). MagicPods is the better pick if you use
non-Apple earbuds or want a low-latency mode. LibrePods is what you want on Android
and Linux.

## Installing on Windows

Download `podatui.msi` from the [nightly release](https://github.com/LargeModGames/podatui/releases/tag/nightly) and run it.
The drivers are only test-signed, so Windows has to be in Test Mode with Secure
Boot turned off first; the installer checks and tells you if it isn't. Read the
[Windows README](windows/README.md) before you do this; it explains the risks and
how to undo it.

If NTPods is installed, podatui replaces it: both ship a driver for the same
AirPods connection, and only one can own it.

## What's in the repo

- `windows/`: the two drivers (AAP channel and virtual mic), the daemon (Rust), the
  WinUI app (C#) and the installer
- `docs/` and `AAP Definitions.md`: notes on the AirPods protocol
- `.github/workflows/ci-windows.yml`: builds everything and publishes the release

## Credits

- [NTPods](https://github.com/arctumn/ntpods) by [arctumn](https://github.com/arctumn):
  the Windows drivers, daemon, app and installer podatui is built on.
- [LibrePods](https://github.com/kavishdevar/librepods) by
  [kavishdevar](https://github.com/kavishdevar) and its contributors. The protocol
  work all of this is built on comes from there.
- [@thibaup](https://github.com/thibaup) (heart rate on Android, LibrePods PR #702)
  and [@SAGIRIxr](https://github.com/SAGIRIxr) for their heart-rate findings.
- [@Gab4545](https://github.com/Gab4545) for building the drivers in CI.
- [pabloaul/apple-wireshark](https://github.com/pabloaul/apple-wireshark) for the
  Wireshark dissectors.

## License

GPL-3.0, same as NTPods and LibrePods; see [NOTICE](NOTICE) for what changed. The
ACX mic driver in `windows/drivers/mic` is based on a Microsoft sample and stays
under its MS-PL license. AirPods is a trademark of Apple Inc. This project isn't
affiliated with Apple.
