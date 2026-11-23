//! The backup destination model shared by `neo restore` and `neo backup`
//! (docs/design/backup-restore.md §2–4): where each destination is
//! ([`source`]), how this machine reaches it ([`access`]), the encrypted
//! format ([`encrypted`]) and reading a backup back ([`restore`]).

pub mod access;
pub mod config_repo;
pub mod encrypted;
pub mod restore;
pub mod source;
pub mod store_ssh;

pub(crate) use access::default_key;
pub use access::Access;
pub(crate) use restore::mount_zfs;
pub use restore::{check, detect, extract_config, restore_into, RestoreInto};
pub use source::{hostname, volumes_root, Format, Location, Source, StoreRef};
