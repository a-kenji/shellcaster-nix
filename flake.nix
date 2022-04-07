{
  description = "Shellcaster Nix Environment";
  inputs = {
    shellcaster.url = "github:jeff-hughes/shellcaster";
    shellcaster.flake = false;
    nixpkgs.url = "github:NixOS/nixpkgs/master";
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nmattia/naersk";
    naersk.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };
  outputs = {
    self,
    nixpkgs,
    flake-utils,
    naersk,
    rust-overlay,
    shellcaster,
  }:
    flake-utils.lib.eachDefaultSystem
    (
      system: let
        pkgs = import nixpkgs {inherit system overlays;};
        overlays = [
          (import rust-overlay)
          naersk.overlay
        ];
        naersk-lib = naersk.lib."${system}";

        rustToolchainToml = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain;
        cargo = rustToolchainToml;
        rustc = rustToolchainToml;

        #channel = "nightly";
        #targets = [ ];
        #date = "2021-01-15";
        #extensions = [ "rust-src" "clippy-preview" "rustfmt-preview" "rust-analyzer-preview" ];
        #rustChannelOfTargetsAndExtensions =
        #channel:
        #date:
        #targets:
        #extensions: ( pkgs.rustChannelOf { inherit channel date; } ).rust.override { inherit targets extensions; };
        #rustChan = rustChannelOfTargetsAndExtensions channel date targets extensions;
        buildInputs = [rustToolchainToml];
        fmtInputs = [pkgs.alejandra pkgs.treefmt];
        nativeBuildInputs = [pkgs.ncurses6 pkgs.pkg-config pkgs.openssl pkgs.sqlite];
        # needs to be a function from list to list
        # bundles for better nix compatibility
        #cargoOptions = opts: opts ++ [ "--features" "sqlite_bundled" ];
        RUST_BACKTRACE = 1;
      in rec {
        packages.default =
          naersk-lib.buildPackage
          {
            pname = "shellcaster";
            root = shellcaster;
            inherit nativeBuildInputs;
          };
        apps.shellcaster = flake-utils.lib.mkApp {drv = packages.shellcaster;};
        defaultApp = apps.shellcaster;
        devShells = {
          default =
            pkgs.mkShell
            {
              name = "shellcaster-env";
              inherit buildInputs;
              nativeBuildInputs = nativeBuildInputs ++ fmtInputs;
            };
          fmtShell =
            pkgs.mkShell
            {
              name = "fmtShell";
              buildInputs = fmtInputs;
            };
        };
      }
    );
}
