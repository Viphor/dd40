//! Debug utilities and components for the game.

use std::collections::HashMap;

use bevy::prelude::*;

struct DebugElement {
    description: &'static str,
    value: String,
    is_visible: bool,
}

impl Default for DebugElement {
    fn default() -> Self {
        Self {
            description: "",
            value: "--".to_string(),
            is_visible: true,
        }
    }
}

#[derive(Component, Default)]
pub struct DebugInfo {
    section: &'static str,
    elements: HashMap<&'static str, DebugElement>,
    pub color: Color,
}

impl std::fmt::Display for DebugInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.section.is_empty() {
            writeln!(f, "=== {} ===", self.section)?;
        }
        for element in self.elements.values() {
            if element.is_visible {
                writeln!(f, "{}: {}", element.description, element.value)?;
            }
        }
        Ok(())
    }
}

impl DebugInfo {
    pub fn new(section: &'static str) -> Self {
        Self {
            section,
            ..Default::default()
        }
    }

    pub fn add(mut self, key: &'static str, description: &'static str) -> Self {
        self.elements.insert(
            key,
            DebugElement {
                description,
                ..Default::default()
            },
        );
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn set(&mut self, key: &'static str, value: impl ToString) {
        if let Some(element) = self.elements.get_mut(key) {
            element.value = value.to_string();
        }
    }

    pub fn set_visible(&mut self, key: &'static str, is_visible: bool) {
        if let Some(element) = self.elements.get_mut(key) {
            element.is_visible = is_visible;
        }
    }
}
