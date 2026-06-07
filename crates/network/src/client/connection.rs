use core::net::{IpAddr, Ipv4Addr, SocketAddr};

use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};
use dd40_config::RawConfig;
use dd40_core::prelude::{LoadingTracker, RequestChunk};
use lightyear::{
    link::{Link, RecvLinkConditioner},
    netcode::NetcodeClient,
    prelude::{
        Authentication, Client, Connect, Connected, InputTimelineConfig, LocalAddr,
        MessageReceiver, MessageSender, PeerAddr, PredictionManager, ReplicationReceiver, UdpIo,
        client::{InputDelayConfig, NetcodeConfig},
    },
};

use crate::{
    client::{config::ClientConfig, loading::register_spawn_location_loading_item},
    protocol::*,
    shared::connection::SHARED_SETTINGS,
};
use crate::client::loading::remove_connection_loading_item;

/// Marker component spawned by [`super::plugin::ClientNetworkPlugin`].
///
/// Its `on_add` hook reads [`ClientConfig`] (via [`RawConfig`]) and
/// immediately replaces itself with the full set of lightyear client
/// components needed to open a UDP connection to the configured server.
#[derive(Component, Debug, Clone, Default)]
#[component(on_add = DDClient::on_add)]
pub struct DDClient;

impl DDClient {
    fn on_add(mut world: DeferredWorld, context: HookContext) {
        let entity = context.entity;
        world.commands().queue(move |world: &mut World| -> Result {
            // Read config before taking the mutable entity borrow.
            let client_cfg = world
                .get_resource::<RawConfig>()
                .map(|r| r.section::<ClientConfig>())
                .unwrap_or_default();

            let server_addr = SocketAddr::new(
                client_cfg
                    .server_host
                    .parse::<IpAddr>()
                    .unwrap_or_else(|e| {
                        warn!(
                            "invalid client.server_host {:?}: {e} — falling back to 127.0.0.1",
                            client_cfg.server_host
                        );
                        Ipv4Addr::LOCALHOST.into()
                    }),
                client_cfg.server_port,
            );
            let client_id: u64 = rand::random();
            let client_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0);

            let mut entity_mut = world.entity_mut(entity);
            entity_mut.remove::<DDClient>();
            entity_mut.insert((
                Client::default(),
                Link::new(None::<RecvLinkConditioner>),
                LocalAddr(client_addr),
                PeerAddr(server_addr),
                ReplicationReceiver::default(),
                PredictionManager::default(),
                // At 30 Hz one tick ≈ 33 ms.  Allow up to 1 tick of input delay
                // to absorb minor jitter before falling back to rollback prediction.
                // This dramatically reduces rollbacks on lossy connections without
                // adding noticeable input lag.
                InputTimelineConfig::default().with_input_delay(InputDelayConfig {
                    minimum_input_delay_ticks: 0,
                    maximum_input_delay_before_prediction: 1,
                    maximum_predicted_ticks: 15,
                }),
                Name::from("Client"),
            ));

            let auth = Authentication::Manual {
                server_addr,
                client_id,
                private_key: SHARED_SETTINGS.private_key,
                protocol_id: SHARED_SETTINGS.protocol_id,
            };
            let netcode_config = NetcodeConfig {
                client_timeout_secs: 3,
                token_expire_secs: -1,
                ..default()
            };
            entity_mut.insert(NetcodeClient::new(auth, netcode_config)?);
            entity_mut.insert(UdpIo::default());

            Ok(())
        });
    }
}

pub(crate) fn connect(mut commands: Commands, client: Single<Entity, With<Client>>) {
    commands.trigger(Connect {
        entity: client.into_inner(),
    });
}

/// Observer that fires when lightyear adds the [`Connected`] component to the
/// client entity, i.e. when the server handshake completes.
///
/// Attaches all required [`MessageSender`] and [`MessageReceiver`] components
/// to the connection entity and clears the `"network:server_connection"` gate.
///
/// The `"network:initial_chunks"` gate is registered later, when the server
/// sends [`PlayerSpawnLocation`], so that the timeout only starts counting
/// once we actually have something to wait for.
pub fn on_server_connected(
    trigger: On<Add, Connected>,
    mut commands: Commands,
    mut tracker: ResMut<LoadingTracker>,
) {
    let entity = trigger.entity;

    commands.entity(entity).insert((
        MessageSender::<RequestSpawn>::default(),
        MessageSender::<RequestChunk>::default(),
        MessageSender::<NetSlotInteraction>::default(),
        MessageReceiver::<ChunkSnapshot>::default(),
        MessageReceiver::<ChunkUpdate>::default(),
        MessageReceiver::<ChunkRejection>::default(),
        MessageReceiver::<PlayerSpawnLocation>::default(),
        Name::new("ServerConnection"),
    ));

    remove_connection_loading_item(&mut tracker);
    register_spawn_location_loading_item(&mut tracker);
}
