{
  description = "Homeserver Docker flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixos-generators = {
      url = "github:nix-community/nixos-generators";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self
    , nixpkgs
    , nixos-generators
    ,
    }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      homeserver = nixos-generators.nixosGenerate {
        inherit pkgs system;
        format = "docker";
        modules = [ ./configuration.nix ./settings.nix ];
      };
    in
    {
      packages.${system} = {
        inherit homeserver;
        default = homeserver;
      };
    };
}

