let
    sources = import ./nix/sources.nix;
    nixpkgs-mozilla = import sources.nixpkgs-mozilla;
    pkgs = import sources.nixpkgs {
    overlays =
          [
            nixpkgs-mozilla
            (
              self: super:
                {
                  rustc = self.latest.rustChannels.nightly.rust;
                  cargo = self.latest.rustChannels.nightly.rust;
                }
            )
          ];
        };
    #unstable = import sources.nixpkgs-unstable {};
    naersk = pkgs.callPackage sources.naersk {};
    shellcaster = sources.shellcaster;
    src = ./shellcaster;

    nativeBuildInputs = [pkgs.ncurses6 pkgs.pkg-config pkgs.openssl pkgs.sqlite];
    buildInputs = nativeBuildInputs;

    # needs to be a function from list to list
    #cargoOptions = [ "-Z" "unstable-options" ];

    compressTarget = false;

in naersk.buildPackage {inherit src nativeBuildInputs buildInputs ;}
