use std::io;
use std::path::PathBuf;

use arcane_core::{event, Level};
use arcane_keybindings::MenuEvent;
use devicons::FileIcon;
use error_mancer::errors;
use ignore::gitignore;
use ratatui::crossterm::style;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span, Text};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OpenFileBrowser;

#[typetag::serde]
impl arcane_keybindings::BindResult for OpenFileBrowser {}

struct FileBrowserPlugin;

arcane_core::register_plugin!(FileBrowserPlugin);

impl arcane_core::Plugin for FileBrowserPlugin {
    fn new() -> Self {
        Self
    }

    #[errors]
    fn on_load(&mut self, events: &mut arcane_core::EventManager) -> arcane_core::Result<()> {
        events.ensure_event::<OpenFileBrowser>();
        events.dispatch(arcane_keybindings::RegisterKeybind::single_key(
            arcane_keybindings::KeyBind {
                modifiers: arcane_keybindings::KeyModifiers::CONTROL,
                key: arcane_keybindings::KeyCode::Char('o'),
            },
            OpenFileBrowser,
        ));
        events.dispatch(arcane_settings::RegisterSettings(Box::new(
            FileBrowserSettings::default(),
        )));
        Ok(())
    }

    #[errors(io::Error)]
    fn update(
        &mut self,
        events: &mut arcane_core::EventManager,
        _plugins: &arcane_core::PluginStore,
    ) -> arcane_core::Result<()> {
        let (reader, mut writer) = events.split();
        for _ in reader.read::<OpenFileBrowser>() {
            writer.dispatch(arcane_windows::WindowEvent::CreateWindow(Box::new(
                FileBrowserWindow::new()?,
            )));
        }

        Ok(())
    }
}

const CLOSED_FOLDER_ICON: &str = "󰉋";
const OPEN_FOLDER_ICON: &str = "";
const FOLDER_ICON_COLOR: style::Color = style::Color::Blue;

#[derive(Clone)]
enum FilesystemItem {
    File {
        name: String,
        abs_path: PathBuf,
        icon: FileIcon,
    },
    Folder {
        name: String,
        abs_path: PathBuf,
        children: Vec<FilesystemItem>,
        open: bool,
    },
}

/// # Panics
/// Panics if the hex color is invalid
/// While we try to not have panics in the codebase
/// This function should only be called with values from `devicons::FileIcon::color`
/// which are valid
fn parse_color_hex(hex: &str) -> style::Color {
    let hex = hex.trim().trim_start_matches('#');
    let r = u8::from_str_radix(hex.get(0..2).unwrap(), 16).unwrap();
    let g = u8::from_str_radix(hex.get(2..4).unwrap(), 16).unwrap();
    let b = u8::from_str_radix(hex.get(4..6).unwrap(), 16).unwrap();
    style::Color::Rgb { r, g, b }
}

impl FilesystemItem {
    fn new(path: PathBuf) -> Result<Self, io::Error> {
        event!(Level::TRACE, "Processing path: {}", path.display());
        if path.is_file() {
            let icon = FileIcon::from(&path);
            Ok(Self::File {
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                abs_path: path.canonicalize().unwrap_or_default(),
                icon,
            })
        } else {
            Ok(Self::Folder {
                name: path
                    .canonicalize()
                    .unwrap_or_default()
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                abs_path: path.canonicalize().unwrap_or_default(),
                children: Vec::new(),
                open: false,
            })
        }
    }

    fn toggle_folder(&mut self) -> Result<(), io::Error> {
        let path = self.abs_path().clone();
        if let FilesystemItem::Folder { children, open, .. } = self {
            *open = !*open;
            if *open && children.is_empty() {
                event!(Level::DEBUG, "Loading children of {}", path.display());
                children.extend(
                    path.read_dir()?
                        .map(|dir| FilesystemItem::new(dir?.path()))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                children.sort_by(|a, b| a.name().cmp(b.name()));
            }
        }

        Ok(())
    }

    fn name(&self) -> &str {
        match self {
            FilesystemItem::File { name, .. } => name,
            FilesystemItem::Folder { name, .. } => name,
        }
    }

    fn is_hidden(&self) -> bool {
        self.name().starts_with('.')
    }

    fn should_show(
        &self,
        settings: &FileBrowserSettings,
        gitignore: &gitignore::Gitignore,
    ) -> bool {
        if self.is_hidden() {
            if self.is_folder() {
                settings.show_hidden_folders
            } else {
                settings.show_hidden_files
            }
        } else if settings.show_ignored {
            true
        } else {
            !gitignore
                .matched(self.abs_path(), self.is_folder())
                .is_ignore()
        }
    }

    fn abs_path(&self) -> &PathBuf {
        match self {
            FilesystemItem::File { abs_path, .. } => abs_path,
            FilesystemItem::Folder { abs_path, .. } => abs_path,
        }
    }

    fn is_folder(&self) -> bool {
        match self {
            FilesystemItem::File { .. } => false,
            FilesystemItem::Folder { .. } => true,
        }
    }

    fn icon(&self) -> Span {
        match self {
            FilesystemItem::File { icon, .. } => {
                let color = parse_color_hex(icon.color);
                Span::from(icon.icon.to_string()).fg(color)
            }
            FilesystemItem::Folder { open: false, .. } => {
                Span::from(CLOSED_FOLDER_ICON).fg(FOLDER_ICON_COLOR)
            }
            FilesystemItem::Folder { open: true, .. } => {
                Span::from(OPEN_FOLDER_ICON).fg(FOLDER_ICON_COLOR)
            }
        }
    }

    fn shown_children(
        &self,
        settings: &FileBrowserSettings,
        gitignore: &gitignore::Gitignore,
    ) -> Vec<&FilesystemItem> {
        match self {
            FilesystemItem::File { .. } => vec![],
            FilesystemItem::Folder { children, .. } => children
                .iter()
                .filter(|i| i.should_show(settings, gitignore))
                .collect(),
        }
    }

    fn shown_children_mut(
        &mut self,
        settings: &FileBrowserSettings,
        gitignore: &gitignore::Gitignore,
    ) -> Vec<&mut FilesystemItem> {
        match self {
            FilesystemItem::File { .. } => vec![],
            FilesystemItem::Folder { children, .. } => children
                .iter_mut()
                .filter(|i| i.should_show(settings, gitignore))
                .collect(),
        }
    }

    fn len(&self, settings: &FileBrowserSettings, gitignore: &gitignore::Gitignore) -> usize {
        match self {
            FilesystemItem::File { .. } => 1,
            FilesystemItem::Folder { open: false, .. } => 1,
            FilesystemItem::Folder { open: true, .. } => {
                self.shown_children(settings, gitignore)
                    .into_iter()
                    .map(|i| i.len(settings, gitignore))
                    .sum::<usize>()
                    + 1
            }
        }
    }

    fn get(
        &mut self,
        index: usize,
        settings: &FileBrowserSettings,
        gitignore: &gitignore::Gitignore,
    ) -> Option<&mut Self> {
        if index == 0 {
            Some(self)
        } else if index >= self.len(settings, gitignore) {
            None
        } else {
            match self {
                FilesystemItem::File { .. } => None,
                FilesystemItem::Folder { open: false, .. } => None,
                FilesystemItem::Folder { open: true, .. } => {
                    let mut current_len = 1;
                    for child in self.shown_children_mut(settings, gitignore) {
                        let new_len = current_len + child.len(settings, gitignore);
                        if new_len > index {
                            return child.get(index - current_len, settings, gitignore);
                        }
                        current_len = new_len;
                    }
                    None
                }
            }
        }
    }

    fn render(&self, padding: String) -> Line {
        let icon = self.icon();
        let name = self.name();

        Line::from(vec![
            Span::from(padding.to_owned()),
            Span::from("\u{200B}"),
            icon,
            Span::from("\u{200B} "),
            Span::from(name),
        ])
    }

    fn render_tree(
        &self,
        depth: usize,
        settings: &FileBrowserSettings,
        gitignore: &gitignore::Gitignore,
        mut padding: String,
    ) -> Vec<Line> {
        let mut lines = Vec::with_capacity(self.len(settings, gitignore));
        lines.push(self.render(padding.clone()));
        if let FilesystemItem::Folder { open, .. } = self {
            if *open {
                let children = self.shown_children(settings, gitignore);
                if padding.get(padding.len().saturating_sub(3)..) == Some("├") {
                    padding.replace_range(padding.len() - 3.., "│");
                } else if padding.get(padding.len().saturating_sub(3)..) == Some("└") {
                    padding.replace_range(padding.len() - 3.., "  ");
                }
                for (i, child) in children.iter().enumerate() {
                    let mut padding = padding.clone();
                    let is_last = i == children.len() - 1;
                    if is_last {
                        padding.push('└');
                    } else {
                        padding.push('├');
                    }

                    lines.extend(child.render_tree(depth + 1, settings, gitignore, padding));
                }
            }
        }
        lines
    }
}

#[derive(Clone)]
struct FileBrowserWindow {
    root_file: FilesystemItem,
    focused: usize,
    gitignore: gitignore::Gitignore,
}

impl FileBrowserWindow {
    fn new() -> Result<Self, io::Error> {
        let mut gitignore = gitignore::GitignoreBuilder::new(PathBuf::from(".").canonicalize()?);
        let _ = gitignore.add(".gitignore");
        let gitignore = gitignore.build().unwrap();
        Ok(Self {
            root_file: FilesystemItem::new(PathBuf::from("."))?,
            focused: 0,
            gitignore,
        })
    }

    fn reload_filesystem_state(&mut self) -> Result<(), io::Error> {
        self.root_file = FilesystemItem::new(PathBuf::from("."))?;
        Ok(())
    }
}

impl arcane_windows::Window for FileBrowserWindow {
    fn name(&self) -> String {
        let name = self.root_file.name().to_string();
        format!("Folder: {name}")
    }

    fn horizontal_constraints(&self) -> ratatui::prelude::Constraint {
        ratatui::prelude::Constraint::Percentage(25)
    }

    // #[errors]
    fn update(
        &mut self,
        events: &mut arcane_core::EventManager,
        plugins: &arcane_core::PluginStore,
        focused: bool,
        _id: arcane_windows::WindowID,
    ) -> arcane_core::Result<()> {
        if !focused {
            return Ok(());
        }

        let Some(settings) = arcane_settings::get_settings::<FileBrowserSettings>(plugins) else {
            return Ok(());
        };

        for event in events.read::<MenuEvent>() {
            match event {
                MenuEvent::Down => {
                    self.focused = self.focused.saturating_add(1);
                }
                MenuEvent::Up => {
                    self.focused = self.focused.saturating_sub(1);
                }
                MenuEvent::Select => {
                    if let Some(item) = self.root_file.get(self.focused, &settings, &self.gitignore)
                    {
                        match item {
                            FilesystemItem::File { abs_path, .. } => {
                                // TODO
                            }
                            FilesystemItem::Folder { .. } => {
                                item.toggle_folder()?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        self.focused = self
            .focused
            .clamp(0, self.root_file.len(&settings, &self.gitignore) - 1);

        Ok(())
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
        plugins: &arcane_core::PluginStore,
    ) {
        let Some(settings) = arcane_settings::get_settings::<FileBrowserSettings>(plugins) else {
            return;
        };

        let mut lines = self
            .root_file
            .render_tree(0, &settings, &self.gitignore, String::from(""));
        if let Some(line) = lines.get_mut(self.focused) {
            *line = line.clone().underlined()
        }

        let visible_lines = area.height as usize;
        if lines.len() > visible_lines {
            let target_focused = visible_lines / 2;
            let max_scroll = lines.len().saturating_sub(visible_lines);
            let scroll = self.focused.saturating_sub(target_focused);
            let scroll = scroll.clamp(0, max_scroll);
            lines = lines.iter().skip(scroll).cloned().collect();
        }

        let text = Text::from(lines);
        frame.render_widget(text, area);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileBrowserSettings {
    show_hidden_files: bool,
    show_hidden_folders: bool,
    show_ignored: bool,
}

impl Default for FileBrowserSettings {
    fn default() -> Self {
        Self {
            show_hidden_files: true,
            show_hidden_folders: true,
            show_ignored: false,
        }
    }
}

#[typetag::serde]
impl arcane_settings::PluginSettings for FileBrowserSettings {
    fn name(&self) -> &'static str {
        "File Browser"
    }

    fn values(&mut self) -> Box<[arcane_settings::SettingsValueCommon]> {
        Box::new([
            arcane_settings::SettingsValueCommon {
                name: "Show hidden files",
                value: arcane_settings::SettingsValue::Toogle(&mut self.show_hidden_files),
            },
            arcane_settings::SettingsValueCommon {
                name: "Show hidden folders",
                value: arcane_settings::SettingsValue::Toogle(&mut self.show_hidden_folders),
            },
            arcane_settings::SettingsValueCommon {
                name: "Show ignored files",
                value: arcane_settings::SettingsValue::Toogle(&mut self.show_ignored),
            },
        ])
    }
}
