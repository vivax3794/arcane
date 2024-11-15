//! Splash screen, intro!

use ratatui::layout::{Constraint, Layout};
use ratatui::style::Color;
use ratatui::text::{Line, Text};
use ratatui::widgets::Paragraph;

use super::windows::WindowID;
use crate::prelude::*;

/// The plugin
pub(super) struct SplashScreenPlugin;

impl Plugin for SplashScreenPlugin {
    #[errors]
    fn on_load(&mut self, events: &mut EventManager) -> Result<()> {
        events.dispatch(WindowEvent::CreateWindow(Box::new(SplashScreenWindow)));
        Ok(())
    }
}

/// The window showing the splash screen
#[derive(Clone, Copy)]
struct SplashScreenWindow;

impl Window for SplashScreenWindow {
    fn name(&self) -> String {
        String::from("Splash Screen")
    }

    #[errors]
    fn update(
        &mut self,
        events: &mut EventManager,
        _plugins: &PluginStore,
        _focused: bool,
        id: WindowID,
    ) -> Result<()> {
        let (reader, mut writer) = events.split();
        for event in reader.read::<WindowEvent>() {
            if let WindowEvent::CreateWindow(_) = event {
                writer.dispatch(WindowEvent::CloseWindow(id));
            }
        }

        Ok(())
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
        _plugins: &crate::plugin_manager::PluginStore,
    ) {
        /// ASCII art of the logo
        const LOGO: &str = "
 █████╗ ██████╗  ██████╗ █████╗ ███╗   ██╗███████╗
██╔══██╗██╔══██╗██╔════╝██╔══██╗████╗  ██║██╔════╝
███████║██████╔╝██║     ███████║██╔██╗ ██║█████╗  
██╔══██║██╔══██╗██║     ██╔══██║██║╚██╗██║██╔══╝  
██║  ██║██║  ██║╚██████╗██║  ██║██║ ╚████║███████╗
╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝╚═╝  ╚═══╝╚══════╝
";

        let layout = Layout::vertical([
            Constraint::Percentage(20),
            Constraint::Length(
                TryInto::<u16>::try_into(LOGO.lines().count())
                    .unwrap_or_default()
                    .saturating_add(2),
            ),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .areas::<4>(area);
        let logo = Paragraph::new(LOGO).magenta().centered();
        let gray = Color::Rgb(80, 80, 100);
        let splash = Paragraph::new(Text::from(vec![
            Line::from(vec![
                "The ".white(),
                "🔥blazingly🔥 fast".red().bold(),
                " terminal editor by ".white(),
                "Viv\n".magenta().bold(),
            ]),
            Line::from(vec![
                "(We assume because we use ".fg(gray),
                "Rust".red().bold(),
                " that it is fast)".fg(gray),
            ]),
            Line::from(vec!["((We have in fact done zero benchmarks))".fg(gray)]),
        ]))
        .centered();
        frame.render_widget(logo, layout[1]);
        frame.render_widget(splash, layout[2]);
    }
}
