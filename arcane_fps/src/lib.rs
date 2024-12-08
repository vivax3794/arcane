//! Plugin to show application FPS

use arcane_core::{DeltaTimeEvent, EventManager, Plugin, PluginStore, Result};
use arcane_keybindings::{BindResult, RegisterKeybind};
use error_mancer::errors;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Stylize;
use ratatui::text::ToLine;
use ratatui::widgets::{Block, BorderType, Clear, Sparkline};
use serde::{Deserialize, Serialize};

/// Toggle fps graph
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct ToggleFps;

#[typetag::serde]
impl BindResult for ToggleFps {}

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

arcane_core::register_plugin!(FpsPlugin);

impl Plugin for FpsPlugin {
    /// Create new one
    fn new() -> Self {
        Self {
            fps: 0,
            fps_history: Vec::new(),
            last_delta: 0.0,
            last_recording: 0.0,
            show_fps: false,
        }
    }

    #[errors]
    fn on_load(&mut self, events: &mut EventManager) -> Result<()> {
        events.ensure_event::<ToggleFps>();
        events.dispatch(RegisterKeybind::chord([], ToggleFps));

        Ok(())
    }

    #[errors]
    fn update(&mut self, events: &mut EventManager, _plugins: &PluginStore) -> Result<()> {
        for _ in events.read::<ToggleFps>() {
            self.show_fps = !self.show_fps;
        }

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
            Layout::vertical([Constraint::Fill(1), Constraint::Length(5)]).areas::<2>(area)[1];
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
