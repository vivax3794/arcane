//! Plugin to show application FPS

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::text::ToLine;
use ratatui::widgets::{Block, BorderType, Clear, Sparkline};

use super::keybindings::{KeyBind, KeybindPlugin, SetKeybind};
use crate::editor::{DeltaTimeEvent, KeydownEvent};
use crate::plugin_manager::{EventManager, Plugin, PluginStore};
use crate::prelude::*;

/// Record fps
#[derive(Debug)]
pub struct FpsPlugin {
    /// The last recorded fps
    fps: u64,
    /// The fps history sampled at 0.2s
    fps_history: Vec<u64>,
    /// The last SPF
    last_delta: f64,
    /// How long since the last fps sample
    last_recording: f64,
    /// Should the fps popup be shown?
    show_fps: bool,
}

impl FpsPlugin {
    /// Create new one
    pub(super) const fn new() -> Self {
        Self {
            fps: 0,
            fps_history: Vec::new(),
            last_delta: 0.0,
            last_recording: 0.0,
            show_fps: false,
        }
    }
}

impl Plugin for FpsPlugin {
    fn update(&mut self, events: &EventManager, plugins: &PluginStore) -> Result<()> {
        let Some(keybinds) = plugins.get::<KeybindPlugin>() else {
            return Ok(());
        };

        if !self.show_fps {
            return Ok(());
        }

        for event in events.read::<DeltaTimeEvent>() {
            self.fps = (1_000_000_000_u128)
                .checked_div(event.0.as_nanos())
                .unwrap_or_default()
                .try_into()
                .unwrap_or(self.fps);
            self.last_delta = event.0.as_secs_f64();
            self.last_recording += event.0.as_secs_f64();
        }

        if self.last_recording > 0.2 {
            self.last_recording = 0.0;
            self.fps_history.push(self.fps);
            if self.fps_history.len() > 18 {
                self.fps_history.remove(0);
            }
        }

        Ok(())
    }

    fn z_index(&self) -> u32 {
        u32::MAX
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
        _plugins: &PluginStore,
    ) {
        if !self.show_fps {
            return;
        }

        let area =
            Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]).areas::<2>(area)[0];
        let area =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(20)]).areas::<2>(area)[1];

        let spark_line = Sparkline::default()
            .data(self.fps_history.as_slice())
            .yellow();
        let title = format!("{}", self.fps);
        let bottom = format!("{}", self.last_delta);
        let border = Block::bordered()
            .title_top(title.to_line().right_aligned().red())
            .title_top("FPS")
            .title_bottom(bottom.to_line().right_aligned().blue())
            .title_bottom("SPF")
            .border_type(BorderType::Rounded);
        let spark_line = spark_line.block(border);

        frame.render_widget(Clear, area);
        frame.render_widget(spark_line, area);
    }
}
