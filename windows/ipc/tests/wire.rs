use podatui_ipc::{
    from_line, to_line, Battery, BleMatch, ClientKind, Command, ConnectedDevice, Event, Snapshot,
};

const DEFAULT_SNAPSHOT: &str = concat!(
    r#"{"connected":false,"link_stale":false,"dev_name":"","#,
    r#""battery":{"left":null,"right":null,"case":null,"headphone":null,"#,
    r#""left_charging":false,"right_charging":false,"case_charging":false,"headphone_charging":false},"#,
    r#""anc":0,"mic_recording":false,"mic_available":false,"auto_mode":false,"#,
    r#""conversational_awareness":false,"adaptive_volume":false,"allow_off":false,"#,
    r#""volume":0,"muted":false,"heart_rate":null,"model":"","firmware":"","serial":"","#,
    r#""multipoint":[],"driverless":false,"audio_connected":false,"ble_match":"none","product_id":0}"#
);

const FULL_STATE_EVENT: &str = concat!(
    r#"{"event":"state","connected":true,"link_stale":true,"dev_name":"AirPods Pro","#,
    r#""battery":{"left":80,"right":75,"case":40,"headphone":null,"#,
    r#""left_charging":false,"right_charging":true,"case_charging":true,"headphone_charging":false},"#,
    r#""anc":2,"mic_recording":true,"mic_available":true,"auto_mode":true,"#,
    r#""conversational_awareness":true,"adaptive_volume":false,"allow_off":true,"#,
    r#""volume":55,"muted":false,"heart_rate":72,"model":"A3063","firmware":"8A356","serial":"ABC123","#,
    r#""multipoint":[{"address":"aa:bb:cc:dd:ee:ff","is_this_pc":true,"name":"This PC","kind":"pc"},"#,
    r#"{"address":"11:22:33:44:55:66","is_this_pc":false,"name":"Unknown device","kind":"phone"}],"#,
    r#""driverless":true,"audio_connected":true,"ble_match":"bound","product_id":8231}"#
);

fn full_snapshot() -> Snapshot {
    Snapshot {
        connected: true,
        link_stale: true,
        dev_name: "AirPods Pro".into(),
        battery: Battery {
            left: Some(80),
            right: Some(75),
            case: Some(40),
            headphone: None,
            left_charging: false,
            right_charging: true,
            case_charging: true,
            headphone_charging: false,
        },
        anc: 2,
        mic_recording: true,
        mic_available: true,
        auto_mode: true,
        conversational_awareness: true,
        adaptive_volume: false,
        allow_off: true,
        volume: 55,
        muted: false,
        heart_rate: Some(72),
        model: "A3063".into(),
        firmware: "8A356".into(),
        serial: "ABC123".into(),
        multipoint: vec![
            ConnectedDevice {
                address: "aa:bb:cc:dd:ee:ff".into(),
                is_this_pc: true,
                name: "This PC".into(),
                kind: "pc".into(),
            },
            ConnectedDevice {
                address: "11:22:33:44:55:66".into(),
                is_this_pc: false,
                name: "Unknown device".into(),
                kind: "phone".into(),
            },
        ],
        driverless: true,
        audio_connected: true,
        ble_match: BleMatch::Bound,
        product_id: 0x2027,
    }
}

fn cmd(line: &str) -> Command {
    from_line::<Command>(line).unwrap_or_else(|| panic!("rejected: {line}"))
}

#[test]
fn default_snapshot_json() {
    assert_eq!(
        to_line(&Snapshot::default()),
        format!("{DEFAULT_SNAPSHOT}\n")
    );
}

#[test]
fn default_state_event_line() {
    let body = DEFAULT_SNAPSHOT.strip_prefix('{').unwrap();
    assert_eq!(
        to_line(&Event::State(Snapshot::default())),
        format!("{{\"event\":\"state\",{body}\n")
    );
}

#[test]
fn full_state_event_line() {
    assert_eq!(
        to_line(&Event::State(full_snapshot())),
        format!("{FULL_STATE_EVENT}\n")
    );
}

#[test]
fn state_event_parses_as_snapshot_ignoring_tag() {
    assert_eq!(
        from_line::<Snapshot>(FULL_STATE_EVENT),
        Some(full_snapshot())
    );
}

#[test]
fn overlay_event_line() {
    let ev = Event::Overlay {
        title: "Low battery".into(),
        body: "Left \"10%\"\nCase 5%".into(),
    };
    assert_eq!(
        to_line(&ev),
        "{\"event\":\"overlay\",\"title\":\"Low battery\",\"body\":\"Left \\\"10%\\\"\\nCase 5%\"}\n"
    );
}

#[test]
fn connect_prompt_event_line() {
    let ev = Event::ConnectPrompt {
        name: "AirPods Pro".into(),
        battery: None,
        product_id: 0,
        busy: false,
    };
    assert_eq!(
        to_line(&ev),
        "{\"event\":\"connect_prompt\",\"name\":\"AirPods Pro\",\"battery\":null,\"product_id\":0,\"busy\":false}\n"
    );
}

const FULL_CONNECT_PROMPT: &str = concat!(
    r#"{"event":"connect_prompt","name":"AirPods Pro","#,
    r#""battery":{"left":90,"right":null,"case":30,"headphone":null,"#,
    r#""left_charging":false,"right_charging":false,"case_charging":true,"headphone_charging":false},"#,
    r#""product_id":8231,"busy":true}"#
);

fn full_prompt_battery() -> Battery {
    Battery {
        left: Some(90),
        case: Some(30),
        case_charging: true,
        ..Battery::default()
    }
}

#[test]
fn full_connect_prompt_round_trips() {
    let ev = Event::ConnectPrompt {
        name: "AirPods Pro".into(),
        battery: Some(full_prompt_battery()),
        product_id: 0x2027,
        busy: true,
    };
    assert_eq!(to_line(&ev), format!("{FULL_CONNECT_PROMPT}\n"));
    let Some(Event::ConnectPrompt {
        name,
        battery,
        product_id,
        busy,
    }) = from_line::<Event>(FULL_CONNECT_PROMPT)
    else {
        panic!("not ConnectPrompt");
    };
    assert_eq!(name, "AirPods Pro");
    assert_eq!(battery, Some(full_prompt_battery()));
    assert_eq!(product_id, 0x2027);
    assert!(busy);
}

#[test]
fn connect_prompt_without_new_fields_parses() {
    let Some(Event::ConnectPrompt {
        name,
        battery,
        product_id,
        busy,
    }) = from_line::<Event>(r#"{"event":"connect_prompt","name":"AirPods Pro"}"#)
    else {
        panic!("old connect_prompt rejected");
    };
    assert_eq!(name, "AirPods Pro");
    assert_eq!(battery, None);
    assert_eq!(product_id, 0);
    assert!(!busy);
}

#[test]
fn ble_match_wire_names() {
    for (m, name) in [
        (BleMatch::None, "none"),
        (BleMatch::Ambiguous, "ambiguous"),
        (BleMatch::Likely, "likely"),
        (BleMatch::Weak, "weak"),
        (BleMatch::Bound, "bound"),
    ] {
        let json = format!("\"{name}\"");
        assert_eq!(to_line(&m), format!("{json}\n"));
        assert_eq!(from_line::<BleMatch>(&json), Some(m));
    }
}

#[test]
fn snapshot_without_driverless_fields_parses() {
    let without = |field: &str| {
        let old = FULL_STATE_EVENT.replace(field, "");
        assert_ne!(old, FULL_STATE_EVENT, "{field} not in fixture");
        from_line::<Snapshot>(&old)
    };
    assert_eq!(
        without(r#","audio_connected":true"#),
        Some(Snapshot {
            audio_connected: false,
            ..full_snapshot()
        })
    );
    assert_eq!(
        without(r#","ble_match":"bound""#),
        Some(Snapshot {
            ble_match: BleMatch::None,
            ..full_snapshot()
        })
    );
    assert_eq!(
        without(r#","product_id":8231"#),
        Some(Snapshot {
            product_id: 0,
            ..full_snapshot()
        })
    );
}

#[test]
fn hello_command() {
    assert!(matches!(
        cmd(r#"{"cmd":"hello","kind":"app"}"#),
        Command::Hello {
            kind: ClientKind::App
        }
    ));
    assert!(matches!(
        cmd(r#"{"cmd":"hello","kind":"tray"}"#),
        Command::Hello {
            kind: ClientKind::Tray
        }
    ));
}

#[test]
fn scalar_commands() {
    assert!(matches!(
        cmd(r#"{"cmd":"set_anc","mode":4}"#),
        Command::SetAnc { mode: 4 }
    ));
    assert!(matches!(
        cmd(r#"{"cmd":"set_mic_mode","auto":true,"manual":false}"#),
        Command::SetMicMode {
            auto: true,
            manual: false
        }
    ));
    assert!(matches!(
        cmd(r#"{"cmd":"set_feature","feature":40,"on":true}"#),
        Command::SetFeature {
            feature: 0x28,
            on: true
        }
    ));
    assert!(matches!(
        cmd(r#"{"cmd":"set_control","id":46,"value":100}"#),
        Command::SetControl {
            id: 0x2E,
            value: 100
        }
    ));
    assert!(matches!(
        cmd(r#"{"cmd":"step_volume","delta":-5}"#),
        Command::StepVolume { delta: -5 }
    ));
    assert!(matches!(
        cmd(r#"{"cmd":"set_volume","percent":30}"#),
        Command::SetVolume { percent: 30 }
    ));
    assert!(matches!(
        cmd(r#"{"cmd":"set_heart_rate","on":false}"#),
        Command::SetHeartRate { on: false }
    ));
}

#[test]
fn set_name_command() {
    let Command::SetName { name } = cmd(r#"{"cmd":"set_name","name":"Jay's \"Pods\""}"#) else {
        panic!("not SetName");
    };
    assert_eq!(name, "Jay's \"Pods\"");
}

#[test]
fn unit_commands() {
    assert!(matches!(
        cmd(r#"{"cmd":"toggle_mute"}"#),
        Command::ToggleMute
    ));
    assert!(matches!(cmd(r#"{"cmd":"connect"}"#), Command::Connect));
    assert!(matches!(
        cmd(r#"{"cmd":"rebuild_audio"}"#),
        Command::RebuildAudio
    ));
    assert!(matches!(
        cmd(r#"{"cmd":"repair_connection"}"#),
        Command::RepairConnection
    ));
    assert!(matches!(
        cmd(r#"{"cmd":"disconnect"}"#),
        Command::Disconnect
    ));
    assert!(matches!(cmd(r#"{"cmd":"get_state"}"#), Command::GetState));
    assert!(matches!(cmd(r#"{"cmd":"shutdown"}"#), Command::Shutdown));
}

#[test]
fn set_hearing_aid_command() {
    let line = concat!(
        r#"{"cmd":"set_hearing_aid","on":true,"#,
        r#""left_eq":[10,20.5,0,0,0,0,0,-5],"right_eq":[0,0,0,0,0,0,0,0],"#,
        r#""amplification":0.5,"balance":-0.25,"tone":0,"conversation_boost":true,"#,
        r#""ambient_noise_reduction":1,"own_voice":0.75}"#
    );
    let Command::SetHearingAid {
        on,
        left_eq,
        right_eq,
        amplification,
        balance,
        tone,
        conversation_boost,
        ambient_noise_reduction,
        own_voice,
    } = cmd(line)
    else {
        panic!("not SetHearingAid");
    };
    assert!(on);
    assert_eq!(left_eq, [10.0, 20.5, 0.0, 0.0, 0.0, 0.0, 0.0, -5.0]);
    assert_eq!(right_eq, [0.0; 8]);
    assert_eq!(amplification, 0.5);
    assert_eq!(balance, -0.25);
    assert_eq!(tone, 0.0);
    assert!(conversation_boost);
    assert_eq!(ambient_noise_reduction, 1.0);
    assert_eq!(own_voice, 0.75);
}

#[test]
fn command_tag_need_not_come_first() {
    assert!(matches!(
        cmd(r#"{"mode":2,"cmd":"set_anc"}"#),
        Command::SetAnc { mode: 2 }
    ));
}

#[test]
fn from_line_trims_line_endings() {
    assert!(matches!(
        cmd("  {\"cmd\":\"get_state\"}\r\n"),
        Command::GetState
    ));
}

#[test]
fn bad_command_lines_are_rejected() {
    for line in [
        "",
        "not json",
        r#"{"cmd":"launch_missiles"}"#,
        r#"{"cmd":"SetAnc","mode":2}"#,
        r#"{"mode":2}"#,
        r#"{"cmd":"set_anc"}"#,
        r#"{"cmd":"set_anc","mode":256}"#,
        r#"{"cmd":"hello","kind":"phone"}"#,
        "{\"cmd\":\"get_state\"}\n{\"cmd\":\"shutdown\"}",
    ] {
        assert!(from_line::<Command>(line).is_none(), "accepted: {line:?}");
    }
}

#[test]
fn snapshot_without_defaulted_fields_parses() {
    let old = concat!(
        r#"{"connected":true,"dev_name":"AirPods","#,
        r#""battery":{"left":50,"right":null,"case":null,"headphone":null},"#,
        r#""anc":3,"mic_recording":false,"auto_mode":true,"#,
        r#""conversational_awareness":false,"adaptive_volume":true,"allow_off":false,"#,
        r#""volume":20,"muted":true,"heart_rate":null}"#
    );
    let expected = Snapshot {
        connected: true,
        dev_name: "AirPods".into(),
        battery: Battery {
            left: Some(50),
            ..Battery::default()
        },
        anc: 3,
        auto_mode: true,
        adaptive_volume: true,
        volume: 20,
        muted: true,
        ..Snapshot::default()
    };
    assert_eq!(from_line::<Snapshot>(old), Some(expected));
}

#[test]
fn snapshot_without_driverless_parses_as_driver_mode() {
    let old = DEFAULT_SNAPSHOT.replace(r#","driverless":false"#, "");
    assert!(!old.contains("driverless"));
    let snap = from_line::<Snapshot>(&old).expect("old snapshot rejected");
    assert!(!snap.driverless);
}

#[test]
fn battery_without_charging_flags_parses() {
    assert_eq!(
        from_line::<Battery>(r#"{"left":1,"right":2,"case":3,"headphone":4}"#),
        Some(Battery {
            left: Some(1),
            right: Some(2),
            case: Some(3),
            headphone: Some(4),
            ..Battery::default()
        })
    );
}

#[test]
fn battery_without_level_fields_parses_as_absent() {
    assert_eq!(from_line::<Battery>("{}"), Some(Battery::default()));
}

#[test]
fn snapshot_missing_required_field_is_rejected() {
    let body = DEFAULT_SNAPSHOT.replace(r#""muted":false,"#, "");
    assert!(from_line::<Snapshot>(&body).is_none());
}
