use std::fmt::Display;

use bevy::{
    log::{Level, LogPlugin},
    utils::default,
};

struct Filter {
    module: &'static str,
    level: Level,
}

struct LogConfig {
    level: Level,
    filters: Vec<Filter>,
}

impl Display for LogConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let filters = self
            .filters
            .iter()
            .map(|filter| format!("{}={}", filter.module, filter.level))
            .collect::<Vec<_>>()
            .join(",");
        write!(f, "{}", filters)
    }
}

/// Returns a [`LogPlugin`] configured for the game.
///
/// When the `debug_logging` feature is enabled, the default log level is set to
/// [`Level::DEBUG`]. Otherwise, it defaults to [`Level::INFO`].
pub fn log_plugin() -> LogPlugin {
    #[cfg(feature = "debug_logging")]
    let config = LogConfig {
        level: Level::INFO,
        filters: vec![
            Filter {
                module: "wgpu",
                level: Level::ERROR,
            },
            Filter {
                module: "bevy_render",
                level: Level::INFO,
            },
            Filter {
                module: "bevy_ecs",
                level: Level::WARN,
            },
            Filter {
                module: "bevy_time",
                level: Level::WARN,
            },
            Filter {
                module: "naga",
                level: Level::WARN,
            },
            Filter {
                module: "bevy_enhanced_input::action::fns",
                level: Level::ERROR,
            },
            Filter {
                module: "cosmic_text",
                level: Level::WARN,
            },
            Filter {
                module: "dd40_core",
                level: Level::DEBUG,
            },
            Filter {
                module: "dd40_world",
                level: Level::DEBUG,
            },
            Filter {
                module: "dd40_player",
                level: Level::DEBUG,
            },
            Filter {
                module: "dd40_network",
                level: Level::DEBUG,
            },
            Filter {
                module: "dd40_chunk_storage",
                level: Level::DEBUG,
            },
        ],
    };
    #[cfg(not(feature = "debug_logging"))]
    let config = LogConfig {
        level: Level::INFO,
        filters: vec![
            Filter {
                module: "wgpu",
                level: Level::ERROR,
            },
            Filter {
                module: "bevy_render",
                level: Level::INFO,
            },
            Filter {
                module: "bevy_ecs",
                level: Level::WARN,
            },
            Filter {
                module: "bevy_time",
                level: Level::WARN,
            },
            Filter {
                module: "naga",
                level: Level::WARN,
            },
            Filter {
                module: "bevy_enhanced_input::action::fns",
                level: Level::ERROR,
            },
            Filter {
                module: "cosmic_text",
                level: Level::WARN,
            },
        ],
    };

    LogPlugin {
        level: config.level,
        filter: config.to_string(),
        ..default()
    }
}
