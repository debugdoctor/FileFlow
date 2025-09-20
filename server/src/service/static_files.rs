use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../web/dist"]
pub struct StaticFiles;