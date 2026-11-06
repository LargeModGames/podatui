# Research summary (2026-11-06)

Condensed from multi-agent research with adversarial verification. Full reports are kept outside
the repo (owner's machines).

## No user-mode L2CAP on Windows 11

- Winsock lists "MSAFD L2CAP [Bluetooth]" and `wshbth.dll` maps it to `\Device\BthTdi`, but no
  driver creates that device on Windows 11; the socket fails before touching the radio.
- `bthport.sys` accepts a fixed table of 72 user-mode IOCTLs. The only one that opens a remote
  L2CAP channel is the SDP client: PSM 0x0001, SDP-shaped traffic only. L2CAP open/register/send
  are kernel-only BRBs on IRP_MJ_INTERNAL_DEVICE_CONTROL.
- `Windows.Devices.Bluetooth.dll` carries marshalling for an unshipped
  `Windows.Devices.Bluetooth.L2cap` WinRT namespace with no implementation or registration. Watch
  new Windows builds for it, and for `\Device\BthTdi`: either would enable a driverless Tier 2.
- No inbox signed driver can be pointed at PSM 0x1001 with an INF: each validates its own profile.
- UMDF drivers cannot submit BRBs. Phone Link uses BLE GATT and public WinRT only.

## What works on Windows without a driver

Battery in 10% steps, charging, lid, in-ear (BLE proximity adverts, 2-4 s latency); exact battery
after a one-time IRK/ENC_KEY import (fetched over AAP on Linux or Android); connect/disconnect; default
audio endpoint; AVRCP media keys; HFP mic. Heart rate only via an iPhone app rebroadcasting it as a
BLE 0x180D sensor. Noise control, conversational awareness, hi-res mic and smart-routing takeover
need a driver or extra hardware.

## Tier 2 on Windows: options

- Attestation signing via Partner Center: account free, needs an EV code-signing cert. Private
  individuals cannot get EV; sole proprietors can (SSL.com about $359/yr). Certs last at most about
  15 months; signed drivers keep loading after the cert expires. Test drivers for HVCI (Memory
  Integrity) and lock device objects to admin/service. Azure Trusted Signing and SignPath cannot
  sign kernel drivers.
- Raspberry Pi Zero 2 W USB gadget: holds AAP itself, exposes a UAC2 mic and CDC-ACM to Windows
  through inbox drivers. Costs a host slot (AirPods keep about two hosts).
- The April 2026 Windows 11 policy dropped trust for legacy cross-signed drivers; that ended
  MagicPods' "community-signed" route. Do not use leaked certificates or signature bypasses.

## GUI

Judges ranked Slint first (about 20 ms start, about 20 MB tray memory); the owner chose Raven for its
looks. WinUI's slowness is mostly code (blocking daemon shutdown on the UI thread, eager window).
On Wayland no toolkit can position the connect popup or tray flyout freely; plan per-OS code there.
