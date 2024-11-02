//! Loads all builtin and third party plugins
#![allow(clippy::module_name_repetitions)]

use crate::plugin_manager::PluginStore;
use crate::prelude::*;

mod fps;
pub mod keybindings;
mod logs;
pub mod settings;
mod splashscreen;
pub mod windows;

/// Load all functions
pub(crate) fn load_plugins(store: &mut PluginStore) {
    event!(Level::INFO, "Loading plugins.");
    store.insert(fps::FpsPlugin::new());
    store.insert(windows::WindowPlugin::new());
    store.insert(keybindings::KeybindPlugin::new());
    store.insert(splashscreen::SplashScreenPlugin);
    store.insert(logs::LogPlugin);
    store.insert(settings::SettingsPlugin::new());
}
