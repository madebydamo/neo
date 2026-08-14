pub struct NixExtractor {
    pub file_name: &'static str,
    pub content: &'static str,
    pub load_name: &'static str,
}

pub static NIX_EXTRACTORS: &[NixExtractor] = &[
    NixExtractor {
        file_name: "extract_services.nix",
        content: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/commands/web/nix/extract_services.nix"
        )),
        load_name: "extractServices",
    },
    NixExtractor {
        file_name: "extract_service_options.nix",
        content: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/commands/web/nix/extract_service_options.nix"
        )),
        load_name: "extractServiceOptions",
    },
    NixExtractor {
        file_name: "extract_proxied_services.nix",
        content: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/commands/web/nix/extract_proxied_services.nix"
        )),
        load_name: "extractProxiedServices",
    },
    NixExtractor {
        file_name: "extract_neo_theme.nix",
        content: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/commands/web/nix/extract_neo_theme.nix"
        )),
        load_name: "extractNeoTheme",
    },
    NixExtractor {
        file_name: "extract_plugin_inventory.nix",
        content: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/commands/web/nix/extract_plugin_inventory.nix"
        )),
        load_name: "extractPluginInventory",
    },
];

pub static EXTRACT_SERVICES: &NixExtractor = &NIX_EXTRACTORS[0];
pub static EXTRACT_SERVICE_OPTIONS: &NixExtractor = &NIX_EXTRACTORS[1];
pub static EXTRACT_PROXIED_SERVICES: &NixExtractor = &NIX_EXTRACTORS[2];
pub static EXTRACT_NEO_THEME: &NixExtractor = &NIX_EXTRACTORS[3];
pub static EXTRACT_PLUGIN_INVENTORY: &NixExtractor = &NIX_EXTRACTORS[4];
