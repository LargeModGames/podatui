# podatui TUI design

`podatui` is a full-screen terminal app modelled on the macOS System Settings AirPods page: a sidebar of
sections and grouped settings on the right. Terminals smaller than 100×30 get a compact card modelled on the
menu-bar dropdown instead. It uses macOS dark-mode colours and Nerd Font icons.

## Full screen (120×34 shown)

```
 󰋋  podatui                                                          󰧞 AirPods Pro van Jay  connected
╭────────────────────────╮ ╭──────────────────────────────────────────────────────────────────────────╮
│ 󰋋  AirPods Pro van Jay │ │  General                                                                 │
│ L 99%  R 99%  C 100%   │ │                                                                          │
│────────────────────────│ │  󱡏  Left                  󱡏  Right                  󰋌  Case              │
│ 󰒓  General             │ │  ██████████████████████   ██████████████████████    ██████████████████████│
│ 󰟅  Noise Control       │ │  ██████████████████████   ██████████████████████    ██████████████████████│
│ 󰝁  Controls            │ │  99%  in case             99%  in case              100%                 │
│ 󰗶  Health              │ │                                                                          │
│ 󰋽  About               │ │  ╭──────────────────────────────────────────────────────────────────╮   │
│                        │ │  │ 󰟅  Noise Control               Off  Transp.  Adaptive  Noise C.  │   │
│                        │ │  │ 󰗋  Conversation Awareness                                      󰔡 │   │
│                        │ │  │ 󰕾  Personalized Volume                                         󰨙 │   │
│                        │ │  ╰──────────────────────────────────────────────────────────────────╯   │
╰────────────────────────╯ ╰──────────────────────────────────────────────────────────────────────────╯
 tab sidebar/settings   ↑↓ move   ←→ change   ⏎ toggle   1-4 noise mode   q quit
```

Sections and their settings (AAP control ids from `docs/control_commands.md`):

| Section | Settings |
|---|---|
| General | battery bars for left, right, case; Noise Control (0x0D); Conversation Awareness (0x28); Personalized Volume (0x26) |
| Noise Control | the four modes (0x0D); press-and-hold cycle (0x1A bitmask: Off 0x01, NC 0x02, Transparency 0x04, Adaptive 0x08); NC with one AirPod (0x1B); Adaptive Audio strength (0x2E); Conversation Awareness; Personalized Volume |
| Controls | press speed (0x17); hold duration (0x18); swipe to adjust volume (0x25); swipe interval (0x23); automatic ear detection (0x0A); microphone (0x01); chime volume (0x1F) |
| Health | heart rate monitoring (0x30); live heart rate (not wired yet); sleep detection (0x35) |
| About | name, model, firmware, Bluetooth address |

Row kinds: switch (0x01 on, 0x02 off), choice (segmented options), radio, checkbox, slider (0 to 100, steps of 5)
and read-only info. A switch, slider or info row the AirPods have not reported shows `--`; an unreported choice,
radio or checkbox shows nothing selected.

## Compact (under 100×30)

A 40-column card centred in the terminal: device name, left/right/case battery and ear state, the four noise
modes as radio rows, Conversation Awareness and Personalized Volume.

## Colours (macOS dark mode, truecolor)

| Role | Hex |
|---|---|
| window background | `#1C1C1E` |
| sidebar and content panes | `#28282A` |
| settings group | `#2C2C2E` |
| border, separators, picked option | `#48484A` |
| sidebar selection, unfocused; empty battery bar | `#3A3A3C` |
| label text | `#EBEBF5` |
| secondary text, key hints | `#98989D` |
| empty radios and checkboxes, off switches, unknown values | `#7C7C80` |
| focused row background (white text keeps 4.5:1) | `#0A6CDB` |
| selected radio and checkbox, slider fill | `#0A84FF` |
| switch on, charging battery | `#30D158` |
| battery at or below 20 % | `#FF453A` |

## Icons (Nerd Font, Material Design set)

| Use | Glyph | Name |
|---|---|---|
| device / not connected | `U+F02CB` / `U+F07CE` | md-headphones / md-headphones_off |
| left / right bud, case | `U+F184F`, `U+F02CC` | md-earbuds, md-headphones_box |
| battery | `U+F007A`..`U+F0079` | md-battery_10 .. md-battery |
| battery charging | `U+F089C`, `U+F0086`..`U+F0085` | md-battery_charging_10 .. _100 |
| sections | `U+F0493`, `U+F07C5`, `U+F0741`, `U+F05F6`, `U+F02FD` | md-cog, md-ear_hearing, md-gesture_tap, md-heart_pulse, md-information_outline |
| noise modes | `U+F1852`, `U+F07C5`, `U+F0674`, `U+F0A45` | md-earbuds_outline, md-ear_hearing, md-creation, md-ear_hearing_off |
| radio, checkbox | `U+F043D`/`U+F043E`, `U+F0131`/`U+F0132` | md-radiobox_blank/_marked, md-checkbox_blank_outline/_marked |
| switch on / off | `U+F0521` / `U+F0A19` | md-toggle_switch / _off_outline |
| status dot | `U+F09DE` | md-circle_medium |

The TUI needs a Nerd Font (Omarchy ships JetBrainsMono Nerd Font). There is no plain-Unicode fallback.

## Keys

| Key | Action |
|---|---|
| `Tab` | switch between sidebar and settings |
| `↑` `↓`, `k` `j` | move in the focused pane |
| `⏎`, `space` | toggle the row, step a choice, or open the section from the sidebar |
| `←` `→`, `h` `l` | step a choice or slider |
| `1`..`4` | Off, Transparency, Adaptive, Noise Cancellation, from anywhere |
| `q`, `Esc` | quit |

## Behaviour

- The AirPods report every control value when the link opens but do not echo later changes (checked live on
  AirPods Pro 3), so a command is applied to the shown state as it is sent, as LibrePods does.
- Ear state per bud: the ear packet does not say which bud is which, so a charging bud counts as in the case and
  the other bud gets the remaining status; anything still ambiguous shows nothing.
- Disconnected: the device reads `Not connected`, values show `--`, the top bar says why, and the link is retried
  every 3 seconds.

## Build

`tui/` crate, binary `podatui`, ratatui (crossterm backend). Until the ipc crate leaves `windows/`, the TUI owns the
AAP link through the `linux/daemon` library on a background thread; later it becomes an ipc client of `podatuid`.
