pub mod schedules;
pub use schedules::*;

use bevy::{
    prelude::*,
    render::{
        camera::{ManualTextureView, ManualTextureViews},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        renderer::{RenderAdapter, RenderDevice, RenderInstance},
    },
    window::{PrimaryWindow, RawHandleWrapper},
};

use crate::{
    graphics,
    resources::{
        OXrSessionSetupInfo, XrFormat, XrInstance, XrResolution, XrSession, XrSessionRunning,
        XrSwapchain,
    },
    LEFT_XR_TEXTURE_HANDLE, RIGHT_XR_TEXTURE_HANDLE,
};

#[derive(Resource, Event, Clone, Copy, PartialEq, Eq, Reflect, Debug, ExtractResource)]
pub enum XrStatus {
    NoInstance,
    Enabled,
    Enabling,
    Disabled,
    Disabling,
}

#[derive(
    Resource, Clone, Copy, PartialEq, Eq, Reflect, Debug, ExtractResource, Default, Deref, DerefMut,
)]
pub struct XrShouldRender(pub bool);

pub struct XrEarlyInitPlugin;

pub struct XrInitPlugin;

pub fn xr_only() -> impl FnMut(Res<XrStatus>) -> bool {
    resource_equals(XrStatus::Enabled)
}
pub fn xr_render_only() -> impl FnMut(Res<XrShouldRender>) -> bool {
    resource_equals(XrShouldRender(true))
}

impl Plugin for XrEarlyInitPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<SetupXrData>()
            .add_event::<CleanupXrData>()
            .add_event::<StartXrSession>()
            .add_event::<EndXrSession>();
    }
}

impl Plugin for XrInitPlugin {
    fn build(&self, app: &mut App) {
        add_schedules(app);
        app.add_plugins(ExtractResourcePlugin::<XrStatus>::default());
        app.add_plugins(ExtractResourcePlugin::<XrShouldRender>::default());
        app.init_resource::<XrShouldRender>();
        app.add_systems(PreUpdate, setup_xr.run_if(on_event::<SetupXrData>()))
            .add_systems(PreUpdate, cleanup_xr.run_if(on_event::<CleanupXrData>()));
        app.add_systems(
            PostUpdate,
            start_xr_session.run_if(on_event::<StartXrSession>()),
        );
        app.add_systems(
            PostUpdate,
            stop_xr_session.run_if(on_event::<EndXrSession>()),
        );
        app.add_systems(XrSetup, setup_manual_texture_views);
    }
}

fn setup_manual_texture_views(
    mut manual_texture_views: ResMut<ManualTextureViews>,
    swapchain: Res<XrSwapchain>,
    xr_resolution: Res<XrResolution>,
    xr_format: Res<XrFormat>,
) {
    info!("Creating Texture views");
    let (left, right) = swapchain.get_render_views();
    let left = ManualTextureView {
        texture_view: left.into(),
        size: **xr_resolution,
        format: **xr_format,
    };
    let right = ManualTextureView {
        texture_view: right.into(),
        size: **xr_resolution,
        format: **xr_format,
    };
    manual_texture_views.insert(LEFT_XR_TEXTURE_HANDLE, left);
    manual_texture_views.insert(RIGHT_XR_TEXTURE_HANDLE, right);
}

pub fn setup_xr(world: &mut World) {
    world.run_schedule(XrPreSetup);
    world.run_schedule(XrSetup);
    world.run_schedule(XrPrePostSetup);
    world.run_schedule(XrPostSetup);
    *world.resource_mut::<XrStatus>() = XrStatus::Enabled;
}
fn cleanup_xr(world: &mut World) {
    world.run_schedule(XrPreCleanup);
    world.run_schedule(XrCleanup);
    world.run_schedule(XrPostCleanup);
    *world.resource_mut::<XrStatus>() = XrStatus::Disabled;
}

#[derive(Event, Clone, Copy, Default)]
pub struct StartXrSession;

#[derive(Event, Clone, Copy, Default)]
pub struct EndXrSession;

#[derive(Event, Clone, Copy, Default)]
struct SetupXrData;
#[derive(Event, Clone, Copy, Default)]
pub(crate) struct CleanupXrData;

#[allow(clippy::too_many_arguments)]
fn start_xr_session(
    mut commands: Commands,
    mut setup_xr: EventWriter<SetupXrData>,
    mut status: ResMut<XrStatus>,
    instance: Res<XrInstance>,
    primary_window: Query<&RawHandleWrapper, With<PrimaryWindow>>,
    setup_info: NonSend<OXrSessionSetupInfo>,
    render_device: Res<RenderDevice>,
    render_adapter: Res<RenderAdapter>,
    render_instance: Res<RenderInstance>,
) {
    info!("start Session");
    match *status {
        XrStatus::Disabled => {}
        XrStatus::NoInstance => {
            warn!("Trying to start OpenXR Session without instance, ignoring");
            return;
        }
        XrStatus::Enabled | XrStatus::Enabling => {
            warn!("Trying to start OpenXR Session while one already exists, ignoring");
            return;
        }
        XrStatus::Disabling => {
            warn!("Trying to start OpenXR Session while one is stopping, ignoring");
            return;
        }
    }
    let (
        xr_session,
        xr_resolution,
        xr_format,
        xr_session_running,
        xr_frame_waiter,
        xr_swapchain,
        xr_input,
        xr_views,
        xr_frame_state,
    ) = match graphics::start_xr_session(
        primary_window.get_single().cloned().ok(),
        &setup_info,
        &instance,
        &render_device,
        &render_adapter,
        &render_instance,
    ) {
        Ok(data) => data,
        Err(err) => {
            error!("Unable to start OpenXR Session: {}", err);
            return;
        }
    };
    commands.insert_resource(xr_session);
    commands.insert_resource(xr_resolution);
    commands.insert_resource(xr_format);
    commands.insert_resource(xr_session_running);
    commands.insert_resource(xr_frame_waiter);
    commands.insert_resource(xr_swapchain);
    commands.insert_resource(xr_input);
    commands.insert_resource(xr_views);
    commands.insert_resource(xr_frame_state);
    *status = XrStatus::Enabling;
    setup_xr.send_default();
}

fn stop_xr_session(session: ResMut<XrSession>, mut status: ResMut<XrStatus>) {
    match session.request_exit() {
        Ok(_) => {}
        Err(err) => {
            error!("Error while trying to request session exit: {}", err)
        }
    }
    *status = XrStatus::Enabling;
}
