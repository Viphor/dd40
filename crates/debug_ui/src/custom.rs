use bevy::prelude::*;
use dd40_core::debug::DebugInfo;

#[derive(Component)]
#[relationship(relationship_target = UiElementOf)]
pub(crate) struct UiDataFor(Entity);

#[derive(Component)]
#[relationship_target(relationship = UiDataFor, linked_spawn)]
pub(crate) struct UiElementOf(Entity);

#[derive(Component)]
pub(crate) struct DebugUiElementRoot;

pub(crate) fn spawn_custom_debug_ui(
    root: Single<Entity, With<DebugUiElementRoot>>,
    query: Query<(Entity, &DebugInfo), Added<DebugInfo>>,
    mut commands: Commands,
) {
    for (entity, element) in query.iter() {
        let text_entity = commands
            .spawn((
                Text::new(element.to_string()),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(element.color),
                Node {
                    position_type: PositionType::Relative,
                    ..default()
                },
            ))
            .id();

        commands.entity(root.entity()).add_child(text_entity);
        commands.entity(entity).insert(UiDataFor(text_entity));
    }
}

pub(crate) fn update_custom_debug_ui(
    query: Query<(&DebugInfo, &UiDataFor), Changed<DebugInfo>>,
    mut ui_elements: Query<&mut Text, With<UiElementOf>>,
) {
    for (element, entity) in query.iter() {
        if let Ok(mut text) = ui_elements.get_mut(entity.0) {
            **text = format!("{}", element);
        }
    }
}
