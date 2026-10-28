//! Nix extractor files: written into the repl eval dir, each bound as `load_name`.
pub struct NixExtractor {
    pub file_name: &'static str,
    pub content: &'static str,
    pub load_name: &'static str,
}

macro_rules! extractor {
    ($file:literal, $load_name:literal) => {
        NixExtractor {
            file_name: $file,
            content: include_str!($file),
            load_name: $load_name,
        }
    };
}

pub static EXTRACT_SERVICES: NixExtractor = extractor!("extract_services.nix", "extractServices");
pub static EXTRACT_SERVICE_OPTIONS: NixExtractor =
    extractor!("extract_service_options.nix", "extractServiceOptions");
pub static EXTRACT_PROXIED_SERVICES: NixExtractor =
    extractor!("extract_proxied_services.nix", "extractProxiedServices");
pub static EXTRACT_NEO_THEME: NixExtractor = extractor!("extract_neo_theme.nix", "extractNeoTheme");
pub static EXTRACT_PLUGIN_INVENTORY: NixExtractor =
    extractor!("extract_plugin_inventory.nix", "extractPluginInventory");

pub static EXTRACT_SECRET_PATHS: NixExtractor =
    extractor!("extract_secret_paths.nix", "extractSecretPaths");

pub static NIX_EXTRACTORS: [&NixExtractor; 6] = [
    &EXTRACT_SERVICES,
    &EXTRACT_SERVICE_OPTIONS,
    &EXTRACT_PROXIED_SERVICES,
    &EXTRACT_NEO_THEME,
    &EXTRACT_PLUGIN_INVENTORY,
    &EXTRACT_SECRET_PATHS,
];

/// Helpers the extractors `import ./extract_lib.nix` (written, not bound).
pub static EXTRACT_LIB: (&str, &str) = ("extract_lib.nix", include_str!("extract_lib.nix"));
