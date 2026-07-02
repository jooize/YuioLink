{
  description = "YuioLink - wieldy ephemeral links: every link expires";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # Pin a version with rust-bin.stable."1.85.0".default if needed.
        rustToolchain = pkgs.rust-bin.stable.latest.default;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        # Keep transcripts / scratch dirs out of the build source so editing
        # them does not bust the Nix build cache.
        src = pkgs.lib.cleanSourceWith {
          src = pkgs.lib.cleanSource ./.;
          filter = path: _type:
            let base = baseNameOf (toString path);
            in !(pkgs.lib.hasPrefix ".claude" base)
            && base != ".tmp"
            && base != ".direnv";
        };
      in
      {
        # `nix build` -> the YuioLink binaries (server/cli, as crates land).
        packages.default = rustPlatform.buildRustPackage {
          pname = "yuiolink";
          # Single source of truth: the workspace version in Cargo.toml.
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
          inherit src;
          cargoLock.lockFile = ./Cargo.lock;

          # SQLite (libsqlite3-sys) for the forthcoming server crate.
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.sqlite ];
        };

        # `nix develop` -> dev shell with the Rust toolchain + native deps.
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.pkg-config
            pkgs.sqlite
          ];
        };
      }
    );
}
