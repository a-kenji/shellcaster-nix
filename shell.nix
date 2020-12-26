{
  isDevelopmentShell ? true
}:

let
  sources = import ./nix/sources.nix { };
  moz_overlay = import sources.nixpkgs-mozilla;
  pkgs = import sources.nixpkgs { overlays = [ moz_overlay ]; };
  unstable = import sources.nixpkgs-unstable { };
  rustnightly = (pkgs.latest.rustChannels.nightly.rust.override {
    extensions = [ "rust-src" "rust-analysis" "rustfmt-preview" "clippy-preview"];
  });
  nixpkgs-mozilla = import sources.nixpkgs-mozilla;
  #pkgs = import sources.nixpkgs {
   #overlays =
          #[
            #nixpkgs-mozilla
            #(
              #self: super:
                #{
                  #rustc = self.latest.rustChannels.nightly.rust;
                  #cargo = self.latest.rustChannels.nightly.rust;
                #}
            #)
          #];
        #};

  # The root directory of this project
  SHELLCASTER_ROOT = toString ./shellcaster;

  # For env_logger
  SC_LOG = "debug";

  # Keep project-specific shell commands local
  HISTFILE = "${toString ./.}/.bash_history";
  # Only in development shell

  # Needed for racer “jump to definition” editor support
  # In Emacs with `racer-mode`, you need to set
  # `racer-rust-src-path` to `nil` for it to pick
  # up the environment variable with `direnv`.
  RUST_SRC_PATH = "${pkgs.rustc.src}/lib/rustlib/x86_64-unknown-linux-gnu/lib/";
  # Set up a local directory to install binaries in
  CARGO_INSTALL_ROOT = "${SHELLCASTER_ROOT}/.cargo";

  RUST_BACKTRACE = 1;

  buildInputs = [
    pkgs.cargo
    pkgs.rustc
    pkgs.rustup
    pkgs.rustfmt
    pkgs.clippy
    unstable.rust-analyzer
    #ruststable
    rustnightly

    pkgs.niv
    pkgs.lorri
    pkgs.nixpkgs-fmt
    pkgs.git
    pkgs.direnv
    pkgs.shellcheck
  ];

    nativeBuildInputs = [pkgs.ncurses6 pkgs.pkg-config pkgs.openssl pkgs.sqlite];

in

pkgs.mkShell {
  name = "shellcaster";
  # rust analyzer seems to need them
  # as nativeBuildInputs
  src = ./shellcaster;

  inherit nativeBuildInputs;
  inherit buildInputs;

  inherit RUST_BACKTRACE;
  inherit RUST_SRC_PATH;
  inherit SC_LOG;

  shellHook = ''
      # - from lorri project -
      # we can only output to stderr in the shellHook,
      # otherwise direnv `use nix` does not work.
      # see https://github.com/direnv/direnv/issues/427
      exec 3>&1 # store stdout (1) in fd 3
      exec 1>&2 # make stdout (1) an alias for stderr (2)

      # watch the output to add the binary once it's built
      export PATH="${SHELLCASTER_ROOT}/target/debug:$PATH"

      ${pkgs.lib.optionalString isDevelopmentShell ''
      echo "shellcaster (sc)" | ${pkgs.figlet}/bin/figlet | ${pkgs.lolcat}/bin/lolcat
    ''}
      # restore stdout and close 3
      exec 1>&3-
    '';

}
