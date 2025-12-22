{
  description = "Homeserver Docker flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    nixos-generators.url = "github:nix-community/nixos-generators";
  };

  outputs = { self, nixpkgs, nixos-generators }: {
    packages.x86_64-linux.homeserver = nixos-generators.nixosGenerate {
      modules = [ ./configuration.nix ];
      format = "docker";
    };

    packages.x86_64-linux.default = self.packages.x86_64-linux.homeserver;
  };
}