use core::net::{Ipv4Addr, SocketAddr};

use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};
pub use lightyear::{link::RecvLinkConditioner, prelude::LinkConditionerConfig};
use lightyear::{
    netcode::NetcodeServer,
    prelude::{
        LocalAddr, Server,
        server::{NetcodeConfig, ServerUdpIo, Start},
    },
};

use crate::shared::connection::{SHARED_SETTINGS, SharedSettings, parse_private_key_from_env};

#[derive(Component, Debug, Clone)]
#[component(on_add = DDServer::on_add)]
pub struct DDServer {
    /// TODO: Add support for conditioner
    pub conditioner: Option<RecvLinkConditioner>,
    pub port: u16,
    pub shared: SharedSettings,
}

impl DDServer {
    pub fn new(port: u16) -> Self {
        Self {
            conditioner: None,
            port,
            shared: SHARED_SETTINGS,
        }
    }

    fn on_add(mut world: DeferredWorld, context: HookContext) {
        let entity = context.entity;
        world.commands().queue(move |world: &mut World| -> Result {
            let mut entity_mut = world.entity_mut(entity);
            let settings = entity_mut.take::<DDServer>().unwrap();
            entity_mut.insert((Name::from("Server"),));

            // Use private key from environment variable, if set. Otherwise from settings file.
            let private_key = if let Some(key) = parse_private_key_from_env() {
                info!("Using private key from LIGHTYEAR_PRIVATE_KEY env var");
                key
            } else {
                settings.shared.private_key
            };

            entity_mut.insert(NetcodeServer::new(NetcodeConfig {
                protocol_id: settings.shared.protocol_id,
                private_key,
                ..Default::default()
            }));
            let server_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), settings.port);
            entity_mut.insert((LocalAddr(server_addr), ServerUdpIo::default()));

            Ok(())
        });
    }
}

pub(crate) fn start(mut commands: Commands, server: Single<Entity, With<Server>>) {
    commands.trigger(Start {
        entity: server.into_inner(),
    });
}
