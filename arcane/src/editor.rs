//! The editor root

use std::time::Instant;

use arcane_core::Result;
use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::Frame;

/// The core editor
pub(crate) struct Editor {
    /// The editors plugin
    state: arcane_core::StateManager,
    /// At what point did the last frame happen
    last_frame: Instant,
}

impl Editor {
    /// Create a new editor instance
    pub(crate) fn new() -> Self {
        Self {
            state: arcane_core::StateManager::new(),
            last_frame: std::time::Instant::now(),
        }
    }

    /// Does inital setup
    pub(crate) fn on_load(&mut self) -> Result<()> {
        self.state.on_load()?;
        Ok(())
    }

    /// Draw the editor ui
    pub(crate) fn draw(&self, frame: &mut Frame, area: Rect) {
        self.state.draw(frame, area);
    }

    /// Run event handlers and update other state
    pub(crate) fn update(&mut self) -> Result<()> {
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.last_frame);
        self.state
            .events
            .dispatch(arcane_core::DeltaTimeEvent(delta));
        self.last_frame = now;

        self.state.events.swap_buffers();
        self.state.update()?;
        Ok(())
    }

    /// Handle editor key inputs
    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        self.state.events.dispatch(arcane_core::KeydownEvent(key));
    }
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn on_load() {
        let mut editor = Editor::new();
        editor.on_load().unwrap();
    }

    #[test]
    fn test_update_delta() {
        const DURATION: f32 = 0.5;

        let mut editor = Editor::new();
        editor.update().unwrap();
        thread::sleep(Duration::from_secs_f32(DURATION));
        editor.update().unwrap();

        for event in editor.state.events.read::<arcane_core::DeltaTimeEvent>() {
            let delta = event.0.as_secs_f32();
            assert!(
                (delta - DURATION).abs() < 0.05,
                "Delta time too far off 1 seconds {delta}"
            );
        }
    }
}
