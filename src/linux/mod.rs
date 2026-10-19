mod desktop;
pub(crate) mod engine;
mod media;
pub use desktop::run;
pub use media::media_self_test;
pub use media::publish_self_test;

#[cfg(test)]
pub(crate) use media::decoder_count;
