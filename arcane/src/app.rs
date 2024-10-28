//! Holds root applications logic

use ansi_to_tui::IntoText;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::symbols::border;
use ratatui::widgets::{Block, Clear, Padding, Paragraph};

use crate::editor::Editor;
use crate::prelude::*;

/// The root app
pub(crate) struct App {
    /// The editor widget
    editor: Editor,
    /// Should the application exit next frame?
    exit_application: bool,
    /// Is there a current error popup?
    error_popup: Option<color_eyre::eyre::Report>,
}

impl App {
    /// Create new default app
    pub(crate) fn new() -> Self {
        Self {
            editor: Editor::new(),
            exit_application: false,
            error_popup: None,
        }
    }

    /// Run the application
    #[inline(always)]
    #[instrument(skip(self, terminal))]
    pub(crate) fn run(
        mut self,
        mut terminal: ratatui::Terminal<impl ratatui::backend::Backend>,
    ) -> Result<()> {
        event!(Level::INFO, "Starting Application");
        self.editor.on_load()?;
        while !self.exit_application {
            self.read_events()?;
            if let Err(err) = self.editor.update() {
                self.handle_error(err);
            }
            terminal.draw(|frame| self.draw(frame))?;
        }
        event!(Level::INFO, "Exiting Application");
        Ok(())
    }

    /// Quit the application on the next frame
    fn quit(&mut self) {
        self.exit_application = true;
    }

    /// Create a error popup for the user to read
    fn handle_error(&mut self, err: color_eyre::eyre::Report) {
        event!(Level::ERROR, "{err}");
        event!(Level::ERROR, "Error occured, creating popup");
        self.error_popup = Some(err);
    }

    /// Handle input for the application
    fn read_events(&mut self) -> Result<()> {
        if crossterm::event::poll(self.editor.event_poll_rate())? {
            if let Event::Key(key) = crossterm::event::read()? {
                if key.kind == KeyEventKind::Press {
                    if let Err(err) = self.handle_key(key) {
                        self.handle_error(err);
                    };
                }
            }
        }
        Ok(())
    }

    /// Handle a single key press
    // #[instrument(skip(self))]
    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        event!(Level::TRACE, "Handling Key {}+{}", key.modifiers, key.code);
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => self.quit(),
            _ => {
                if self.error_popup.is_some() {
                    if key.code == KeyCode::Enter {
                        self.error_popup = None;
                    }
                } else {
                    self.editor.handle_key(key)?;
                }
            }
        }
        Ok(())
    }

    /// Draw the app
    fn draw(&self, frame: &mut ratatui::Frame) {
        self.editor.draw(frame, frame.area());
        if let Some(err) = self.error_popup.as_ref() {
            Self::draw_error(frame, err);
        }
    }

    /// Draw the error window
    #[instrument(skip(frame, err))]
    fn draw_error(frame: &mut ratatui::Frame, err: &color_eyre::Report) {
        let err = format!("WARNING: Application might be in an invalid state\n{err:?}")
            .into_text()
            .unwrap_or_else(|rendering_err| {
                event!(
                    Level::WARN,
                    "Failed to render color_eyre ansi: {rendering_err}"
                );
                format!("Failed to render Error\n{rendering_err}\n{err}").into()
            });

        let err = Paragraph::new(err);
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .red()
            .padding(Padding::uniform(1));
        let err = err.block(block);

        let constraint = Constraint::from_percentages([10, 80, 10]);

        #[allow(clippy::indexing_slicing)]
        let vertical_area = Layout::vertical(&constraint).split(frame.area())[1];
        #[allow(clippy::indexing_slicing)]
        let area = Layout::horizontal(constraint).split(vertical_area)[1];

        frame.render_widget(Clear, area);
        frame.render_widget(err, area);
    }
}

#[cfg(test)]
mod tests {
    use color_eyre::eyre::eyre;
    use ratatui::backend::TestBackend;

    use super::App;

    #[test]
    fn test_error_drawing() {
        let mut terminal = ratatui::Terminal::new(TestBackend::new(80, 80)).unwrap();
        let error = eyre!("Test Test Test\ntest test test\naadjiojad");
        terminal
            .draw(|frame| App::draw_error(frame, &error))
            .unwrap();
    }
}
