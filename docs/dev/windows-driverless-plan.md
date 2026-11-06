# Final plan: podatui driverless mode

## 0. Basis

**Base design.** I started from **R1, "advert-first, prompt-don't-steal"**, which had the highest total (75+72+84 = 231). The robust-matching design scored 223 and core-first 216.

**Grafts from the robust-matching design:**
- Windows' own link state is the identity anchor.
- Actions are tiered by confidence.
- Siblings are merged (one pair advertising from two addresses).
- Guard D.
- Every connect is verified.
- The pnputil fix for the mic devnodes.
- Mode detection checks whether the driver interface exists.

**Grafts from the core-first design:**
- `--probe` writes the replay-fixture format directly, and `core/tests/replay.rs` runs those fixtures, including merged two-session fixtures.
- `Snapshot.connected` keeps its AAP meaning, and a separate `audio_connected` is added.
- An explicit "Several AirPods nearby" state.
- A case-open overlay while connected.
- Prompt art picked by PID.
- The ear-pause logic is extracted into core early.
- The driverless skeleton lands before the Tracker.

**mustFix items and where each is fixed:**

| mustFix | Fixed in |
|---|---|
| Auto-connect is prompt-only until X3. After that it is automatic only when byte 10 == 0x00 and guard D passes. | §1.7, step 16 |
| No address DenyList. Suppression lasts one visit and re-arms on 10 s of absence, the next lid edge, or LinkUp. A failed connect gives exactly one "may be in use" prompt. | §1.8 |
| B2 (sole candidate, no edge) gives only the **Weak** tier: no Snapshot battery, no overlays, no ear pause. It is promoted only by B1, B3, B4 or a continuity run. | §1.5 |
| The sibling merge and the B1 window are checked explicitly in X2, with a fallback merge rule. | §1.3, X2c/X2b |
| Hardware flags are honest (§5 legend). X1 checks that DevicePID sits on `BTHENUM\DEV_<MAC>` itself. | §5, X1 |
| Uninstall-PodatuiDrivers depends on devcon. Fixed with pnputil `/remove-device`. | step 13 |
| No ACL-based anchor. The anchor is our container's render endpoint going ACTIVE (step 8 lands before the Tracker is wired). | §1.5, step 8 |
| R1's uniqueness rule would mark our own two-address pair Ambiguous. Uniqueness counts merged groups. | §1.3 |
| R1's "scan only while disconnected" contradicts rotation following and BLE ear pause. Scanning is decided by X6. | §4.8, decision D3 |

**Code facts I checked (HEAD 6d802d9):**
- Ctx.mac has 8 uses: field `main.rs:205`, uses at 717, 1118, 1126, 1151, 1164, 1252, 1568 and 2257. The robust design's list missed 1568, the `hr_db.sample` call.
- `le.rs` (96 lines) returns silently on a `setup()` failure (32-35). Its `scanning` flag at 36-46 never sees an Aborted watcher.
- The BLE closure at `main.rs:2228-2264` auto-connects on any Apple 0x07 advert. In driverless mode it must be switched off, or a stranger's AirPods trigger `audio_request`.
- The ear-pause logic is at `main.rs:1835-1865`.
- `open_driver` (SetupDi on `GUID_DEVINTERFACE_PODATUIAAP`) is at `driver.rs:158-185`.
- `Uninstall-PodatuiDrivers` is at `setup-common.ps1:344-357` and skips the ROOT devnodes when `tools\devcon.exe` is missing. Line 341 of the same file already uses `Get-PnpDevice | ? HardwareID -contains ...`.
- The wxs LaunchCondition is at line 38. The upgrade skips are at lines 148 and 150.
- In `windows` 0.58, the `implement` feature is a no-op (`windows-0.58.0/Cargo.toml:741 implement = []`). windows-core 0.58 always re-exports `implement` and `interface` (`lib.rs:58-59`), so no new crate is needed.
- The CM_* functions come with windows-sys `Win32_Devices_DeviceAndDriverInstallation`, which is already enabled.
- Correction to the research: `devnode.rs` does **not** use CM. It only runs `schtasks`.
- `core` has zero dependencies. `gate.ps1` does not build WinUI.

## 1. Own-AirPods matching algorithm

`core/src/podmatch.rs` holds `Tracker`. It is a pure state machine: time is passed in as `now_ms`, it does no IO, and all constants are named consts in one block.

### 1.1 Inputs
- `on_advert(now, addr: u64, public: bool, rssi: i16, &proximity::Advert)`. Only status frames reach it (type 0x07, len 0x19, prefix 0x01). Prefix-0x07 identity frames go to `on_identity(now, addr, public)` and are used only by B3.
- `on_link(now, up: bool)`. Up means the render endpoint of **our container** went ACTIVE (endpoint.rs). Down means it went UNPLUGGED or NOTPRESENT.
- `on_render_edge(now, playing)`, from the volume poller's `any_render_active` edge. Used only by B4.
- `set_owner(mac, pid: Option<u16>)`, with the PID from bt::device_props.
- `reset()`, on Resume or Radio.
- `tick(now)`, every 1 s.

Outputs are a `Vec<Out>` of:
- `Prompt { battery, product_id, busy }`
- `AutoConnect`
- `Bound`
- `Unbound`
- `Battery(Batt)`
- `Ear([bool; 2])`
- `CaseLid { open }`
- `Match(BleMatch)`

`Batt` is a core struct. The daemon maps it to `ipc::Battery`, so core does not depend on ipc.

### 1.2 Disabled states
- `mac == 0` (before the first pairing): the Tracker is inert. There is no popup, no battery and no auto-connect. Only the probe logs.
- `pid == None` and no learned model: same as above. The model is learned at the first **Bound** anchor and kept in memory (decision D5).

### 1.3 Tracks and groups
- One track per advert address holds an RSSI EWMA (alpha 0.3), the last frame, first and last seen, and the time of the last lid edge. A track expires after 10 s of silence. At most 8 tracks are kept.
- **G0 eligibility.** A track is eligible when all of these hold:
  - the model equals the owner model;
  - the colour equals the learned colour, when that colour is known and non-zero;
  - the RSSI EWMA is at least -80.
  
  Anything else is never shown or acted on.
- **Sibling merge into a Group.** Two G0 tracks seen within 2 s of each other form one group when either rule holds:
  - **Rule A (expected):** A.this == B.other and A.other == B.this for the battery nibbles (0xF matches anything), and their bit-5 values are opposite.
  - **Rule B (fallback, enabled if X2c shows rule A does not hold):** the case nibble and the lid counter are equal and the first frames arrived within 2 s.
- All uniqueness checks below count **groups**, not addresses. This fixes the self-ambiguity in R1.

### 1.4 Lid-open edge
- Only trustworthy frames count: status bit 6 or bit 2 set.
- An edge is one of these:
  - lid bit 3 goes from 1 to 0;
  - the lid counter changes;
  - a new group's first trustworthy frame shows Open after at least 10 s of silence.
- The edge must be confirmed by 2 consecutive frames.
- Bit-4-only frames give `Lid::Unknown` and never make an edge (CAPod #598).

### 1.5 Tiers

| Tier | How it is reached | What it allows |
|---|---|---|
| **Bound** | **B1:** on LinkUp, exactly one G0 group had an accepted lid edge or first appeared in [LinkUp-20 s, LinkUp+3 s]. The window is resized from the X2b latency data. **Promotion from Weak** happens through B3 (public identity-frame address == paired MAC, if X2h confirms it), B4 (byte 10 follows our render edges 0x04<->0x05 within 3 s, if X2d confirms it), or two consecutive rotation-continuity hand-offs (§1.6) while the group stays the only G0 group at >= -70. | Snapshot battery, `ble_match=bound`, case-open/close and low-battery overlays, ear pause |
| **Weak** | **B2:** linked with no edge (daemon start, resume, buds already worn), and exactly one G0 group at >= -70 for >= 5 s, and not "both in case, lid closed". | `ble_match=weak` only. The battery shows HFP battery if X1 proves it exists, otherwise the missing-value dash. No ear pause. |
| **Likely** | Not linked, and all of: an edge within the last 3 s; RSSI EWMA >= -60 over >= 2 frames; no other G0 group within 8 dB in the last 5 s. Optional bonus **L4:** dormant continuity, where the lid counter = dormant+1 mod 8 and the batteries are within 1 step. | `Prompt` with that group's battery. `AutoConnect` only under §1.7. |
| **Ambiguous** | Two or more groups fail the uniqueness rule, or rotation finds two or more qualifiers. | `ble_match=ambiguous`: "Several AirPods nearby". Only a prompt with no battery. |
| **None** | Anything else. | Nothing. |

### 1.6 Rotation continuity
This is AirPodsDesktop's rule, tightened. When the bound track has been silent for more than 3 s, a new G0 track that first appeared within 5 s inherits the binding only if all of these hold:
- each of L, R and case moved by at most 1 nibble step (0xF matches anything);
- the charging flags are identical;
- the in-case and in-ear flags are identical or show one plausible change;
- the lid counter is unchanged;
- |ΔRSSI| <= 20 dB;
- it is the **only** qualifier.

With two or more qualifiers the result is Ambiguous and the Snapshot battery goes to None. The Tracker never guesses.

### 1.7 Connect policy (fixes the steal risk)
- **Until X3 shows that ONESHOT_RECONNECT does not take the buds from an idle or playing iPhone, every Likely edge only Prompts.** KsProperty returns S_OK immediately, so a steal cannot be undone afterwards.
- After X3 and X2f, `AutoConnect` is emitted only when all of these hold:
  - the tier is Likely;
  - the group's byte 10 == 0x00 (attached to no host), which X2f must confirm on the Pro 3;
  - (L4 or RSSI >= -55);
  - !user_disconnected;
  - **guard D:** no other G0 group at >= -75 has an in-ear bit set with byte 10 != 0.
- In every other case the Tracker sends a Prompt instead.
- Every action targets the paired MAC or container, never an advert address, so podatui can never connect a stranger's buds.

### 1.8 Verification and suppression
- After any Connect (auto or button), wait up to 10 s for LinkUp.
  - **Success:** the group becomes Bound through B1.
  - **Failure:** exactly one `Prompt { busy: true }` per visit. The prompt adds the line "The AirPods may be in use on another device".
- Suppression is **per visit only**. It re-arms on any of:
  - 10 s with no frames from the group;
  - the next lid edge;
  - a LinkUp to our container.
- There is no address TTL or DenyList, so a failure while the buds were on the iPhone cannot silence our own next case-open.

### 1.9 Lost
- After 10 s of silence while linked, the battery keeps its last values, which are simply no longer updated.
- While unlinked it clears.
- The case nibble keeps its last non-0xF value for the life of the track (CAPod behaviour).
- On unbind, a Dormant record {lid counter, batteries, t} is kept for L4.

### 1.10 Residual risk
A stranger with the same model who opens a case within about 1 m, while ours are closed, gives one false prompt with their battery. That prompt's Connect only pages our MAC. Only the later IRK import removes this risk.

### 1.11 Initial constants
These are tuned from the X2 fixtures:

| Constant | Initial value |
|---|---|
| RSSI floor | -80 |
| Popup threshold | -60 |
| Auto-connect without L4 | -55 |
| Weak (B2) threshold | -70 |
| Guard D | -75 |
| Uniqueness margin | 8 dB |
| Continuity RSSI | 20 dB |
| Edge window | 3 s |
| B1 window | -20 s / +3 s |
| Lost timer | 10 s |
| Verify timeout | 10 s |

## 2. Architecture

**One binary, mode chosen at runtime.** There is no cargo feature.

- `driver::interface_present()` is split out of `open_driver` (SetupDi on `GUID_DEVINTERFACE_PODATUIAAP`, `DIGCF_PRESENT`). It runs once at startup and logs `mode: driver` or `mode: driverless`. A driver change needs a reboot anyway.

**Driver mode (dev box with Test Mode).** Behaviour is exactly today's. The old BLE closure is adapted to the new le.rs signature and gated on `proximity::parse` returning a status frame. Nothing else changes, so there are no interleaved connect paths.

**Driverless mode:**
- No run_receiver, poll_mic, rename::apply or devnode recovery, and no old BLE closure.
- AAP-only commands are inert: SetAnc, SetFeature, SetControl, SetHeartRate, SetHearingAid and SetName log and return without changing state. This fixes the optimistic writes at `main.rs:1013` and `1039-1047`.
- RepairConnection is treated as Connect.

### core (OS-free, std only)
- `proximity.rs`: `parse(&[u8]) -> Option<Advert>`, `parse_identity`, `model_name(pid)`.
  - It walks [type][len] records.
  - Batteries are nibble x10: 0xF gives None, 11-14 are clamped to 100. The side comes from bit 5.
  - The primary bud follows CAPod (bit5 XOR bit6), and in-ear is suppressed for a charging bud.
  - Lid is `Open(n)`, `Closed(n)`, `Unknown` or `NotInCase`. Also colour, audio byte 10, and the 16-byte `tail` (for later IRK work).
- `podmatch.rs`: `Tracker` (§1).
- `ear.rs`: `EarPause::update([Option<bool>; 2], playing: impl FnOnce()->bool) -> EarAction`. This is `main.rs:1835-1865` moved without behaviour change, with transitional values passed as None.
- `core/tests/replay.rs` plus `core/tests/fixtures/*.txt`. The fixture format, one event per line:
  ```
  <ms> ADV <addr12> <P|R> <rssi> <hex>
  <ms> EP <0|1>
  <ms> RENDER <0|1>
  <ms> RESUME
  <ms> NOTE <label>
  ```
  A `merge(a, b, addr_remap, rssi_offset)` helper simulates a stranger's identical pair.

### windows/daemon
- **`le.rs`:**
  - Callback is `Fn(Sighting{addr, public, rssi, data})` for every Apple record.
  - `AdvertisementFilter` on CompanyId 0x004C.
  - The loop is driven by `Status()`. It recreates the watcher on Aborted, or on a Stopped we did not request, retrying every 3 s.
  - A Stopped handler logs `args.Error()`. A setup() failure is logged.
  - An external `restart` flag is consumed by the loop.
- **`bt.rs`:** `device_props(mac) -> {pid, container, hfp_battery}`.
  - Uses `CM_Get_Device_ID_ListW("BTHENUM")` plus `CM_Get_DevNode_PropertyW`.
  - PID: `{2BD67D8B-...} 8` on `BTHENUM\DEV_<MAC>`.
  - Container: `DEVPKEY_Device_ContainerId`.
  - HFP battery: `{104EA319-...} 2` on the `{0000111E` node.
- **`endpoint.rs`** (new, MTA thread):
  - **Container join.** `EnumAudioEndpoints(eRender|eCapture, DEVICE_STATEMASK_ALL)` filtered on `PKEY_Device_ContainerId`.
  - **State notifications.** An `IMMNotificationClient` that only posts `In::Endpoint{active}` to the channel and never calls MMDevice inside the callback.
  - **`oneshot(connect)`.** Path: IDeviceTopology, then IConnector::GetConnectedTo, then IPart::GetTopologyObject, then GetDeviceId. It keeps IDs starting with `{2}.\\?\bth`, dedupes them, and calls `Activate(IKsControl)` then `KsProperty(KSPROPSETID_BtAudio, ONESHOT_RECONNECT/DISCONNECT, GET)`. Filters are resolved fresh on every call.
  - **`set_default(render_id)`.** IPolicyConfig: IID F8679F50, CLSID 870AF99C, 10 placeholder slots, then SetDefaultEndpoint. It covers eConsole, eMultimedia and eCommunications, skipping any role that is already ours. Render only.
- **`ble_link.rs`** (new): `run_driverless(ctx)` owns the Tracker and reads `mpsc::Receiver<In>` with a 1 s `recv_timeout` that drives tick. `In` is one of Adv, Identity, Endpoint, Render, Resume, Radio, Tick.
  - It re-polls `find_airpods` and device_props every 30 s while mac == 0, and on Radio.
  - It executes the Tracker outputs:
    - Prompt: `Event::ConnectPrompt`.
    - AutoConnect: `endpoint::oneshot(true)`.
    - Battery: written to the Snapshot (Bound only).
    - CaseLid: overlay using the text format of `main.rs:1701-1708`.
    - Ear: `EarPause`, then media.
    - On a LinkUp edge: `set_default`.
- **`power.rs`** (conditional on X4): `PowerRegisterSuspendResumeNotification(DEVICE_NOTIFY_CALLBACK)` and `CM_Register_Notification(GUID_BTHPORT_DEVICE_INTERFACE)`. The callbacks only post Resume or Radio to the channel.
- **`main.rs`:**
  - mode switch;
  - `Ctx.mac` becomes `Arc<AtomicU64>` with `fn mac()` (8 sites);
  - apply_command routes Connect, Disconnect and RebuildAudio to `endpoint::oneshot` in driverless mode (DISCONNECT then RECONNECT replaces `force_audio_rebuild` at 672-684);
  - `--probe` is handled before the mutex at `main.rs:2141`.
- **Cargo:**
  - `windows`: add `Win32_Media_KernelStreaming`, `Win32_Devices_FunctionDiscovery`, `Win32_UI_Shell_PropertiesSystem`, `Win32_System_Com_StructuredStorage` and `Win32_System_Variant`.
  - `windows-sys`: add `Win32_System_Power` only if power.rs lands.
  - No version bumps.

### IPC (additive, all `#[serde(default)]`)
- **Snapshot** gains:
  - `driverless: bool`
  - `audio_connected: bool`, meaning our render endpoint is ACTIVE
  - `ble_match: BleMatch {None, Ambiguous, Likely, Weak, Bound}` (snake_case)
  - `product_id: u16`
- `connected` keeps its AAP meaning.
- **`Event::ConnectPrompt`** gains `battery: Option<Battery>`, `product_id: u16` and `busy: bool`. The old app reads only `name` (Messages.cs:276).

### WinUI
- Messages.cs parses the new fields.
- **DeviceHeader:** "connected" is `Connected || AudioConnected`. When Driverless, Repair and Rename are hidden, and Disconnect and Restore audio are shown.
- **DevicePage.Update (160-167):**
  - VolumeCard is enabled on `AudioConnected`.
  - When Driverless, the NoiseControl, AdaptiveNoise, Features, Mic, HeartRate, HearingAid and Multipoint cards are hidden, so `Mic_NotLoaded` never shows.
- **BatteryCard and tray:** an Ambiguous state shows "Several AirPods nearby". The NC, Transparency and Adaptive rows are hidden when Driverless.
- **App.xaml.cs:77-79:** the island fires on the rising edge of `Connected || AudioConnected`.
- **MainWindow.OnConnectPrompt (198-217):** suppresses the prompt while `_connected || AudioConnected`.
- **ConnectPromptWindow:** a battery row (hidden parts when null), the busy line, and art from a new `DeviceArt.FamilyForPid`:
  - 0x2027 to Pro 3
  - 0x2014 and 0x2024 to Pro 2
  - 0x200E to Pro
  - 0x2019 and 0x201B to AirPods 4

  The name fallback at 161-173 stays.

## 3. Installer changes

The end-user MSI and zip ship **no drivers**. The drivers become dev-only.

**`setup-common.ps1`** (its own commit, before the installer commit). In `Uninstall-PodatuiDrivers`, replace the `devcon remove` calls with:
```
Get-PnpDevice | ? { $_.HardwareID -contains 'ROOT\PodatuiMicPC' -or $_.HardwareID -contains 'ROOT\AudioCodec' } | % { pnputil /remove-device $_.InstanceId }
```
Wrap each call in Invoke-Tool. The rest of the function is unchanged.

**`podatui.wxs`:**
- Delete the BOOTOPTIONS search and the LaunchCondition (33-39).
- Delete the Drivers and MicAcx features (100-108) and the markers (81-89).
- Delete the OSBUILD/MICDRIVER/SetPodatuiInstallDrv actions (122-129), PodatuiRemoveDrivers (131, 147-148) and ScheduleReboot (152).
- Stop shipping devcon, fix-driver.ps1, rename-mic.ps1 and the driver folders.
- Keep the FFmpeg DLLs (build.rs:12-24 links them unconditionally), the Run values (74-79), PodatuiStop, PodatuiInstall and PodatuiUninstall.

**`msi-setup.ps1 -Action install`:**
- Drop `-Drivers`/`-MicDriver`, the Test Mode throw (41) and `Register-PodatuiTasks` (46).
- Add a **migration block, run only when `-not (Test-TestMode)`**:
  1. Uninstall-PodatuiDrivers
  2. Remove-MicDriverPackages
  3. Unregister-PodatuiTasks
  4. Remove-TestCert `CN=podatui Test Cert`
  5. Remove-SetupLeftovers
  6. Delete `%LOCALAPPDATA%\podatui\fix-driver.ps1` and `rename-mic.ps1`

  Each step sits in try/catch and logs. This closes the major-upgrade gap (wxs 148 and 150) and the user-writable elevated-task privilege escalation. With Test Mode on, nothing is touched, so a dev box survives upgrades.
- Keep the NTPods/LibrePods cleanup.

**`install.ps1` (zip):**
- Remove the Secure Boot/testsigning header (14-17), the hard-fail (38-53), the BitLocker prompt (56-62), the driver and devcon entries in the required list (64-74), Install-PodatuiDrivers (90-93), the script copies (99-101), the task registration (109) and "reboot needed" (124).
- Add the same migration block.
- Admin stays required (decision D9).

**New `windows/installer/dev-drivers.ps1`** (not packaged):
- Requires admin and Test Mode.
- Calls Install-PodatuiDrivers and Register-PodatuiTasks.
- `-Remove` calls the uninstall half.

**`make-dist.ps1`:** drop the driver, driver-mic, driver-mic-acx and tools folders (60-64), the script copies (75-76), the mic throw (83-86), the driver and devcon entries in the required list (107-116) and the catalog check (118-129).

**`ci-windows.yml`:**
- The WDK and driver build (57-146) moves to a separate job that does not feed packaging (decision D6).
- The release notes (280-282) say "No Test Mode needed".

**Docs:** README.md:51-62, windows/README.md:43-57, 83, 123 and 204, and windows/drivers/aap/README.md:54-85, which becomes the dev path. Add the note: "you may now `bcdedit /set testsigning off` and re-enable Secure Boot".

**Migration matrix:**

| Starting point | Result |
|---|---|
| Fresh install, Secure Boot on (the owner) | Installs with no reboot; the daemon logs `mode: driverless`. |
| Old install, Test Mode off | Drivers, ROOT devnodes, cert, tasks and scripts are removed. |
| Old install, Test Mode on | Everything is kept; `mode: driver`. |

## 4. Per-feature mechanism

1. **Installer:** see §3.
2. **Daemon without the driver:**
   - The mode switch skips the driver threads, which removes about 2,400 log lines an hour and the "Recovering the AirPods driver" island every 2 minutes (`main.rs:1331-1358`).
   - The old any-advert auto-connect closure is off.
   - mac is atomic and re-polled every 30 s, so a pairing is picked up without a restart.
   - `audio_connected` comes from endpoint notifications, with no ACL poll.
3. **Advert data:** core `proximity::parse` on the WinRT `ManufacturerData.Data` bytes. le.rs sends address, type, RSSI and bytes.
4. **Case-open popup with battery:**
   - A Likely edge while `!audio_connected` produces `ConnectPrompt{battery, product_id}`.
   - A Bound edge while connected produces a case overlay.
   - On LinkUp the island shows the Bound battery (B1 binds at that moment).
5. **Connect:**
   - `endpoint::oneshot(true)` with no admin, verified by ACTIVE within 10 s, with a busy prompt on failure.
   - Prompt-only until X3, then gated per §1.7.
   - Disconnect uses ONESHOT_DISCONNECT, which avoids BluetoothSetServiceState churn and Code 38.
   - The `bt::set_audio_connected` path stays for driver mode.
6. **Default endpoint follows:**
   - Once per LinkUp edge, processed on the ble_link thread: if `GetDefaultAudioEndpoint(eRender, eMultimedia)` is not ours, `set_default` for the 3 roles. Render only, because opening capture forces HFP.
   - No retry, and it never re-asserts after a manual change.
7. **Sleep/resume and radio:**
   - le.rs self-heals on Aborted or an unrequested Stopped.
   - ble_link re-resolves endpoints on every call and re-polls device_props.
   - power.rs (Resume and BTHPORT arrival lead to watcher restart, `tracker.reset()`, endpoint re-registration) lands **before release** if X4 shows a watcher that is deaf but Started, or one with no Stopped event.
8. **Ear pause:**
   - Requires all of: Bound; `audio_connected`; our endpoint is the default render device; `!state.connected` (AAP wins).
   - A change must hold for 2 consecutive frames from the bound group. Charging or in-case counts as out.
   - Feeds `core::ear::EarPause` (we_paused semantics).
   - **Scanning while connected:** the default after X6 is to scan while `!audio_connected || !any_render_active`. That keeps the le.rs:5-8 promise but gives no BLE ear pause during playback. If X6 is clean, scan always (decision D3).
9. **Later, not in these steps:** IRK/ENC_KEY import (AES-128-ECB on `tail`, RPA resolution with the byte-reversed IRK) adds a Proven tier, exact battery and the last-host octets. The iPhone heart-rate bridge uses WinRT GATT 0x180D/0x2A37.

## 5. Commit-sized steps

**Hardware legend:**
- **AirPods** = needs the owner's paired Pro 3.
- **PC** = needs this PC's Bluetooth radio but no buds.
- **none** = gate.ps1 only.

Every Rust step runs `gate.ps1`. WinUI steps also run `dotnet build windows/winui/Podatui.WinUI.sln`.

| # | Title | Files and what | Tests | HW |
|---|---|---|---|---|
| 1 | core: proximity parser | `core/src/proximity.rs`, `lib.rs`: Advert, parse, parse_identity, model_name. | Every vector in advert-format §3. Pro 3 #1: L80/R80, not in case, left sends. Earshot 0x2B: both in ear, 90/90. MacMix 0x0B/0x2B. CAPod #31: all charging, lid open, counter 1. #164 x2: case 80, right charging. Reddit Pro 2: L100/R90. Apple Pro 1: right primary in case. The 4 reject frames give None. Bit-4-only gives Unknown. Multi-record and truncated buffers. | none |
| 2 | le.rs: Sighting callback, 0x004C filter, Status()-driven self-heal | `le.rs`; adapt the closure at `main.rs:2228-2264` (driver mode only, gated on parse). | gate. Toggle Bluetooth off/on in Settings: the log shows the watcher recreated. | PC |
| 3 | bt.rs: device_props | `bt.rs`: PID, container ID and HFP battery via CM. | Unit tests for BTHENUM instance-ID parsing (the `DEV_<MAC>` and `{0000111E` forms). | none (values checked in X1) |
| 4 | `podatuid --probe` (drop 1) | `main.rs` branch before the mutex, `probe.rs`. Writes an inventory header (mac, name, PID, container, HFP battery, AAP key presence, watcher status), ADV/IDENT lines, RENDER edges, a 2 s ACL hint line, watcher Status changes, and NOTE from typed stdin. Output goes to stdout and `%LOCALAPPDATA%\podatui\probe-<utc>.txt`. **Ship it as a zip**: podatuid.exe plus avcodec-61, avutil-59 and swresample-5. | Formatter round-trip test (a line parses back into a replay event). Manual run on this PC. | PC |
| 5 | core: EarPause extraction | `core/src/ear.rs`; `main.rs:1835-1865` calls it. No behaviour change. | Seed gives no action. Single removal while playing gives Pause. Not playing gives nothing. Both back in after we paused gives Resume. Not paused by us gives nothing. None holds the previous value. | none |
| 6 | Daemon driverless skeleton | `driver.rs` interface_present; mode switch; skip the driver threads and the old closure; inert AAP commands; `Ctx.mac` atomic (8 sites) with a 30 s re-poll; `Snapshot.driverless`. | IPC: old JSON parses as driverless=false. Pure `mode(interface_present)` test. On this PC (no driver): 2 min of log with no `driver open FAILED` and no Fix Driver overlay. Pairing while the daemon runs shows the mac. | PC |
| 7 | IPC: driverless fields | `ipc/src/lib.rs`: audio_connected, ble_match, product_id; ConnectPrompt battery, product_id, busy. | Serde round-trip and old-JSON tests for each. | none |
| 8 | endpoint.rs: container join and state notifications (probe drop 2) | `endpoint.rs`, Cargo features; `Snapshot.audio_connected`; the probe gains EP lines. | Unit test for the container-GUID compare. Connect/disconnect from Settings flips audio_connected within 1 s. | AirPods |
| 9 | Owner experiments X1/X2/X4/X6; commit fixtures | `core/tests/fixtures/*.txt` with the MAC scrubbed to `11:22:33:44:55:66` by a pwsh `-replace`. `core/tests/replay.rs` harness. Record the constants. | Every ADV line parses or is a known reject. After a "lid open" NOTE, the next trustworthy frame is Open. Sibling behaviour asserted per X2c. | AirPods |
| 10 | core: podmatch Tracker | `core/src/podmatch.rs` as in §1. | Synthetic: lone pair (edge then LinkUp gives Prompt, then Bound); stranger at -58 gives Prompt and a failed verify gives one busy Prompt, the next edge re-arms; two groups within 8 dB give Ambiguous; siblings merge (rules A and B); rotation 1 step follows, 2 qualifiers give Ambiguous; B2 gives Weak with no Battery or Ear outputs; Weak promoted by 2 continuity hand-offs; guard D blocks AutoConnect but not Prompt; byte10!=0 gives Prompt only; bit-4 phantom gives no edge; mac==0 gives inert. Replay: the owner fixture reaches Bound after EP 1. Merged with a remapped copy at -10 dB, the lock stays on the owner; at 0 dB, Ambiguous. | none |
| 11 | ble_link.rs wiring | `ble_link.rs`, main.rs spawn. Tracker outputs go to Snapshot, ConnectPrompt and the case overlay. Auto-connect is compiled in but `AUTO_CONNECT=false` (prompt-only). | Replay test feeding the channel `In` events. Owner checklist: 10 case-opens at the desk give 10 prompts; a closed case stays silent; the battery appears on connect. | AirPods |
| 12 | WinUI driverless UI | Files in §2 WinUI. | dotnet build. Manual against the live driverless daemon: cards hidden, prompt battery and art, "Several AirPods nearby", island on AudioConnected. | AirPods |
| 13 | setup-common: pnputil device removal | Uninstall-PodatuiDrivers without devcon. | VM with the old MSI: run the function with devcon absent; `Get-PnpDevice` shows no ROOT\PodatuiMicPC or ROOT\AudioCodec. | VM |
| 14 | Installer, CI, docs (release R1) | §3 files plus dev-drivers.ps1. | CI builds the MSI and zip. Owner PC (Secure Boot on): install, no reboot, `mode: driverless`. VM, old MSI, Test Mode off, then upgrade: `pnputil /enum-drivers` shows no podatui package, Root has no cert, `schtasks` shows no tasks. VM with Test Mode on: everything kept. Uninstall removes the Run values. | VM and PC |
| 15 | KS oneshot connect/disconnect | `endpoint::oneshot`, `--probe-connect`; Connect, Disconnect and RebuildAudio in driverless mode; 10 s verify and busy prompt. | Unit tests for the filter-ID prefix and dedupe. **X3 matrix.** Disconnect leaves no Code 38 in Device Manager. | AirPods |
| 16 | Enable gated auto-connect | Flip `AUTO_CONNECT` per X3 and X2f; §1.7 rules. | Tracker unit tests already cover the gates. Owner: case-open with no host connects; with the iPhone playing, prompt only. | AirPods |
| 17 | Default render endpoint follows | `endpoint::set_default`, `--probe-default`. | Unit test for the role-skip logic. Speakers default, then connect: switches for 3 roles. Pick speakers manually: no switch-back. Capture default unchanged, A2DP kept. | AirPods |
| 18 | BLE ear pause | ble_link feeds EarPause under the §4.8 gates; scan policy per D3. | 20 removals per bud while playing: log latency and misses (#329). | AirPods |
| 19 | power.rs (only if X4 requires it, before step 14 ships) | `power.rs`, windows-sys `Win32_System_Power`. | 5x S3 sleep/resume and 5x radio toggle: adverts resume within 5 s and a case-open still prompts. | PC and AirPods |

**Order constraints:**
- 1-4 ship the probe immediately.
- 5-8 do not depend on probe data.
- 10 waits for 9.
- 14 can land after 12; it is the first installable product for the owner.
- 16 waits for X3.
- 18 waits for X6.
- 19 is decided by X4 before 14 is released.

## 6. Owner experiments

- **X0, now: pair the Pro 3 in Settings.**
- **X1 (15 min, right after pairing, before steps 8 and 10).** In pwsh:
  ```
  Get-PnpDevice | ? InstanceId -match '<MAC>' | ft InstanceId,Status
  Get-PnpDeviceProperty -InstanceId 'BTHENUM\DEV_<MAC>*' -KeyName '{2BD67D8B-8BEB-48D5-87E0-6CDA3428040A} 8'
  ```
  The second command must return 8231 (0x2027) **on the DEV_ node itself**. Also read key 7 (VID). Then:
  - Read `{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2` on the `{0000111E` node while connected and while disconnected. Compare it with the iPhone's L, R and case readings.
  - Read the probe drop-1 inventory: container ID and AAP key `{74ec2172}`.
  
  Gates: the model gate (falls back to learn-at-anchor), the Weak-tier HFP display, and the find_airpods path.
- **X2 (probe, 1-2 h, with NOTE labels typed for each action):**
  - **a)** Closed case for 2 min: silence, or only 0x11 variants?
  - **b)** Lid open/close 5x each at 0.3 m, 2 m, 5 m and the next room: RSSI distribution and edge behaviour. **Repeated with probe drop 2** after step 8 for the edge-to-EP latency over 10 runs, with the PC as last host and with the iPhone as last host. This sizes the B1 window.
  - **c)** Addresses per pair: lid open with both in case, one bud out, both out, both worn. Count the distinct addresses, and check whether siblings swap this/other nibbles and share the lid counter and case nibble. This chooses sibling rule A or B.
  - **d)** Connected and idle, start/stop PC playback: does byte 10 follow 0x04/0x05 within 3 s? This enables B4.
  - **e)** Worn and connected for 30 min: rotation interval and battery/RSSI deltas across rotations. Sets the continuity limits.
  - **f)** Buds on the iPhone (idle, then playing), then on no host: byte 10 values. This gates §1.7.
  - **g)** Left and right bud out/in, 10x each while playing: bit latency and #329 misses.
  - **h)** Does a prefix-0x07 frame on connect carry a Public address equal to the paired MAC? This enables B3. Also check whether any 0x07/0x0F frame appears.
- **X4 (probe drop 1, before step 14 ships):** one S3 sleep/resume and one Bluetooth toggle. Does the watcher go Aborted or Stopped, and how fast do adverts return? This decides step 19.
- **X5 (optional):** any second pair of AirPods present, recorded as a real ambiguity fixture.
- **X6 (10 min A2DP music with the probe scanning, then 10 min without, judged by ear):** decides D3.
- **X3 (after step 15, `--probe-connect`):**
  - lid open, PC as last host;
  - iPhone as last host, idle;
  - iPhone playing;
  - closed case;
  - out of range.
  
  Log the time to ACTIVE and whether the iPhone loses audio. This decides step 16.

**Fixtures:** before committing, scrub the paired MAC and drop non-0x07 lines.

## 7. Owner decisions (recommended default first)

1. **D1, installer (product).** Drivers are dev-only through `dev-drivers.ps1`. Migration cleanup runs only when Test Mode is off. The alternative is to always strip drivers, which breaks dev boxes on every upgrade.
2. **D2, auto-connect (product).** Prompt-only until X3. Afterwards, automatic only when byte 10 == 0x00 and guard D passes; otherwise prompt.
3. **D3, scanning during playback (dependency/product).**
   - Default: stop scanning while A2DP renders if X6 shows static. That means no BLE ear pause during playback.
   - Option: duty-cycle the scan (2 s on, 6 s off, about 8 s latency).
   - Option: upgrade `windows` 0.58 to 0.62 for `ScanParameters::CoexistenceOptimized` (0.62.2 is already in the cargo cache). Choose this only if you want ear pause during playback.
   - If X6 is clean, scan always.
4. **D4, release cut (product).** Ship R1 after step 14, with Connect still on BluetoothSetServiceState. When the PC was last host, Windows pages the buds itself. KS connect (step 15) follows in the next release. The alternative is to hold the release until step 15.
5. **D5, learned model persistence.** Default: memory only. Revisit (`pods.json`) only if X1 shows the PID is missing.
6. **D6, CI.** Keep the driver build as a separate job that feeds no packaging, to catch rot. The alternative is to drop it and save the WDK install time.
7. **D7, FFmpeg.** Keep shipping the 3 DLLs; putting `eld` behind a cfg is a later change.
8. **D8, fixtures in git.** Commit the scrubbed fixtures. The alternative is to keep them local and test with synthetic data only.
9. **D9, zip installer elevation.** Keep admin for this release. The alternative is to elevate only when migration items exist.
10. **D10, IPC semantics.** `connected` stays AAP and `audio_connected` is new. The alternative, redefining `connected` in driverless mode, is simpler for the UI but changes driver-mode meaning.
11. **D11, mode detection.** Startup only, logged. The alternative is to re-check on BTHPORT arrival.

## 8. What this plan does not verify

- No byte layout has been checked on the owner's 9A348 Pro 3 (X2).
- ONESHOT_RECONNECT, IPolicyConfig and the power callbacks have not been run on 26300 or the RZ616.
- It is unknown whether HFP battery exists for the Pro 3.
- It is unknown whether DevicePID sits on the DEV_ node.
- Pro 3 left-bud ear removal is sometimes not reported (#329), so some pauses will be missed.
- IPolicyConfig is undocumented and could break with a Windows update; errors are logged and never fatal.
- `daemon.log` still has no rotation. Driverless mode removes the main spam source, and Tracker logs are one line per state change.