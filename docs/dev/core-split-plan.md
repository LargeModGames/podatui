# podatui core split: final implementation plan

This plan starts from the "Minimal extraction" design, which won with 238 points (86 + 68 + 84). Sans-IO scored 199 and backend-traits scored 146. The judges' grafts are folded in and every mustFix item is resolved. Line numbers were checked against HEAD 0053c36.

## Ground truth checked this session (read-only)

**Baseline gate at 0053c36 is green.** I ran it with `CARGO_TARGET_DIR` pointing into the scratchpad, so the repo's `target/` was not touched:
- `windows/daemon`
  - `cargo fmt --check`: exit 0
  - `cargo clippy --all-targets -- -D warnings`: `Finished dev profile ... in 7.49s` (clean)
  - `cargo test`: `test tests::utc_timestamp_is_iso8601 ... ok` / `1 passed; 0 failed`
  - `RUSTFLAGS='-C target-feature=+crt-static' cargo build --release`: `Finished release profile [optimized] target(s) in 16.00s`, and `podatuid.exe` is 1,868,800 bytes. This is the size baseline for every later move commit.
- `windows/ipc`
  - fmt: exit 0
  - clippy: clean
  - `cargo test`: `running 0 tests ... ok`

**The move is lint-clean.** I copied `aap.rs` and `hr.rs` verbatim into a scratch lib crate with no dependencies. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` were both clean with zero edits. So once the files leave the cover of `main.rs:7 #![allow(dead_code)]`, no private item becomes dead, and no `#[allow]` or deletion is needed (judge 1 mustFix 7, judge 3 mustFix 4).

**Files under `core/` are not ignored today.** The only Rust ignore rule is `windows/.gitignore` (`**/target/`), so a top-level `core/target/` would show up as untracked. `windows/ipc` has no committed `Cargo.lock`, and nothing ignores one. Running cargo in `windows/ipc` created an untracked `windows/ipc/Cargo.lock`; I deleted it, and the tree is clean at 0053c36.

**Why there is no workspace:**
- `ci-windows.yml:151` caches `windows/daemon -> target`.
- `:158` builds in `windows/daemon`.
- `:208` passes `windows/daemon/target/release/podatuid.exe`.
- `make-dist.ps1:49` defaults to `daemon\target\x86_64-pc-windows-gnu\release\podatuid.exe`.

A workspace would move `target/` and break all four.

**`hearing.rs:8` and `hrdb.rs:21` stay as they are.** They are the only `crate::aap` / `crate::hr` users. A private `use podatui_core::{aap, hr};` at the crate root keeps both paths resolving.

**`aap::Battery` and `ipc::Battery` are not the same type.**
- `aap::Battery` (aap.rs:428): `Default, Clone, Copy`, no serde, `headphone` first.
- `ipc::Battery` (ipc lib.rs:25): adds serde and `#[serde(default)]` on the charging flags, `left` first.
- `merge_battery` (main.rs:1679-1694) converts field by field.

**Concurrency details the plan must keep:**
- `conv_duck` is locked separately at 1036, 1796 and 1798. `volume::set` runs inside `ConvDuck` (volume.rs:148-181) under that lock.
- The ear block (1823-1864) shows the overlay before `media::is_playing()` is called. A single-call API that runs the probe inside core would reverse that order, so the plan uses a two-phase API.

## 1. Crate layout

```
E:\Code\podatui\
  core\                 podatui-core (lib, edition 2021, ZERO dependencies through the whole refactor)
    .gitignore          /target/  (+ Cargo.lock per owner decision D1)
    Cargo.toml
    src\lib.rs          pub mod aap; pub mod hr; pub mod policy;
    src\aap.rs          git mv from windows/daemon/src/aap.rs (100% rename, tests travel with it)
    src\hr.rs           git mv from windows/daemon/src/hr.rs
    src\policy.rs       grows by one item per commit
    tests\no_os_deps.rs ratchet: scans Cargo.toml AND src/**/*.rs for windows, windows_sys, bluer, dbus, tokio, rusqlite, serde
  windows\ipc\          podatui-ipc, unchanged in production code; gains golden tests
  windows\daemon\       podatuid; depends on ../../core and ../ipc; all OS modules stay
```

**Dependency rule:** core depends on nothing, ipc depends on serde only, and the daemon depends on both.
- Core never depends on ipc, and ipc never depends on core. Battery unification has been dropped (judge 3 mustFix 2), so the inversion judge 1 objected to never happens. Because of that, judge 2's point 6 (move ipc to the root before core depends on it) does not apply.
- ELD (`eld.rs` FFI plus `build.rs:5-24`) stays in the daemon. So do `hrdb` (rusqlite), `hearing`, `gatt`, `micpipe`, `a2dp`, and the case-open edge (`last_case_present`, declared at main.rs:1278 in run_receiver's outer scope and reset only at 2066). A later `podatui-eld` crate serves Linux.
- Later, outside this refactor: `git mv windows/ipc ipc` when the TUI lands, then `tui/` (depends on ipc, plus core only if it decodes AAP itself) and `linux/daemon/` (bluer, depends on core and ipc).

## 2. Core API sketch (`core/src/policy.rs`, final state)

Rules for the whole API:
- No traits, no runtime, no logger, no clock.
- Time arrives as `now: Instant`.
- Conditional COM probes arrive as `impl FnOnce`, so the call counts and short-circuit order stay what they are today.
- Core returns decisions. The daemon logs and executes them at today's call site, under today's lock.
- Every `pub fn new()` gets a matching `Default` so clippy's `new_without_default` stays quiet in the lib.

```rust
// ConvDuck: volume.rs:133-187
#[derive(Default)]
pub struct ConvDuck { original: Option<u8>, started: bool }
impl ConvDuck {
    /// Volume to set, if any. `current` is called only for status 1 (volume.rs:148).
    pub fn on_status(&mut self, status: u8, current: impl FnOnce() -> Option<u8>) -> Option<u8>;
    pub fn restore(&mut self) -> Option<u8>;
}

// Tier and reclaim: main.rs:528-550, 600-613, 637, 648, 2042-2047
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier { Call, Playback, Background }
impl Tier {
    pub fn defends_audio(self) -> bool;
    pub fn label(self) -> &'static str;
    /// `playing` = `|| media::is_playing() || volume::any_render_active()`, not called when mic_on.
    pub fn of_host(mic_on: bool, playing: impl FnOnce() -> bool) -> Tier;
}
pub const RECLAIM_COOLDOWN: Duration;
pub const RECLAIM_BACKOFF_MS: [u64; 8];
pub fn reclaim_cooling(last_reclaim: Option<Instant>, now: Instant) -> bool;

// ANC echo filter: main.rs:1745-1750
pub fn anc_echo_accepted(mode: u8, pending: Option<(u8, Instant)>, now: Instant) -> bool;

// Ear tracking: main.rs:1823-1864. Two-phase, so the overlay still precedes the SMTC probe.
pub enum MediaAction { Play, Pause }
pub struct EarTracker { prev_status: [EarStatus; 2], prev_ear: [bool; 2], we_paused: bool }
impl EarTracker {
    pub fn new() -> Self;   // [Disconnected; 2], [false; 2], false
    /// 1823-1832: true at most once per packet when a bud newly enters the case; updates prev_status.
    pub fn case_entered(&mut self, now: [EarStatus; 2]) -> bool;
    /// 1836-1864: transitional hold, then the decision. `is_playing` is called only when
    /// the state changed && !all_in && was_wearing.
    pub fn wear_changed(&mut self, now: [EarStatus; 2], is_playing: impl FnOnce() -> bool) -> Option<MediaAction>;
    pub fn both_out(&self) -> bool;  // for the status-diag log at main.rs:1951
}

// BLE proximity gate: main.rs:2235-2252. `busy` is evaluated after the re-arm and prompted
// checks, while the caller still holds the prox mutex.
pub struct Proximity { prompted: bool, last_seen: Instant }
impl Proximity {
    pub fn new(now: Instant) -> Self;
    pub fn on_advert(&mut self, now: Instant, busy: impl FnOnce() -> bool) -> bool;
}

// Low-battery hysteresis: main.rs:1711-1737. Two calls keep the daemon's two separate state reads.
#[derive(Default)]
pub struct LowBattery { bud_warned: bool, case_warned: bool }
impl LowBattery {
    pub fn buds(&mut self, min: u8) -> bool;   // warn at <=20, re-arm at >25
    pub fn case(&mut self, pct: u8) -> bool;   // warn at <=15, re-arm at >20
}

// Keepalive: main.rs:1446-1453, 1892-1931
pub enum AckOut { Recovered, Missed { n: u32, dead: bool } }
pub struct Keepalive { last_probe: Instant, pending: Option<(Instant, Instant)>, misses: u32 }
impl Keepalive {
    pub fn new(now: Instant) -> Self;
    /// 5 s answer window against the `last_data > baseline` comparison; `dead` at 2 misses, misses reset.
    pub fn check_ack(&mut self, now: Instant, last_data: Instant) -> Option<AckOut>;
    /// Once 30 s have passed and nothing is pending: resets the cadence, calls `anc`, and calls
    /// `active` only if anc != 0. Returns the mode to send. Mirrors main.rs:1920-1925 exactly.
    pub fn probe_due(&mut self, now: Instant, anc: impl FnOnce() -> u8, active: impl FnOnce() -> bool) -> Option<u8>;
    pub fn probe_sent(&mut self, now: Instant, baseline: Instant);
}

// Link watch: main.rs:1478-1486, 1525-1530, 1956-2039
pub struct DownActions { pub rearm_mic: bool, pub became_stale: bool, pub nudge_due: bool, pub release: bool }
pub struct LinkWatch { fails: u32, stale: bool, last_nudge: Instant, last_mic_rearm: Instant }
impl LinkWatch {
    pub fn new(now: Instant) -> Self;
    pub fn fails(&self) -> u32;             // status diag
    pub fn status_ok(&mut self);            // 1958-1959
    /// `os_connected` = bt::is_connected(mac), called by the daemon once per down tick, as at 1981.
    pub fn status_down(&mut self, now: Instant, os_connected: bool, mic_on: bool, last_data: Instant) -> DownActions;
    pub fn data_arrived(&mut self) -> bool; // true = stale flag just cleared (1525-1530)
}

// Data
pub const SETUP: [(&[u8], u64); 3];          // main.rs:1372-1376 handshake, (frame, delay_ms after); lives in core::aap
pub const HR_INIT: [(&[u8], u64); 4];        // main.rs:931-936
pub const HR_RETRY_BACKOFF_MS: [u64; 3];     // plus HR_MAX_ATTEMPTS, HR_START_COMMAND_DELAY_MS, HR_FIRST_SAMPLE_TIMEOUT_MS
pub fn hr_seq(n: u16) -> u16;                // 128 + n % (16_384 - 128); the HR_SEQ static stays in the daemon
pub fn hr_backoff_ms(attempt: u32) -> u64;
```

**Left out on purpose:**
- `MicAuto` (poll_mic 2099-2138) serves the Windows-only virtual mic, so moving it is churn (judge 3 mustFix 5).
- `battery_text`, `merge_battery`, `frame` and `is_setup` stay in the daemon, typed against `ipc::Battery`, and get daemon-local tests.

## 3. Backend boundary

**No traits in this refactor.** The "backend" is the daemon's OS modules, called directly as they are today.

**What crosses into core:**
- packet bytes
- scalars read from `Ctx`
- `now`
- lazy probes as closures

**What comes back:** `Option<u8>`, `bool`, `Option<MediaAction>`, `AckOut`, `DownActions`.

**What does not change:**
- the `Ctx` atomics and mutexes (main.rs:137-207)
- the thread layout
- COM init sites (295, 701, 1262-1263)
- the HR_SEQ and GATT_PROBED statics
- the `bt.rs` AUDIO_GATE, the `hearing.rs` HEARING_STATE, and the `devnode.rs` LAST_FIRED

All of these stay single-instance in the daemon, so do not make anything multi-device.

**When `AapLink` gets added:** `AapLink { send, recv }` (mirroring driver.rs:99/104) is added only in the BlueZ commit, and only if the two receive loops end up sharing a body. Key storage, BLE scanning, media control and audio routing stay as per-backend free functions until a second implementation exists.

## 4. IPC and frontends

**The daemon stays the only owner of the AAP link.** The NDJSON pipe pair and the L2CAP proxy pipes (ipc lib.rs:12-22) are unchanged.

**The wire format is frozen and pinned by golden tests in step 2.** On the C# side:
- WinUI decodes events by hand: `GetStr(root, ...)` at Messages.cs:270-276.
- It serializes commands with `JsonSerializer` and `Wire.Json` (Messages.cs:233, 245).
- No `UnmappedMemberHandling` is set, so System.Text.Json ignores unknown members. That makes additive `#[serde(default)]` fields safe.

**TUI and Raven GUI.** Later, these are clients of `podatui-ipc` through a std-only `client` module:
- `connect() -> io::Result<(mpsc::Receiver<Event>, CommandSink)>`.
- On Windows, `std::fs::OpenOptions` opens the two pipe clients, so no windows-sys is needed in ipc.
- On Unix, `$XDG_RUNTIME_DIR` sockets.
- The TUI links core only if it decodes AAP itself.

**Case-open popup.** It can use the existing `Event::ConnectPrompt`, which WinUI already handles (MainWindow.xaml.cs:83) but which is never emitted, so no wire change is needed.

## 5. Gate (no workspace, so three directories)

`cargo test` in `windows/daemon` does not run the core or ipc tests. The gate, in pwsh, is:

```powershell
foreach ($d in 'core','windows\ipc','windows\daemon') {
  Push-Location "E:\Code\podatui\$d"
  cargo fmt --check; if ($LASTEXITCODE) { throw "fmt $d" }
  cargo clippy --all-targets -- -D warnings; if ($LASTEXITCODE) { throw "clippy $d" }
  cargo test; if ($LASTEXITCODE) { throw "test $d" }
  Pop-Location
}
Push-Location E:\Code\podatui\windows\daemon
$env:RUSTFLAGS = '-C target-feature=+crt-static'; cargo build --release; $rc = $LASTEXITCODE
Remove-Item Env:RUSTFLAGS; Pop-Location; if ($rc) { throw 'release' }
(Get-Item E:\Code\podatui\windows\daemon\target\release\podatuid.exe).Length
```

Before step 4, drop `core` from the list. Paste the raw tails for each commit.

## 6. Commit-sized steps

Each step lists the files it touches, what changes, how it is verified, the expected risk, and whether it adds tests. Every step runs the gate in section 5. Every move commit also checks that `git diff -M --stat` shows a 100% rename and compares the exe size against 1,868,800 bytes. Every policy step also gets a manual hardware check plus a `daemon.log` diff of a real connect, before versus after.

### 0. Baseline (no commit)
- **What:** Already done in this session (see the top). Re-run it on the day of implementation and record the raw tails.
- **Risk:** None.

### 1. Pin `aap.rs` in place (adds tests)
- **Files:** `#[cfg(test)] mod tests` at the bottom of `windows/daemon/src/aap.rs`. No production change.
- **Vectors:**
  - `parse_connected_devices` on the two 0x2E captures (aap.rs:228-229). This pins the `data[8]` count offset as it is.
  - `sensor_stream(152, STREAM_HEART_RATE_LEGACY, 1_000_000)` against the aap.rs:130 frame.
  - Battery: partial, full, and charging bytes 0x01 and 0x05.
  - Ear: bytes 0x00-0x04 and an unknown byte, plus `is_transitional`.
  - `parse_anc_mode(d) == parse_control_value(d, 0x0D)`.
  - CA 0x4B.
  - Metadata, including the empty path (aap.rs:555).
  - `build_rename` / `parse_rename` round trip.
  - Byte layouts for `anc_command`, `control_command` and `feature_command`.
  - `is_audio_packet` and `for_each_au`.
  - Short and wrong-header rejection for every parser.
- **Verify:** daemon tests go from 1 to about 21.
- **Risk:** None at runtime. If a vector disagrees, pin the current behaviour.
- **Size:** about 200 lines, all tests. Flag this against the 200-line rule.

### 2. IPC wire goldens (adds tests)
- **Files:** `windows/ipc/tests/wire.rs`.
- **What:**
  - Literal JSON for `Snapshot::default()`.
  - A fully populated Snapshot, including a populated Battery and the multipoint, model, link_stale and mic_available fields.
  - Every `Event` variant, including ConnectPrompt.
  - `from_line::<Command>` for every variant.
  - Backward-compat: `from_line` of Snapshot and Battery JSON that lacks the `#[serde(default)]` fields (the charging flags, link_stale, mic_available, model, multipoint).
  - The literals are generated from current serde output and then cross-checked by hand against the Messages.cs records, the event switch (270-276) and the command serialization (245). The commit message says exactly that and does not claim they were captured from C#.
- **Verify:** ipc goes from 0 tests to about 18.
- **Risk:** None.

### 3. Daemon-local helper tests (adds tests)
- **Files:** `#[cfg(test)]` in `main.rs`, `bt.rs`, `hearing.rs` and `hrdb.rs`.
- **What:**
  - `frame()` (main.rs:57)
  - `is_setup` (465)
  - the `battery_text` table (106)
  - `kind_from_cod` (bt.rs:112)
  - the hearing ATT PDU builders and `put_f32`
  - `hrdb::format_mac` (123)
  - the hrdb writer against `Connection::open_in_memory`, but only if the writer accepts a `Connection` without a production change. Otherwise skip it and list it as unverified.
- **Verify:** gate.
- **Risk:** None.

### 4. Create `podatui-core` and move `aap.rs` (ratchet test added)
- **Files:**
  - New `core/Cargo.toml` (no dependencies), `core/src/lib.rs` (`pub mod aap;`) and `core/.gitignore` (`/target/`, plus the D1 decision).
  - `git mv windows/daemon/src/aap.rs core/src/aap.rs`.
  - Daemon `Cargo.toml` gets `podatui-core = { path = "../../core" }`.
  - `main.rs:12`: `mod aap;` becomes `use podatui_core::aap;`. hearing.rs is untouched, because `crate::aap` resolves through the root `use`.
  - `core/tests/no_os_deps.rs` ratchet.
- **Verify:**
  - Gate in all three directories; the aap tests now run in core without FFmpeg.
  - `cargo tree` in core shows no dependencies.
  - 100% rename; exe size comparison.
- **Risk:** Low. The scratch clippy run proved aap.rs builds lint-clean as a lib.

### 5. Move `hr.rs` (adds tests)
- **Files:**
  - `git mv` to `core/src/hr.rs`; add `pub mod hr;`.
  - `main.rs:19` becomes `use podatui_core::{aap, hr};`. hrdb.rs:21 is untouched.
- **Tests:** synthetic frames from the module constants (prefix hr.rs:72, services 84/20/19, length 18, BPM offset 1, confidence 0x80, BPM 30-220, status tails hr.rs:27):
  - a valid frame yields a sample
  - a 2-3 chunk split reassembles
  - confidence 0x7F and BPM 29/221 are rejected
  - garbage before the prefix is skipped
  - `reset` drops the carry
  - `contains_frame_prefix` both ways
- **Verify:** gate, 100% rename, exe size.
- **Risk:** Low. The synthetic frames pin the code, not the firmware.

### 6. `ConvDuck` into core (adds tests)
- **Files:** `core/src/policy.rs` (new, `pub mod policy;`), `volume.rs` (loses ConvDuck), `main.rs:178`, `2186`, `1036`, `1796`, `1798`.
- **Call sites:** bind the guard explicitly so `volume::set` stays inside the lock:
  ```rust
  let mut d = ctx.conv_duck.lock().unwrap();
  if let Some(v) = d.on_status(status, volume::get) { volume::set(v) }
  ```
  `restore()` uses the same pattern at 1036 and 1796.
- **Tests:**
  - status 1 at 60 returns 25; at 20 it returns None
  - a repeated 1 keeps the first original
  - 2 returns 15 only when original > 15
  - 3 returns `min(orig, 25)` only once started
  - 4, 6 and 7 restore and clear
  - `restore` when idle returns None
  - a panicking `current` for statuses other than 1
- **Verify:** hardware: CA duck and restore, and CA off mid-duck.
- **Risk:** Lock scope, which the explicit binding addresses.

### 7. `Tier`, `RECLAIM_*` and `reclaim_cooling` (adds tests)
- **Files:**
  - `main.rs:528-550` and `637/648` move to core.
  - `host_tier` (600-613) becomes `Tier::of_host(ctx.mic_on.load(..), || media::is_playing() || volume::any_render_active())`.
  - 2042-2047 uses `reclaim_cooling`.
  - `spawn_reclaim` is otherwise unchanged.
- **Tests:** the tier table; the closure is not called when mic_on; the backoff sum; cooling edges.
- **Verify:** hardware: link loss while playing reclaims; while idle it does not.
- **Risk:** Minimal.

### 8. `anc_echo_accepted` (adds tests)
- **What:** replaces the match at 1745-1750 with `anc_echo_accepted(m, *ctx.anc_cmd.lock().unwrap(), Instant::now())`.
- **Tests:** a mismatch inside 1.5 s is rejected; a match is accepted; after the window it is accepted; None is accepted.
- **Verify:** hardware: fast ANC switching shows no Off flicker.
- **Risk:** Minimal.

### 9. `EarTracker` (adds tests)
- **What:** replaces the locals at 1432/1433/1458 and the block at 1823-1864. The daemon does:
  ```rust
  if t.case_entered(now) { ctx.overlay("AirPod in case") }
  match t.wear_changed(now, media::is_playing) {
      Some(Play) => { media::play(); }
      Some(Pause) => { media::pause(); }
      None => {}
  }
  ```
  The status diag at 1951 uses `t.both_out()`.
- **Tests:**
  - single removal while playing gives Pause
  - both back gives Play only if we paused
  - Transitional holds the previous state
  - both buds entering the case together give one true
  - `is_playing` is not called when !was_wearing, not on the all_in branch, and not when the state is unchanged (panicking closure)
- **Verify:** hardware: bud out pauses, bud in resumes, case gives one overlay.
- **Risk:** Ordering, which the two-phase API resolves.

### 10. `Proximity` (adds tests)
- **What:** replaces the tuple at 2232. The closure holds the prox guard across `busy`, which preserves the nested prox then state order at 2240-2248:
  ```rust
  let mut p = prox.lock().unwrap();
  if !p.on_advert(Instant::now(), || c.state.lock().unwrap().connected || c.connect_requested.load(Ordering::Relaxed)) { return; }
  drop(p);
  ```
  The rest is unchanged.
- **Tests:** re-arm after more than 45 s; no second fire in a visit; busy does not consume the prompt; `last_seen` updates while busy.
- **Verify:** hardware: opening the case while disconnected auto-connects.
- **Risk:** Low.

### 11. `LowBattery` (adds tests)
- **What:** replaces 1455-1456 and the warn logic at 1711-1737. The two state reads and the overlay order are unchanged.
- **Tests:** 21 to 20 warns once, 19 does not warn, 24 does not re-arm, 26 re-arms; the case at 15/20.
- **Verify:** hardware: one overlay per crossing.
- **Risk:** Minimal.

### 12. `Keepalive` (adds tests)
- **What:** replaces 1446-1453 and 1892-1931. The daemon maps `Recovered` to the "answered" log, `Missed { n, dead }` to the "miss n/2" log, and `dead` to the "link is dead" log plus `overlay("Restoring audio")` plus the `force_audio_rebuild` thread. The probe send is:
  ```rust
  if let Some(mode) = ka.probe_due(now, || ctx.state.lock().unwrap().anc, || ctx.mic_on.load(Ordering::Relaxed) || volume::any_render_active()) {
      let baseline = last_data;
      if driver.send(&aap::anc_command(mode)).is_ok() { ka.probe_sent(Instant::now(), baseline) }
  }
  ```
- **Tests (synthetic Instants):**
  - answered clears
  - a miss at 5 s
  - two misses give dead and reset
  - recovery only after a miss
  - the 30 s cadence resets even when no probe is sent
  - `anc` is not read before 30 s
  - `active` is not called when anc == 0
- **Verify:** a 24 h music soak shows no spurious "Restoring audio".
- **Risk:** Timing semantics, which the tests pin.

### 13. `SETUP` script and HR schedule data (adds tests)
- **What:**
  - `aap::SETUP` replaces the literal sequence at 1372-1376.
  - `HR_INIT`, the HR consts, `hr_seq` and `hr_backoff_ms` replace their inline equivalents at 810-815 and 909-1002. The HR_SEQ static and the campaign thread stay in the daemon.
- **Tests:** SETUP equals HANDSHAKE, SET_FEATURES and REQUEST_NOTIFS with 300 ms gaps; `hr_seq` stays in 128..16384 and wraps; the backoff table repeats its last value.
- **Verify:** hardware: HR starts within about 12 s on supported buds.
- **Risk:** Low.

### 14. `LinkWatch` (adds tests; the riskiest step, split if over 200 lines)
- **What:** replaces `status_fails`, `link_stale`, `last_link_nudge` and `last_mic_rearm`, plus 1525-1530, 1956-1959 and 1981-2039. The daemon executes in today's order:
  1. `rearm_mic`: log, then send START_AUDIO.
  2. `became_stale`: set the state flag, push_state, log.
  3. `nudge_due`: read anc; if it is not 0, log and send.
  4. `release`: the existing 2040-2089 block runs verbatim (host_tier, cooling, overlay, hr end, state reset, driver_cell = None, `hr_decoder.reset()`, `last_anc = 0`, `last_case_present = None`, audio_rebuild_pending, spawn_reclaim or clear connect_requested, push_state, break).
- **Tests:**
  - status_ok resets fails
  - os_connected=false three times releases. This is the regression test for the find_airpods bug at 1962-1980.
  - mic on and silent for 8 s re-arms, but not again within 5 s
  - mic off and silent for 30 s goes stale once, with a nudge every 30 s
  - `data_arrived` clears stale once
- **Verify:** hardware: casing the buds releases; an iPhone steal is reclaimed; the zombie link sets link_stale.
- **Risk:** Medium.
  - `mic_on` is now loaded once per tick instead of twice. That is a sub-microsecond race window, accepted and noted.
  - `now` injection replaces `elapsed()`. Same note.

**Later feature commits (behaviour changes, outside the refactor):**
- `REQUEST_KEYS` (0x30) and `parse_keys` (0x31)
- `core::ble` on `aes`: RPA resolve and decode
- widen le.rs:26's callback to `(addr, mfg, rssi)`
- DPAPI key store
- case-open popup via ConnectPrompt
- default endpoint via IPolicyConfig
- WM_POWERBROADCAST sets `wants_reconnect`
- `core::routing` takeover behind a feature that is off by default
- fixes for volume.rs:120 and driver.rs:153, each in its own commit
- ipc client module, `tui/`, `linux/daemon`, `podatui-eld`

## 7. Estimated size

- **Steps 1-3:** about +450 lines, all tests. Steps 1 and 2 each come close to the 200-line note.
- **Steps 4-5:** about +110 (manifest, lib.rs, .gitignore, ratchet, hr tests). The renames are net 0.
- **Steps 6-14:** about +130 net production lines and about +450 test lines. main.rs shrinks by about 250 lines.
- **Total:** about +1,150 net, about 80% of it tests. Every production commit stays under about 150 lines except possibly step 14.

## 8. Not verified here

- No hardware checks were run, and none of steps 1-14 were executed.
- The release build used the msvc target with crt-static, as CI does. The `x86_64-pc-windows-gnu` path that make-dist.ps1:49 defaults to was not built.
- `core` was compiled on Windows only, with clippy only; `cargo test` was not run on Linux.
- C# acceptance of the golden literals was reasoned from Messages.cs, not executed.

## Decisions for the owner (tooling, dependencies, CI)

- **D1. `Cargo.lock` policy for `core/`, and for `windows/ipc`, which already leaves an untracked lockfile when run standalone.**
  - Recommended: ignore both lockfiles via `core/.gitignore` and a `windows/.gitignore` line. The committed `windows/daemon/Cargo.lock` stays the one that pins the shipped binary.
  - Alternative: commit both.
- **D2. How the three-directory gate is run.**
  - Recommended: a committed `gate.ps1` at the repo root containing the section 5 loop.
  - Alternative: a documented one-liner only.
- **D3. CI.**
  - Recommended: one commit after step 4 that adds fmt, clippy `-D warnings` and test steps for core, ipc and the daemon to ci-windows.yml, plus an `ubuntu-latest` job that runs `cargo test` in `core/` to prove it is OS-free.
  - Alternative: keep CI release-only.
- **D4. Workspace.**
  - Recommended: none, because of ci-windows.yml:151/158/208 and make-dist.ps1:49.
  - Revisit when the TUI or Linux crates land.
- **D5. Core location.** The plan assumes a top-level `core/` (cross-platform). The alternative is `windows/core`, which avoids touching the root .gitignore but misplaces a cross-platform crate.
- **D6. Future dependencies (none are added in this refactor).** `aes` for IRK/RPA, `ratatui`/`crossterm` for the TUI, `bluer`/`tokio` for Linux. Later: whether ipc may take `windows-sys`; recommended no, using `std::fs::OpenOptions`.
- **D7. Dead code hidden by `main.rs:7`.** `a2dp.rs`, the unused aap builders and `gatt::uuid_str` are mentioned only, not deleted. Prune them in a separate approved commit if wanted.
- **D8. Real captures.** HR RTBuddy frames and more 0x2E packets would replace the synthetic vectors in step 5 and pin the `data[8]` vs LibrePods `packet[6]` offset before Linux depends on it.