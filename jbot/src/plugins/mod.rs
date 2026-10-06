#[cfg(feature = "bili")]
mod bili;

#[cfg(feature = "douyin")]
mod douyin;

#[cfg(feature = "group_config")]
pub mod group_config;

mod help;

#[cfg(feature = "merge")]
pub mod merge;

#[cfg(feature = "roll")]
mod roll;

#[cfg(feature = "spoiler")]
pub mod spoiler;

#[cfg(feature = "twitter")]
mod twitter;

#[cfg(feature = "verify")]
mod verify;

#[cfg(feature = "youtube")]
mod youtube;

#[cfg(feature = "hosting")]
pub mod hosting;
