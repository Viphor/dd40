use core::net::{Ipv4Addr, SocketAddr};

use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};
use lightyear::{
    link::Link,
    netcode::NetcodeClient,
    prelude::{
        Authentication, Client, Connect, LocalAddr, PeerAddr, PredictionManager,
        ReplicationReceiver, UdpIo, client::NetcodeConfig,
    },
};
pub use lightyear::{link::RecvLinkConditioner, prelude::LinkConditionerConfig};

use crate::connection::shared::{SHARED_SETTINGS, SharedSettings};

#[derive(Component, Debug, Clone)]
#[component(on_add = DDClient::on_add)]
pub struct DDClient {
    pub client_id: u64,
    pub client_port: u16,
    pub server_addr: SocketAddr,
    pub conditioner: Option<RecvLinkConditioner>,
    pub shared: SharedSettings,
}

impl DDClient {
    pub fn new(client_port: u16, server_addr: SocketAddr) -> Self {
        Self {
            client_id: rand::random(),
            client_port,
            server_addr,
            conditioner: None,
            shared: SHARED_SETTINGS,
        }
    }

    fn on_add(mut world: DeferredWorld, context: HookContext) {
        let entity = context.entity;
        world.commands().queue(move |world: &mut World| -> Result {
            let mut entity_mut = world.entity_mut(entity);
            let settings = entity_mut.take::<DDClient>().unwrap();
            let client_addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), settings.client_port);
            entity_mut.insert((
                Client::default(),
                Link::new(settings.conditioner.clone()),
                LocalAddr(client_addr),
                PeerAddr(settings.server_addr),
                ReplicationReceiver::default(),
                PredictionManager::default(),
                Name::from("Client"),
            ));

            let auth = Authentication::Manual {
                server_addr: settings.server_addr,
                client_id: settings.client_id,
                private_key: settings.shared.private_key,
                protocol_id: settings.shared.protocol_id,
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
