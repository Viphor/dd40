use core::net::{Ipv4Addr, SocketAddr};

use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};
use dd40_config::RawConfig;
use lightyear::{
    netcode::NetcodeServer,
    prelude::{
        LocalAddr, Server,
        server::{NetcodeConfig, ServerUdpIo, Start},
    },
};

use crate::shared::{
    config::NetworkConfig,
    connection::{SHARED_SETTINGS, parse_private_key_from_str},
};

/// Marker component spawned by [`super::ServerNetworkPlugin`].
///
/// Its `on_add` hook reads [`ServerConfig`] (via [`RawConfig`]) and
/// immediately replaces itself with the full set of lightyear server
/// components needed to open a UDP socket on the configured port.
#[derive(Component, Debug, Clone, Default)]
#[component(on_add = DDServer::on_add)]
pub struct DDServer;

impl DDServer {
    fn on_add(mut world: DeferredWorld, context: HookContext) {
        let entity = context.entity;
        world.commands().queue(move |world: &mut World| -> Result {
            // Read config before borrowing the entity mutably.
            let cfg = world
                .get_resource::<RawConfig>()
                .map(|r| r.section::<NetworkConfig>())
                .unwrap_or_default();

            let private_key = if cfg.private_key.is_empty() {
                SHARED_SETTINGS.private_key
            } else {
                parse_private_key_from_str(&cfg.private_key)
                    .inspect_err(|e| warn!("invalid network.private_key in config: {e}"))
                    .unwrap_or(SHARED_SETTINGS.private_key)
            };

            let mut entity_mut = world.entity_mut(entity);
            entity_mut.remove::<DDServer>();
            entity_mut.insert(Name::from("Server"));
            entity_mut.insert(NetcodeServer::new(NetcodeConfig {
                protocol_id: SHARED_SETTINGS.protocol_id,
                private_key,
                ..Default::default()
            }));
            let server_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), cfg.port);
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
