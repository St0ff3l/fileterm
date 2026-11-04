use super::SerialControlState;

#[test]
fn marks_rts_as_driver_managed_for_hardware_flow_and_rs485() {
    let hardware = SerialControlState::from_profile(&serde_json::json!({
        "flowControl": "hardware"
    }));
    assert!(hardware.hardware_flow_control);
    assert!(hardware.rts_managed);

    let rs485 = SerialControlState::from_profile(&serde_json::json!({
        "rs485Mode": "half-duplex"
    }));
    assert!(!rs485.hardware_flow_control);
    assert!(rs485.rts_managed);

    let manual = SerialControlState::from_profile(&serde_json::json!({}));
    assert!(!manual.rts_managed);
}

#[test]
fn reset_state_keeps_the_open_levels_after_manual_changes() {
    let mut state = SerialControlState::from_profile(&serde_json::json!({
        "dtrOnOpen": false,
        "rtsOnOpen": true
    }));
    state.dtr = Some(true);
    state.rts = Some(false);

    state.dtr = state.initial_dtr;
    state.rts = state.initial_rts;

    assert_eq!(state.dtr, Some(false));
    assert_eq!(state.rts, Some(true));
}
