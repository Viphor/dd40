//! Graceful shutdown for headless binaries.
//!
//! Bevy's headless `MinimalPlugins` has no window, so nothing translates
//! an OS signal (Ctrl-C, SIGTERM) into a Bevy [`AppExit`] message.  The
//! process simply dies — which means anything that runs in the `Last`
//! schedule on `AppExit` (e.g. the entity-sidecar flush from
//! `dd40_chunk_storage`) never gets a chance to run.
//!
//! [`GracefulShutdownPlugin`] installs an OS-level Ctrl-C / SIGTERM
//! handler that flips a shared atomic flag, and adds a one-shot system
//! that, on the next frame after the flag is set, writes
//! [`AppExit::Success`] into the Bevy message queue.  Subsequent frames
//! see `AppExit` and shut the app down cleanly through Bevy's normal
//! exit path, giving every `Last`-scheduled saver a chance to run.
//!
//! # When to add
//!
//! Add this plugin to any headless binary (servers, CLI tools).
//! Windowed binaries already receive `AppExit` from the windowing
//! layer and do not need it.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::app::{App, AppExit, Plugin, PreUpdate};
use bevy::ecs::message::MessageWriter;
use bevy::log::{error, info, warn};

/// Set by the Ctrl-C / SIGTERM handler installed by
/// [`GracefulShutdownPlugin`].  A single global flag is correct here:
/// the OS only delivers signals to the process, and we only need one
/// `AppExit` even if the user mashes Ctrl-C several times.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Tracks whether the global signal handler has been installed.  Using
/// [`OnceLock`] makes installation idempotent — adding the plugin
/// twice (or in tests that build many apps in one process) is a
/// silent no-op on the second call.
static HANDLER_INSTALLED: OnceLock<()> = OnceLock::new();

/// Plugin that maps OS shutdown signals to Bevy [`AppExit`].
///
/// Installs a process-wide Ctrl-C handler the first time it is
/// added; subsequent additions only register the polling system.
#[derive(Default)]
pub struct GracefulShutdownPlugin;

impl Plugin for GracefulShutdownPlugin {
    fn build(&self, app: &mut App) {
        HANDLER_INSTALLED.get_or_init(|| {
            if let Err(e) = ctrlc::set_handler(|| {
                // Idempotent: every signal flips the same flag.
                SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
            }) {
                error!("GracefulShutdownPlugin: failed to install Ctrl-C handler: {e}");
            } else {
                info!("GracefulShutdownPlugin: Ctrl-C / SIGTERM will trigger AppExit");
            }
        });

        app.add_systems(PreUpdate, emit_app_exit_on_shutdown_request);
    }
}

/// Polls [`SHUTDOWN_REQUESTED`] and writes a single
/// [`AppExit::Success`] when it transitions to `true`.
///
/// Bevy's main loop will then run one more frame (including the `Last`
/// schedule) before exiting, which is exactly the window
/// `save_entities_on_exit` needs.
fn emit_app_exit_on_shutdown_request(mut writer: MessageWriter<AppExit>) {
    // `swap` ensures we only emit AppExit on the first observation of
    // the flag — subsequent frames see `false` and do nothing, which
    // matters because the main loop may run several more frames before
    // every system observes the exit.
    if SHUTDOWN_REQUESTED.swap(false, Ordering::SeqCst) {
        warn!("Shutdown signal received — emitting AppExit::Success");
        writer.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_message::<AppExit>();
        app.add_systems(PreUpdate, emit_app_exit_on_shutdown_request);
        app
    }

    // Combined into a single test because [`SHUTDOWN_REQUESTED`] is a
    // process-wide static; running multiple `#[test]` fns in parallel
    // would race on it.
    #[test]
    fn signal_flag_emits_exactly_one_appexit() {
        #[derive(Resource, Default)]
        struct ExitCount(usize);

        // Phase 1: no signal → no AppExit even across many frames.
        SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
        let mut app = make_app();
        app.init_resource::<ExitCount>();
        app.add_systems(
            Update,
            |mut r: MessageReader<AppExit>, mut c: ResMut<ExitCount>| {
                c.0 += r.read().count();
            },
        );
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<ExitCount>().0,
            0,
            "no signal, no exit"
        );

        // Phase 2: flip the flag → exactly one AppExit across the rest of the run.
        SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<ExitCount>().0,
            1,
            "flag must emit exactly one AppExit total"
        );
    }
}
