//! Manages app settings.
#![feature(trait_upcasting)]

use std::any::Any;
use std::cell::Ref;

use arcane_anymap::{dyn_clone, AnyMap};
use arcane_core::{event, project_dirs, Level, Result};
use error_mancer::errors;

/// The value in enum
#[derive(Debug)]
pub enum SettingsValue<'v> {
    /// A integer value
    Integer {
        /// The actual value
        value: &'v mut i32,
        /// Minimum value
        min: i32,
        /// Maximum value
        max: i32,
        /// Step
        step: i32,
    },
    /// Multiple Kinds of Values
    Selection(&'v mut String, &'static [&'static str]),
    /// A toogle
    Toogle(&'v mut bool),
}

/// Common metadata for settings
#[derive(Debug)]
pub struct SettingsValueCommon<'v> {
    /// The name of the settings
    pub name: &'static str,
    /// A mutable reference to the data that needs changing
    pub value: SettingsValue<'v>,
}

impl SettingsValueCommon<'_> {
    /// Update a settings
    ///
    /// `alt_mode` indicated the update should happen in the other direction.
    pub fn handle_settings_update(self, alt_mode: bool) {
        match self.value {
            SettingsValue::Toogle(value) => {
                *value = !*value;
            }
            SettingsValue::Selection(value, possible) => {
                let selected = possible
                    .iter()
                    .enumerate()
                    .find_map(|(index, p)| (p == value).then_some(index))
                    .unwrap_or_default();

                let selected = if alt_mode {
                    selected
                        .checked_sub(1)
                        .unwrap_or(possible.len().saturating_sub(1))
                } else {
                    selected
                        .saturating_add(1)
                        .checked_rem(possible.len())
                        .unwrap_or_default()
                };

                if let Some(new) = possible.get(selected) {
                    *value = String::from(*new);
                }
            }
            SettingsValue::Integer {
                value,
                min,
                max,
                step,
            } => {
                if alt_mode {
                    *value = value.saturating_sub(step).min(max).max(min);
                } else {
                    *value = value.saturating_add(step).min(max).max(min);
                }
            }
        }
    }
}

/// Settings for a specific plugin
#[typetag::serde(tag = "plugin", content = "settings")]
pub trait PluginSettings: Any + dyn_clone::DynClone {
    /// Used for serializing and loading settings.
    fn name(&self) -> &'static str;
    /// The values in the settings menu
    fn values(&mut self) -> Box<[SettingsValueCommon]>;
}

impl arcane_anymap::Downcast for dyn PluginSettings {
    fn downcast<T>(this: &Self) -> Option<&T>
    where
        T: 'static,
    {
        (this as &dyn Any).downcast_ref()
    }
    fn downcast_mut<T>(this: &mut Self) -> Option<&mut T>
    where
        T: 'static,
    {
        (this as &mut dyn Any).downcast_mut()
    }
}

impl<P: PluginSettings> arcane_anymap::IntoBoxed<dyn PluginSettings> for (P,) {
    fn into(self) -> Box<dyn PluginSettings> {
        Box::new(self.0)
    }
}

/// Register a new settings, thing
pub struct RegisterSettings(pub Box<dyn PluginSettings>);

/// The settings management plugin
pub struct SettingsPlugin {
    /// The settings for each plugin
    pub settings: AnyMap<dyn PluginSettings>,
}

arcane_core::register_plugin!(SettingsPlugin);

impl SettingsPlugin {
    /// Get a settings object from the plugin
    pub fn get<S: PluginSettings>(&self) -> Option<&S> {
        self.settings.get::<S>()
    }

    /// Get a sorted version of the settings list
    pub fn sorted_settings(&mut self) -> Vec<&mut Box<dyn PluginSettings>> {
        let mut plugins = self.settings.iter_mut().collect::<Vec<_>>();
        plugins.sort_by_key(|plugin| plugin.name());
        plugins
    }
}

/// Convnient method that retrives the plugin from the store, then your settings from the plugin
pub fn get_settings<S: PluginSettings>(store: &arcane_core::PluginStore) -> Option<Ref<S>> {
    let Some(plugin) = store.get::<SettingsPlugin>() else {
        event!(Level::ERROR, "Settings plugin not found");
        return None;
    };
    let Ok(settings) = Ref::filter_map(plugin, SettingsPlugin::get) else {
        event!(Level::ERROR, "Failed to get settings");
        return None;
    };
    Some(settings)
}

/// Save the settings
#[derive(Clone, Debug)]
pub struct SaveSettings;

#[errors]
impl arcane_core::Plugin for SettingsPlugin {
    fn new() -> Self {
        Self {
            settings: AnyMap::new(),
        }
    }

    #[errors(serde_json::Error)]
    fn on_load(&mut self, _events: &mut arcane_core::EventManager) -> Result<()> {
        if let Some(project_directory) = project_dirs() {
            let config_path = project_directory.config_dir().join("config.json");
            if let Ok(file) = std::fs::File::open(&config_path) {
                // let data: Vec<Box<dyn PluginSettings>> = serde_json::de::from_reader(file)?;
                // for value in data {
                //     self.settings.insert_raw(value);
                // }
                let data: Vec<serde_json::Value> = serde_json::from_reader(file)?;
                event!(Level::DEBUG, "loading {} settings", data.len());
                for value in data {
                    if let Ok(value) = serde_json::from_value(value) {
                        self.settings.insert_raw(value);
                    } else {
                        event!(Level::ERROR, "Invalid settings entry!");
                    }
                }
                event!(Level::DEBUG, "Loaded {} settings", self.settings.len());
            }
        }

        Ok(())
    }

    #[errors(std::io::Error, serde_json::Error)]
    fn update(
        &mut self,
        events: &mut arcane_core::EventManager,
        _plugins: &arcane_core::PluginStore,
    ) -> Result<()> {
        for event in events.read::<RegisterSettings>() {
            let settings = dyn_clone::clone_box(&*event.0);
            self.settings.insert_raw_if_missing(settings);
        }

        if !events.read::<SaveSettings>().is_empty() {
            let Some(project_directory) = project_dirs() else {
                return Ok(());
            };
            let config_dir = project_directory.config_dir();

            std::fs::create_dir_all(config_dir)?;
            let config_path = config_dir.join("config.json");
            event!(Level::INFO, "Saving config to {config_path:?}");
            let file = std::fs::File::create(config_path)?;

            let settings = self.settings.iter().collect::<Vec<_>>();
            serde_json::ser::to_writer_pretty(file, &settings)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{SettingsValue, SettingsValueCommon};

    #[test]
    fn select_one() {
        let mut value = String::from("1");
        let settings_value = SettingsValueCommon {
            name: "test",
            value: SettingsValue::Selection(&mut value, &["1", "2"]),
        };
        settings_value.handle_settings_update(false);

        assert_eq!(value, "2");
    }

    #[test]
    fn select_one_alt() {
        let mut value = String::from("2");
        let settings_value = SettingsValueCommon {
            name: "test",
            value: SettingsValue::Selection(&mut value, &["1", "2", "3"]),
        };
        settings_value.handle_settings_update(true);

        assert_eq!(value, "1");
    }

    #[test]
    fn select_overflow() {
        let mut value = String::from("1");

        let settings_value = SettingsValueCommon {
            name: "test",
            value: SettingsValue::Selection(&mut value, &["1", "2"]),
        };
        settings_value.handle_settings_update(false);

        let settings_value = SettingsValueCommon {
            name: "test",
            value: SettingsValue::Selection(&mut value, &["1", "2"]),
        };
        settings_value.handle_settings_update(false);

        assert_eq!(value, "1");
    }

    #[test]
    fn select_overflow_alt() {
        let mut value = String::from("1");
        let settings_value = SettingsValueCommon {
            name: "test",
            value: SettingsValue::Selection(&mut value, &["1", "2", "3"]),
        };
        settings_value.handle_settings_update(true);

        assert_eq!(value, "3");
    }

    #[test]
    fn select_invalid() {
        let mut value = String::from("invalid");

        let settings_value = SettingsValueCommon {
            name: "test",
            value: SettingsValue::Selection(&mut value, &["1", "2"]),
        };
        settings_value.handle_settings_update(false);
    }

    #[test]
    fn toggle_true() {
        let mut value = false;

        let settings_value = SettingsValueCommon {
            name: "test",
            value: SettingsValue::Toogle(&mut value),
        };
        settings_value.handle_settings_update(false);

        assert!(value);
    }

    #[test]
    fn toggle_false() {
        let mut value = true;

        let settings_value = SettingsValueCommon {
            name: "test",
            value: SettingsValue::Toogle(&mut value),
        };
        settings_value.handle_settings_update(false);

        assert!(!value);
    }

    #[test]
    fn integer() {
        let mut value = 0;

        let settings_value = SettingsValueCommon {
            name: "test",
            value: SettingsValue::Integer {
                value: &mut value,
                min: 0,
                max: 10,
                step: 1,
            },
        };
        settings_value.handle_settings_update(false);

        assert_eq!(value, 1);
    }

    #[test]
    fn integer_step() {
        let mut value = 0;

        let settings_value = SettingsValueCommon {
            name: "test",
            value: SettingsValue::Integer {
                value: &mut value,
                min: 0,
                max: 10,
                step: 5,
            },
        };
        settings_value.handle_settings_update(false);

        assert_eq!(value, 5);
    }

    #[test]
    fn integer_max() {
        let mut value = 0;

        let settings_value = SettingsValueCommon {
            name: "test",
            value: SettingsValue::Integer {
                value: &mut value,
                min: 0,
                max: 10,
                step: 20,
            },
        };
        settings_value.handle_settings_update(false);

        assert_eq!(value, 10);
    }

    #[test]
    fn integer_alt() {
        let mut value = 5;

        let settings_value = SettingsValueCommon {
            name: "test",
            value: SettingsValue::Integer {
                value: &mut value,
                min: 0,
                max: 10,
                step: 1,
            },
        };
        settings_value.handle_settings_update(true);

        assert_eq!(value, 4);
    }

    #[test]
    fn integer_min() {
        let mut value = 5;

        let settings_value = SettingsValueCommon {
            name: "test",
            value: SettingsValue::Integer {
                value: &mut value,
                min: 0,
                max: 10,
                step: 20,
            },
        };
        settings_value.handle_settings_update(true);

        assert_eq!(value, 0);
    }
}
