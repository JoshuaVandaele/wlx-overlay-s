use std::{array, fs::File, io::Write, time::Duration};

use anyhow::bail;
use ovr_overlay::{
    input::{ActionHandle, ActionSetHandle, ActiveActionSet, InputManager, InputValueHandle},
    sys::{
        ETrackedControllerRole, ETrackedDeviceClass, ETrackedDeviceProperty,
        ETrackingUniverseOrigin,
    },
    system::SystemManager,
    TrackedDeviceIndex,
};

use crate::{
    backend::input::{Haptics, TrackedDevice, TrackedDeviceRole},
    config_io::CONFIG_ROOT_PATH,
    state::AppState,
};

use super::helpers::{Affine3AConvert, OVRError};

const SET_DEFAULT: &str = "/actions/default";
const INPUT_SOURCES: [&str; 2] = ["/user/hand/left", "/user/hand/right"];
const PATH_POSES: [&str; 2] = [
    "/actions/default/in/LeftHand",
    "/actions/default/in/RightHand",
];
const PATH_HAPTICS: [&str; 2] = [
    "/actions/default/out/HapticsLeft",
    "/actions/default/out/HapticsRight",
];

const PATH_CLICK: &str = "/actions/default/in/Click";
const PATH_GRAB: &str = "/actions/default/in/Grab";
const PATH_SCROLL: &str = "/actions/default/in/Scroll";
const PATH_ALT_CLICK: &str = "/actions/default/in/AltClick";
const PATH_SHOW_HIDE: &str = "/actions/default/in/ShowHide";
const PATH_SPACE_DRAG: &str = "/actions/default/in/SpaceDrag";
const PATH_CLICK_MODIFIER_RIGHT: &str = "/actions/default/in/ClickModifierRight";
const PATH_CLICK_MODIFIER_MIDDLE: &str = "/actions/default/in/ClickModifierMiddle";

const INPUT_ANY: InputValueHandle = InputValueHandle(ovr_overlay::sys::k_ulInvalidInputValueHandle);

pub(super) struct OpenVrInputSource {
    hands: [OpenVrHandSource; 2],
    set_hnd: ActionSetHandle,
    click_hnd: ActionHandle,
    grab_hnd: ActionHandle,
    scroll_hnd: ActionHandle,
    alt_click_hnd: ActionHandle,
    show_hide_hnd: ActionHandle,
    space_drag_hnd: ActionHandle,
    click_modifier_right_hnd: ActionHandle,
    click_modifier_middle_hnd: ActionHandle,
}

pub(super) struct OpenVrHandSource {
    has_pose: bool,
    input_hnd: InputValueHandle,
    pose_hnd: ActionHandle,
    haptics_hnd: ActionHandle,
}

impl OpenVrInputSource {
    pub fn new(input: &mut InputManager) -> Result<Self, OVRError> {
        let set_hnd = input.get_action_set_handle(SET_DEFAULT)?;

        let click_hnd = input.get_action_handle(PATH_CLICK)?;
        let grab_hnd = input.get_action_handle(PATH_GRAB)?;
        let scroll_hnd = input.get_action_handle(PATH_SCROLL)?;
        let alt_click_hnd = input.get_action_handle(PATH_ALT_CLICK)?;
        let show_hide_hnd = input.get_action_handle(PATH_SHOW_HIDE)?;
        let space_drag_hnd = input.get_action_handle(PATH_SPACE_DRAG)?;
        let click_modifier_right_hnd = input.get_action_handle(PATH_CLICK_MODIFIER_RIGHT)?;
        let click_modifier_middle_hnd = input.get_action_handle(PATH_CLICK_MODIFIER_MIDDLE)?;

        let input_hnd: Vec<InputValueHandle> = INPUT_SOURCES
            .iter()
            .map(|path| Ok((input.get_input_source_handle(path))?))
            .collect::<Result<_, OVRError>>()?;

        let pose_hnd: Vec<ActionHandle> = PATH_POSES
            .iter()
            .map(|path| Ok((input.get_action_handle(path))?))
            .collect::<Result<_, OVRError>>()?;

        let haptics_hnd: Vec<ActionHandle> = PATH_HAPTICS
            .iter()
            .map(|path| Ok((input.get_action_handle(path))?))
            .collect::<Result<_, OVRError>>()?;

        let hands: [OpenVrHandSource; 2] = array::from_fn(|i| OpenVrHandSource {
            has_pose: false,
            input_hnd: input_hnd[i],
            pose_hnd: pose_hnd[i],
            haptics_hnd: haptics_hnd[i],
        });

        Ok(OpenVrInputSource {
            set_hnd,
            click_hnd,
            grab_hnd,
            scroll_hnd,
            alt_click_hnd,
            show_hide_hnd,
            space_drag_hnd,
            click_modifier_right_hnd,
            click_modifier_middle_hnd,
            hands,
        })
    }

    pub fn haptics(&mut self, input: &mut InputManager, hand: usize, haptics: &Haptics) {
        let hnd = self.hands[hand].haptics_hnd;
        let _ = input.trigger_haptic_vibration_action(
            hnd,
            0.0,
            Duration::from_secs_f32(haptics.duration),
            haptics.frequency,
            haptics.intensity,
            INPUT_ANY,
        );
    }

    pub fn update(
        &mut self,
        input: &mut InputManager,
        system: &mut SystemManager,
        app: &mut AppState,
    ) {
        let aas = ActiveActionSet {
            0: ovr_overlay::sys::VRActiveActionSet_t {
                ulActionSet: self.set_hnd.0,
                ulRestrictedToDevice: 0,
                ulSecondaryActionSet: 0,
                unPadding: 0,
                nPriority: 0,
            },
        };

        let _ = input.update_actions(&mut [aas]);

        let universe = ETrackingUniverseOrigin::TrackingUniverseStanding;

        for i in 0..2 {
            let hand = &mut self.hands[i];
            let app_hand = &mut app.input_state.pointers[i];

            hand.has_pose = false;

            let _ = input
                .get_pose_action_data_relative_to_now(
                    hand.pose_hnd,
                    universe.clone(),
                    0.005,
                    INPUT_ANY,
                )
                .and_then(|pose| {
                    app_hand.pose = pose.0.pose.mDeviceToAbsoluteTracking.to_affine();
                    hand.has_pose = true;
                    Ok(())
                });

            app_hand.now.click = input
                .get_digital_action_data(self.click_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.grab = input
                .get_digital_action_data(self.grab_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.alt_click = input
                .get_digital_action_data(self.alt_click_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.show_hide = input
                .get_digital_action_data(self.show_hide_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.space_drag = input
                .get_digital_action_data(self.space_drag_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.click_modifier_right = input
                .get_digital_action_data(self.click_modifier_right_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.click_modifier_middle = input
                .get_digital_action_data(self.click_modifier_middle_hnd, hand.input_hnd)
                .map(|x| x.0.bState)
                .unwrap_or(false);

            app_hand.now.scroll = input
                .get_analog_action_data(self.scroll_hnd, hand.input_hnd)
                .map(|x| x.0.y)
                .unwrap_or(0.0);
        }

        let devices = system.get_device_to_absolute_tracking_pose(universe, 0.005);
        app.input_state.hmd = devices[0].mDeviceToAbsoluteTracking.to_affine();
    }

    pub fn update_devices(&mut self, system: &mut SystemManager, app: &mut AppState) {
        app.input_state.devices.clear();

        for idx in 0..TrackedDeviceIndex::MAX {
            let device = TrackedDeviceIndex::new(idx as _).unwrap(); // safe
            if !system.is_tracked_device_connected(device) {
                continue;
            }

            let class = system.get_tracked_device_class(device);

            let role = match class {
                ETrackedDeviceClass::TrackedDeviceClass_HMD => TrackedDeviceRole::Hmd,
                ETrackedDeviceClass::TrackedDeviceClass_Controller => {
                    let role = system.get_controller_role_for_tracked_device_index(device);
                    match role {
                        ETrackedControllerRole::TrackedControllerRole_LeftHand => {
                            TrackedDeviceRole::LeftHand
                        }
                        ETrackedControllerRole::TrackedControllerRole_RightHand => {
                            TrackedDeviceRole::RightHand
                        }
                        _ => continue,
                    }
                }
                ETrackedDeviceClass::TrackedDeviceClass_GenericTracker => {
                    TrackedDeviceRole::Tracker
                }
                _ => continue,
            };

            get_tracked_device(system, device, role).and_then(|device| {
                app.input_state.devices.push(device);
                Some(())
            });
        }

        app.input_state.devices.sort_by(|a, b| {
            (a.soc.is_none() as u8)
                .cmp(&(b.soc.is_none() as u8))
                .then((a.role as u8).cmp(&(b.role as u8)))
                .then(a.soc.unwrap_or(999.).total_cmp(&b.soc.unwrap_or(999.)))
        });
    }
}

fn get_tracked_device(
    system: &mut SystemManager,
    index: TrackedDeviceIndex,
    role: TrackedDeviceRole,
) -> Option<TrackedDevice> {
    let soc = system
        .get_tracked_device_property(
            index,
            ETrackedDeviceProperty::Prop_DeviceBatteryPercentage_Float,
        )
        .ok();

    let charging = if soc.is_some() {
        system
            .get_tracked_device_property(index, ETrackedDeviceProperty::Prop_DeviceIsCharging_Bool)
            .unwrap_or(false)
    } else {
        false
    };

    // TODO: cache this
    let is_alvr = system
        .get_tracked_device_property(
            index,
            ETrackedDeviceProperty::Prop_TrackingSystemName_String,
        )
        .map(|x: String| x.contains("ALVR"))
        .unwrap_or(false);

    if is_alvr {
        // don't show ALVR's fake trackers on battery panel
        return None;
    }

    Some(TrackedDevice {
        valid: true,
        index,
        soc,
        charging,
        role,
    })
}

pub fn set_action_manifest(input: &mut InputManager) -> anyhow::Result<()> {
    let action_path = CONFIG_ROOT_PATH.join("actions.json");

    if !action_path.is_file() {
        File::create(&action_path)?.write_all(include_bytes!("../../res/actions.json"))?;
    }

    let binding_path = CONFIG_ROOT_PATH.join("actions_binding_knuckles.json");
    if !binding_path.is_file() {
        File::create(&binding_path)?
            .write_all(include_bytes!("../../res/actions_binding_knuckles.json"))?;
    }

    let binding_path = CONFIG_ROOT_PATH.join("actions_binding_vive.json");
    if !binding_path.is_file() {
        File::create(&binding_path)?
            .write_all(include_bytes!("../../res/actions_binding_vive.json"))?;
    }

    let binding_path = CONFIG_ROOT_PATH.join("actions_binding_oculus.json");
    if !binding_path.is_file() {
        File::create(&binding_path)?
            .write_all(include_bytes!("../../res/actions_binding_oculus.json"))?;
    }

    if let Err(e) = input.set_action_manifest(action_path.as_path()) {
        bail!("Failed to set action manifest: {}", e);
    }
    Ok(())
}
