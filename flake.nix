{
  description = "Homeserver Docker flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    nixos-generators.url = "github:nix-community/nixos-generators";
  };

  outputs =
    {
      self,
      nixpkgs,
      nixos-generators,
    }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      homeserver = nixos-generators.nixosGenerate {
        inherit pkgs system;
        format = "docker";
        modules = [ ./configuration.nix ];
      };
    in
    {
      packages.${system} = {
        inherit homeserver;
        default = homeserver;
      };
    };
}

