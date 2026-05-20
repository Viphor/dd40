//! Type-erased per-block typed-data extension system.
//!
//! # Why
//!
//! Many gameplay systems want to attach typed data to either a block
//! *type* (e.g. a loot table on every stone block) or a *specific block
//! cell* (e.g. the items inside a chest, the text on a sign).  Hard-coding
//! these fields onto [`BlockDefinition`] or [`Chunk`] would force every
//! such system into `dd40_core`, which defeats the point of the modular
//! architecture.
//!
//! Instead, downstream crates define their own data type, implement
//! [`BlockData`], register the type with the app via
//! [`BlockDataAppExt::register_block_data`], and then either:
//!
//! 1. Attach a default instance to a [`BlockDefinition`] (block-type
//!    scoped — same value for every cell of that type), or
//! 2. Store an instance against a `BlockPos` in the chunk's typed-data
//!    map (cell scoped — only present when meaningful).
//!
//! This module ships the trait, type registry, and `App` extension.
//! Items (1) and (2) are implemented in follow-up tasks (S2 + S3).
//!
//! # Wire / disk identity
//!
//! Each registered type is identified two ways:
//!
//! - In memory by [`std::any::TypeId`] — the fast path.
//! - On the wire / on disk by the string returned from
//!   [`std::any::type_name::<T>`] — recorded at registration time and
//!   sent inside [`NetworkedChunkChange`](crate::chunk::NetworkedChunkChange)
//!   (forthcoming) and chunk save files.
//!
//! Implementations of [`BlockData::type_key`] **must** return the same
//! string: the canonical pattern is:
//!
//! ```ignore
//! fn type_key(&self) -> &'static str { std::any::type_name::<Self>() }
//! ```
//!
//! [`BlockDefinition`]: crate::block::BlockDefinition
//! [`Chunk`]: crate::chunk::Chunk

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt::Debug,
};

use bevy::app::App;
use bevy::ecs::resource::Resource;
use serde::de::DeserializeOwned;

/// A type-erased, serialisable, cloneable piece of data that can live on a
/// [`BlockDefinition`](crate::block::BlockDefinition) or against a specific
/// block cell.
///
/// # Implementing
///
/// There is **no blanket impl**.  Each consumer crate provides its own
/// `impl BlockData for MyType` so that the trait stays open for additional
/// supertraits (e.g. reflection) without breaking existing implementors.
///
/// ```
/// use dd40_core::block::data::BlockData;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// pub struct MyMarker;
///
/// impl BlockData for MyMarker {
///     fn type_key(&self) -> &'static str { std::any::type_name::<Self>() }
///     fn clone_box(&self) -> Box<dyn BlockData> { Box::new(self.clone()) }
///     fn as_any(&self) -> &dyn std::any::Any { self }
/// }
/// ```
///
/// # Bounds
///
/// - `erased_serde::Serialize` — lets the chunk authority serialise
///   `Box<dyn BlockData>` without knowing the concrete type.
/// - `Any` — enables type-id lookups and downcasting on read.
/// - `Send + Sync` — required for storage in Bevy resources/components.
/// - `Debug` — every `BlockData` shows up in log lines on rejection paths.
pub trait BlockData: erased_serde::Serialize + Any + Send + Sync + Debug {
    /// Canonical wire/disk identifier for this type.
    ///
    /// The string is paired with the value in serialised form so the
    /// receiving end can find the correct [`BlockDataDecoder`].  By
    /// convention this is [`std::any::type_name::<Self>`].
    fn type_key(&self) -> &'static str;

    /// Returns a fully-owned copy of `self` in its erased form.
    ///
    /// Used by the chunk cache when it needs to duplicate block data —
    /// for example when sending a snapshot to a newly connected client.
    fn clone_box(&self) -> Box<dyn BlockData>;

    /// Upcast to [`Any`] for downcasting.
    ///
    /// Implementations should always be the trivial `self`.  The shim is
    /// required because trait-object upcasting from `&dyn BlockData` to
    /// `&dyn Any` is not yet stable across all supported toolchains.
    fn as_any(&self) -> &dyn Any;
}

erased_serde::serialize_trait_object!(BlockData);

impl Clone for Box<dyn BlockData> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Function signature used by the [`BlockDataTypeRegistry`] to materialise
/// a `Box<dyn BlockData>` from a type-erased deserializer.
pub type BlockDataDecoder =
    fn(&mut dyn erased_serde::Deserializer<'_>) -> Result<Box<dyn BlockData>, erased_serde::Error>;

/// Metadata recorded for each registered [`BlockData`] type.
#[derive(Clone, Copy)]
pub struct BlockDataTypeInfo {
    /// Runtime type identifier of the registered type.  Carried here so
    /// the wire decoder can rebuild [`CellDataChange::Clear`] entries
    /// (which key on `TypeId`) from a `type_key` lookup alone.
    ///
    /// [`CellDataChange::Clear`]: crate::chunk::change::CellDataChange::Clear
    pub type_id: TypeId,
    /// Wire / disk string identifier — `std::any::type_name::<T>()` at
    /// registration time.
    pub type_key: &'static str,
    /// Decoder that materialises a boxed value from an erased
    /// deserializer.
    pub decoder: BlockDataDecoder,
}

impl Debug for BlockDataTypeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockDataTypeInfo")
            .field("type_key", &self.type_key)
            .finish()
    }
}

/// Runtime registry of every known [`BlockData`] type.
///
/// Inserted by [`CorePlugin`](crate::plugin::CorePlugin) as a default
/// resource; populated via [`BlockDataAppExt::register_block_data`].
///
/// Look-up is keyed by [`TypeId`] for in-memory work and by wire string
/// for network / disk deserialisation.
#[derive(Resource, Default, Debug)]
pub struct BlockDataTypeRegistry {
    by_type_id: HashMap<TypeId, BlockDataTypeInfo>,
    by_type_key: HashMap<&'static str, TypeId>,
}

impl BlockDataTypeRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `T` so its serialised form can be decoded later.
    ///
    /// Idempotent — calling this with the same `T` more than once is a
    /// no-op.  Returns `true` if the type was newly inserted.
    ///
    /// # Panics
    ///
    /// Panics if a *different* type has previously been registered under
    /// the same `type_name` string — this would silently corrupt wire
    /// decoding and is therefore a hard error.
    pub fn register<T: BlockData + DeserializeOwned>(&mut self) -> bool {
        let tid = TypeId::of::<T>();
        if self.by_type_id.contains_key(&tid) {
            return false;
        }
        let type_key = std::any::type_name::<T>();
        if let Some(existing) = self.by_type_key.get(type_key) {
            if *existing != tid {
                panic!(
                    "BlockDataTypeRegistry: type_name `{type_key}` is already \
                     registered to a different TypeId; this would corrupt \
                     wire decoding",
                );
            }
        }
        let decoder: BlockDataDecoder = |d| {
            let value: T = erased_serde::deserialize(d)?;
            Ok(Box::new(value) as Box<dyn BlockData>)
        };
        self.by_type_id.insert(
            tid,
            BlockDataTypeInfo {
                type_id: tid,
                type_key,
                decoder,
            },
        );
        self.by_type_key.insert(type_key, tid);
        true
    }

    /// Returns the metadata for `T` if it has been registered.
    pub fn get<T: BlockData>(&self) -> Option<&BlockDataTypeInfo> {
        self.by_type_id.get(&TypeId::of::<T>())
    }

    /// Returns the metadata for a type identified by its [`TypeId`].
    ///
    /// Useful for code paths that only have an erased `&dyn BlockData`
    /// in hand — e.g. the chunk authority's cell-data validator.
    pub fn get_by_type_id(&self, type_id: TypeId) -> Option<&BlockDataTypeInfo> {
        self.by_type_id.get(&type_id)
    }

    /// Returns the metadata for a wire-key string if it has been
    /// registered.
    pub fn get_by_key(&self, type_key: &str) -> Option<&BlockDataTypeInfo> {
        let tid = self.by_type_key.get(type_key)?;
        self.by_type_id.get(tid)
    }

    /// Decodes a wire-keyed payload through the registry, producing the
    /// matching boxed [`BlockData`].
    ///
    /// Returns `Err` when the key is unknown or when the inner decoder
    /// fails.  The unknown-key case must be handled by the caller — the
    /// chunk authority logs a `warn!` and drops the change.
    pub fn decode(
        &self,
        type_key: &str,
        d: &mut dyn erased_serde::Deserializer<'_>,
    ) -> Result<Box<dyn BlockData>, BlockDataDecodeError> {
        let info = self
            .get_by_key(type_key)
            .ok_or_else(|| BlockDataDecodeError::UnknownType(type_key.to_owned()))?;
        (info.decoder)(d).map_err(BlockDataDecodeError::Serde)
    }

    /// Returns the number of registered types.  Mostly useful in tests.
    pub fn len(&self) -> usize {
        self.by_type_id.len()
    }

    /// Returns `true` if no types are registered.
    pub fn is_empty(&self) -> bool {
        self.by_type_id.is_empty()
    }
}

/// Failure mode for [`BlockDataTypeRegistry::decode`].
#[derive(Debug)]
pub enum BlockDataDecodeError {
    /// The wire `type_key` has not been registered with this app.
    UnknownType(String),
    /// The decoder ran but the underlying serde format rejected the
    /// payload.
    Serde(erased_serde::Error),
}

impl std::fmt::Display for BlockDataDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownType(k) => write!(f, "unknown block-data type key `{k}`"),
            Self::Serde(e) => write!(f, "block-data deserialisation failed: {e}"),
        }
    }
}

impl std::error::Error for BlockDataDecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Serde(e) => Some(e),
            Self::UnknownType(_) => None,
        }
    }
}

/// Extension trait that lets crates register their [`BlockData`] types on
/// an [`App`].
///
/// Mirrors the shape of `App::add_message::<T>()` and
/// lightyear's `App::register_message::<T>()`.  Calling
/// [`register_block_data::<T>`](Self::register_block_data) is idempotent
/// so plugins can safely call it from `Plugin::build` even when another
/// plugin has already registered the same type.
pub trait BlockDataAppExt {
    /// Records `T` in this app's [`BlockDataTypeRegistry`], inserting the
    /// registry resource if it does not yet exist.
    fn register_block_data<T: BlockData + DeserializeOwned>(&mut self) -> &mut Self;
}

impl BlockDataAppExt for App {
    fn register_block_data<T: BlockData + DeserializeOwned>(&mut self) -> &mut Self {
        let world = self.world_mut();
        if world.get_resource::<BlockDataTypeRegistry>().is_none() {
            world.insert_resource(BlockDataTypeRegistry::new());
        }
        let mut registry = world.resource_mut::<BlockDataTypeRegistry>();
        registry.register::<T>();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Marker {
        n: u32,
        label: String,
    }

    impl BlockData for Marker {
        fn type_key(&self) -> &'static str {
            std::any::type_name::<Self>()
        }
        fn clone_box(&self) -> Box<dyn BlockData> {
            Box::new(self.clone())
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Other(i64);
    impl BlockData for Other {
        fn type_key(&self) -> &'static str {
            std::any::type_name::<Self>()
        }
        fn clone_box(&self) -> Box<dyn BlockData> {
            Box::new(self.clone())
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[test]
    fn register_is_idempotent() {
        let mut reg = BlockDataTypeRegistry::new();
        assert!(reg.register::<Marker>());
        assert!(!reg.register::<Marker>());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn app_ext_inserts_resource_and_records_type() {
        let mut app = App::new();
        app.register_block_data::<Marker>();
        let reg = app.world().resource::<BlockDataTypeRegistry>();
        assert!(reg.get::<Marker>().is_some());
        assert!(reg.get::<Other>().is_none());
    }

    #[test]
    fn app_ext_is_idempotent_across_plugins() {
        let mut app = App::new();
        app.register_block_data::<Marker>();
        app.register_block_data::<Marker>();
        app.register_block_data::<Other>();
        let reg = app.world().resource::<BlockDataTypeRegistry>();
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn type_key_round_trips_through_registry() {
        let mut reg = BlockDataTypeRegistry::new();
        reg.register::<Marker>();

        let value = Marker {
            n: 42,
            label: "answer".to_owned(),
        };
        let key = value.type_key();
        assert_eq!(key, std::any::type_name::<Marker>());

        // Serialise as a trait object using erased-serde, then decode
        // back through the registry's wire-key lookup.
        let bytes = {
            let boxed: Box<dyn BlockData> = Box::new(value.clone());
            serde_json::to_vec(&boxed).expect("erased serialize")
        };

        let mut json_de = serde_json::Deserializer::from_slice(&bytes);
        let mut erased = <dyn erased_serde::Deserializer>::erase(&mut json_de);
        let decoded = reg.decode(key, &mut erased).expect("decode");

        let downcast = (decoded as Box<dyn Any>)
            .downcast::<Marker>()
            .expect("downcast to Marker");
        assert_eq!(*downcast, value);
    }

    #[test]
    fn decode_unknown_key_errors() {
        let reg = BlockDataTypeRegistry::new();
        let mut json_de = serde_json::Deserializer::from_slice(b"null");
        let mut erased = <dyn erased_serde::Deserializer>::erase(&mut json_de);
        let err = reg.decode("nope::Nope", &mut erased).unwrap_err();
        assert!(matches!(err, BlockDataDecodeError::UnknownType(_)));
    }

    #[test]
    fn clone_box_produces_equivalent_value() {
        let original: Box<dyn BlockData> = Box::new(Marker {
            n: 7,
            label: "seven".to_owned(),
        });
        let copy = original.clone();
        let bytes_a = serde_json::to_vec(&original).unwrap();
        let bytes_b = serde_json::to_vec(&copy).unwrap();
        assert_eq!(bytes_a, bytes_b);
    }
}
