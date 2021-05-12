{
  description = "A very basic flake";

  inputs = {
      # To update all inputs:
      # $ nix flake update --recreate-lock-file

      shellcaster.url = "github:jeff-hughes/shellcaster";
      shellcaster.flake = false;

      #local.url = "files:./shellcaster";
      #local.flake = false;

      nixpkgs.url = "github:NixOS/nixpkgs/master";
      flake-utils.url = "github:numtide/flake-utils";
      devshell.url = "github:numtide/devshell/packages-from";

      naersk.url = "github:nmattia/naersk";
      naersk.inputs.nixpkgs.follows = "nixpkgs";

      mozilla-overlay = {
        type = "github";
        owner = "mozilla";
        repo = "nixpkgs-mozilla";
        flake = false;
      };
      #oxalica-rust-overlay.url = "github:oxalica/rust-overlay";
};
  outputs = { self, nixpkgs, flake-utils, naersk, mozilla-overlay, devshell, shellcaster}:
  flake-utils.lib.eachDefaultSystem (system:
  let
    #pkgs = nixpkgs.legacyPackages."${system}";
    pkgs = import nixpkgs {
            inherit system overlays;
            # Makes the config pure as well. See <nixpkgs>/top-level/impure.nix:
          };

    overlays = [
              (import mozilla-overlay)
              devshell.overlay

              naersk.overlay
              #self.overlay
            ];

      naersk-lib = naersk.lib."${system}";

      channel = "nightly";
      targets = [];
      date = "2021-01-15";
      extensions = ["rust-src" "clippy-preview" "rustfmt-preview" "rust-analyzer-preview"];
      rustChannelOfTargetsAndExtensions = channel: date: targets: extensions:
      (pkgs.rustChannelOf { inherit channel date; }).rust.override {
          inherit targets extensions;
        };
      rustChan = rustChannelOfTargetsAndExtensions channel date targets extensions;

    buildInputs = [
      rustChan

      pkgs.niv
      pkgs.lorri
      pkgs.direnv
      pkgs.nixpkgs-fmt
      pkgs.git
      pkgs.shellcheck
    ];

    nativeBuildInputs = [pkgs.ncurses6 pkgs.pkg-config pkgs.openssl pkgs.sqlite];

      # needs to be a function from list to list
      # bundles for better nix compatibility
      #cargoOptions = opts: opts ++ [ "--features" "sqlite_bundled" ];

      # Needed for racer “jump to definition” editor support
      # In Emacs with `racer-mode`, you need to set
      # `racer-rust-src-path` to `nil` for it to pick
      # up the environment variable with `direnv`.
      RUST_SRC_PATH = "${pkgs.rustc.src}/lib/rustlib/x86_64-unknown-linux-gnu/lib/";

      RUST_BACKTRACE = 1;

    in rec {
      # `nix build`
      packages.shellcaster = naersk-lib.buildPackage {
        pname = "shellcaster";
        root = shellcaster;
        inherit nativeBuildInputs ;
      };
      defaultPackage = packages.shellcaster ;

      # `nix run`
      apps.shellcaster = flake-utils.lib.mkApp {
        drv = packages.shellcaster;
      };
      defaultApp = apps.shellcaster;

      # `nix develop`
      #devShell = pkgs.mkDevShell {
        #packages = nativeBuildInputs ++ buildInputs;
        #env = { inherit RUST_SRC_PATH RUST_BACKTRACE;};
        #motd = "hello";
      #};
    #});
    devShell = pkgs.mkShell {
      name = "shellcaster";
      inherit nativeBuildInputs buildInputs;
    };
  });
}
