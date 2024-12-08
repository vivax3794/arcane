//! Splash screen, intro!

use arcane_core::Result;
use arcane_windows::{Window, WindowEvent, WindowID};
use error_mancer::errors;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Text};
use ratatui::widgets::Paragraph;

/// The plugin
pub struct SplashScreenPlugin;

arcane_core::register_plugin!(SplashScreenPlugin);

impl arcane_core::Plugin for SplashScreenPlugin {
    fn new() -> Self {
        Self
    }
    #[errors]
    fn on_load(&mut self, events: &mut arcane_core::EventManager) -> Result<()> {
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
        events: &mut arcane_core::EventManager,
        _plugins: &arcane_core::PluginStore,
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
        _plugins: &arcane_core::PluginStore,
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
