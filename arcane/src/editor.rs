//! The editor root

use std::any::Any;
use std::time::{Duration, Instant};

use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::Frame;

use crate::plugin_loading::settings::{
    get_settings,
    PluginSettings,
    RegisterSettings,
    SettingsPlugin,
    SettingsValue,
    SettingsValueCommon,
};
use crate::plugin_manager::StateManager;
use crate::prelude::*;

/// Dispatched every frame hodling the delta from the last frame.
#[derive(Clone, Copy, Debug)]
pub struct DeltaTimeEvent(pub Duration);

/// A key was pressed
#[derive(Clone, Copy, Debug)]
pub struct KeydownEvent(pub KeyEvent);

/// Core Editor settings
///
/// NOTE: These should be EXTREMELY minimal (in fact I kinda hate I need it)
#[derive(Clone, Copy)]
struct EditorSettings {
    /// How long should the program wait on events before rendering the next frame
    event_polling_rate: i128,
}

impl PluginSettings for EditorSettings {
    fn name(&self) -> &'static str {
        "Core"
    }
    fn values(&mut self) -> Box<[SettingsValueCommon]> {
        Box::new([SettingsValueCommon {
            name: "max_spf_milis",
            value: SettingsValue::Integer {
                value: &mut self.event_polling_rate,
                min: 0,
                max: 10,
            },
        }])
    }
}

/// The core editor
pub(crate) struct Editor {
    /// The editors plugin
    plugins: StateManager,
    /// At what point did the last frame happen
    last_frame: Instant,
}

impl Editor {
    /// Create a new editor instance
    pub(crate) fn new() -> Self {
        Self {
            plugins: StateManager::new(),
            last_frame: std::time::Instant::now(),
        }
    }

    /// Does inital setup
    pub(crate) fn on_load(&mut self) -> Result<()> {
        crate::plugin_loading::load_plugins(&mut self.plugins.plugins);
        self.plugins.on_load()?;
        // self.plugins
        //     .events
        //     .dispatch(RegisterSettings(Box::new(EditorSettings {
        //         event_polling_rate: 1,
        //     })));
        Ok(())
    }

    /// Get the even polling rate
    pub(crate) fn event_poll_rate(&self) -> Duration {
        let Some(settings) = get_settings::<EditorSettings>(&self.plugins.plugins) else {
            return Duration::from_millis(1);
        };

        Duration::from_millis(settings.event_polling_rate.try_into().unwrap_or(1))
    }

    /// Draw the editor ui
    pub(crate) fn draw(&self, frame: &mut Frame, area: Rect) {
        self.plugins.draw(frame, area);
    }

    /// Run event handlers and update other state
    pub(crate) fn update(&mut self) -> Result<()> {
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.last_frame);
        self.plugins.events.dispatch(DeltaTimeEvent(delta));
        self.last_frame = now;

        self.plugins.events.swap_buffers();
        self.plugins.update()?;
        Ok(())
    }

    /// Handle editor key inputs
    #[instrument(skip(self))]
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        self.plugins.events.dispatch(KeydownEvent(key));
        Ok(())
    }
}
